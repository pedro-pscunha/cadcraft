//! Raw association bytes remain authoritative. Fully supported semantic references can follow
//! geometry; incomplete or unsupported references stay opaque and are reported by the engine.
//! Raw data retires when links change, its owner goes, or a resolved target goes/changes type.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::dimassoc_reference;
use crate::{AssocSnap, Attrib, DimAssoc, Drawing, Entity, EntityKind, Handle};

mod point;

#[derive(Clone, Debug, PartialEq)]
enum TargetKind {
    Entity(String),
    Attribute(Handle),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawDimAssoc {
    owner_type: Option<String>,
    inferred: Vec<DimAssoc>,
    references: Vec<point::Binding>,
    targets: BTreeMap<Handle, TargetKind>,
}

fn entities(d: &Drawing) -> impl Iterator<Item = &Entity> {
    std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter().map(|e| e.as_ref()))
}

impl RawDimAssoc {
    pub(super) fn remapped(&self, map: &BTreeMap<Handle, Handle>, attribute_owners: &BTreeMap<Handle, Handle>) -> Self {
        let mut copy = self.clone();
        for reference in &mut copy.references {
            reference.remap(map);
        }
        for link in &mut copy.inferred {
            link.handle = map.get(&link.handle).copied().unwrap_or(link.handle);
            if let AssocSnap::Intersection { other } = &mut link.snap {
                *other = map.get(other).copied().unwrap_or(*other);
            }
        }
        copy.targets = self
            .targets
            .iter()
            .map(|(old, kind)| {
                let h = map.get(old).copied().unwrap_or(*old);
                let kind = match kind {
                    TargetKind::Entity(name) => TargetKind::Entity(name.clone()),
                    TargetKind::Attribute(owner) => {
                        TargetKind::Attribute(attribute_owners.get(&h).copied().or_else(|| map.get(owner).copied()).unwrap_or(*owner))
                    }
                };
                (h, kind)
            })
            .collect();
        copy
    }
}

impl Drawing {
    /// An embedded attribute instance and the entity whose transform owns it, in any store.
    pub fn attribute_instance(&self, handle: Handle) -> Option<(&Entity, &Attrib)> {
        // REFEDIT projections intentionally retain attribute identity; they are not a second
        // persistent owner of the source attribute. Resolve the original database instance.
        let projections: BTreeSet<_> = self
            .refedit
            .iter()
            .flat_map(|edit| {
                edit.items.iter().filter(|item| item.source.is_some()).map(|item| item.copy).chain(edit.ghosts.iter().map(|(copy, _)| *copy))
            })
            .collect();
        entities(self)
            .filter(|owner| !projections.contains(&owner.handle))
            .find_map(|owner| owner.kind.attributes().iter().find(|a| a.handle == Some(handle)).map(|attribute| (owner, attribute)))
    }

    /// Whether the preserved raw association, rather than an inferred subset, owns this dimension.
    pub fn has_raw_dimassoc(&self, dimension: Handle) -> bool {
        self.foreign.raw_dimassocs.contains_key(&dimension)
    }

    /// Every changed point reference must resolve against the current geometry.
    pub fn raw_dimassoc_can_update(&self, dimension: Handle, changed: impl Fn(Handle) -> bool) -> bool {
        self.raw_dimassoc_update_links(dimension, changed).is_some()
    }

    /// Preflight the whole raw record before returning any changed links. Supported semantics
    /// are retained at import, but geometric solvability is checked anew on every edit.
    pub fn raw_dimassoc_update_links(&self, dimension: Handle, changed: impl Fn(Handle) -> bool) -> Option<Vec<DimAssoc>> {
        let snapshot = self.foreign.raw_dimassocs.get(&dimension)?;
        let mut links = Vec::new();
        for reference in &snapshot.references {
            links.extend(reference.update_links(self, dimension, &changed)?);
        }
        Some(links)
    }

    /// A prior unsupported change left the preserved dimension's cached geometry stale.
    pub fn raw_dimassoc_unupdated(&self, dimension: Handle) -> bool {
        self.foreign.raw_dimassocs.get(&dimension).is_some_and(|snapshot| snapshot.references.iter().any(|reference| reference.is_pending()))
    }

    /// Record every reference left stale by an atomic failure, not only the failed one.
    pub fn mark_raw_dimassocs_unupdated(&mut self, dimensions: &BTreeSet<Handle>, changed: &BTreeSet<Handle>, invalid_vertices: &BTreeSet<Handle>) {
        if dimensions.is_empty() {
            return;
        }
        for (h, snapshot) in &mut std::sync::Arc::make_mut(&mut self.foreign).raw_dimassocs {
            if dimensions.contains(h) {
                for reference in &mut snapshot.references {
                    reference.mark_pending(changed, invalid_vertices);
                }
            }
        }
    }

    /// Clear pending state only after the entire current/pending set was evaluated and applied.
    pub fn complete_raw_dimassoc_updates(&mut self, dimensions: &BTreeSet<Handle>) {
        if dimensions.is_empty() {
            return;
        }
        for (h, snapshot) in &mut std::sync::Arc::make_mut(&mut self.foreign).raw_dimassocs {
            if dimensions.contains(h) {
                for reference in &mut snapshot.references {
                    reference.complete();
                }
            }
        }
    }

    /// Dimensions with retained raw association records.
    pub fn raw_dimassoc_dimensions(&self) -> impl Iterator<Item = Handle> + '_ {
        self.foreign.raw_dimassocs.keys().copied()
    }

    /// All reference dependencies, including unresolved source handles. Resolution guards are
    /// separate: a later-created target must not silently bind without change detection.
    pub fn raw_dimassoc_targets(&self, dimension: Handle) -> impl Iterator<Item = Handle> + '_ {
        self.foreign
            .raw_dimassocs
            .get(&dimension)
            .into_iter()
            .flat_map(|snapshot| snapshot.references.iter().flat_map(|reference| reference.targets()))
    }

    /// Capture the inference/target snapshot after reading raw DIMASSOC records.
    pub fn remember_raw_dimassocs(&mut self) {
        let records: Vec<_> = self
            .foreign
            .objects
            .iter()
            .filter(|o| o.kind == "DIMASSOC")
            .filter_map(|o| {
                let (_, dimension) = dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str())))?;
                let refs: BTreeSet<_> = o.tags.iter().filter(|t| matches!(t.code, 331 | 332)).filter_map(|t| Handle::parse_hex(&t.value)).collect();
                let references = self
                    .entity_including_blocks(dimension)
                    .map(|e| match &e.kind {
                        EntityKind::Dimension(dm) => point::bindings(self, dimension, dm, &o.tags),
                        _ => Vec::new(),
                    })
                    .unwrap_or_default();
                Some((dimension, refs, references))
            })
            .collect();
        if records.is_empty() {
            if !self.foreign.raw_dimassocs.is_empty() {
                std::sync::Arc::make_mut(&mut self.foreign).raw_dimassocs.clear();
            }
            return;
        }
        let attribute_owners: HashMap<_, _> =
            entities(self).flat_map(|e| e.kind.attributes().iter().filter_map(move |a| a.handle.map(|h| (h, e.handle)))).collect();
        let mut snapshots = BTreeMap::new();
        for (dimension, refs, references) in records {
            let owner = self.entity_including_blocks(dimension);
            let owner_type = owner.map(|e| e.kind.dxf_name().to_string());
            let inferred = owner
                .map(|e| match &e.kind {
                    EntityKind::Dimension(d) => d.assoc.clone(),
                    _ => Vec::new(),
                })
                .unwrap_or_default();
            let targets = refs
                .into_iter()
                .filter_map(|h| {
                    let kind = self
                        .entity_including_blocks(h)
                        .map(|e| TargetKind::Entity(e.kind.dxf_name().to_string()))
                        .or_else(|| attribute_owners.get(&h).map(|owner| TargetKind::Attribute(*owner)))?;
                    Some((h, kind))
                })
                .collect();
            snapshots.insert(dimension, RawDimAssoc { owner_type, inferred, references, targets });
        }
        std::sync::Arc::make_mut(&mut self.foreign).raw_dimassocs = snapshots;
    }

    /// Raw associations invalidated by model edits or loss/type change of resolved targets.
    /// Shared by the command boundary and the pure save boundary, including non-engine callers.
    pub fn stale_raw_dimassocs(&self) -> BTreeSet<Handle> {
        self.stale_raw_dimassocs_matching(|_| true)
    }

    /// Validate only the owners of an active database projection. Hidden host owners must not
    /// be interpreted as erased just because they are outside the editor's model store.
    pub fn stale_raw_dimassocs_for(&self, owners: &BTreeSet<Handle>) -> BTreeSet<Handle> {
        self.stale_raw_dimassocs_matching(|h| owners.contains(&h))
    }

    fn stale_raw_dimassocs_matching(&self, includes: impl Fn(Handle) -> bool) -> BTreeSet<Handle> {
        self.foreign
            .raw_dimassocs
            .iter()
            .filter(|(h, _)| includes(**h))
            .filter_map(|(h, snapshot)| {
                let valid = self.entity_including_blocks(*h).is_some_and(|owner| {
                    let inferred = match &owner.kind {
                        EntityKind::Dimension(d) => d.assoc.as_slice(),
                        _ => &[],
                    };
                    snapshot.owner_type.as_deref().is_some_and(|name| owner.kind.dxf_name().eq_ignore_ascii_case(name))
                        && inferred == snapshot.inferred
                        && snapshot.targets.iter().all(|(target, kind)| match kind {
                            TargetKind::Entity(name) => {
                                self.entity_including_blocks(*target).is_some_and(|e| e.kind.dxf_name().eq_ignore_ascii_case(name))
                            }
                            TargetKind::Attribute(owner) => self
                                .entity_including_blocks(*owner)
                                .is_some_and(|entity| entity.kind.attributes().iter().any(|a| a.handle == Some(*target))),
                        })
                });
                (!valid).then_some(*h)
            })
            .collect()
    }
}

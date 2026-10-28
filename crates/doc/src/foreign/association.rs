//! Opaque raw associations have one authority. Inferred model links are a guard snapshot, not
//! a partially usable association: the engine leaves their cached geometry alone. Raw data is
//! retired when those links change, its owner goes, or an initially resolved target goes/changes
//! type. Originally unresolved input references remain opaque rather than being guessed valid.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::dimassoc_reference;
use crate::{AssocSnap, DimAssoc, Drawing, Entity, EntityKind, Handle};

#[derive(Clone, Debug, PartialEq)]
enum TargetKind {
    Entity(String),
    Attribute(Handle),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawDimAssoc {
    owner_type: String,
    inferred: Vec<DimAssoc>,
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
    /// Whether the preserved raw association, rather than an inferred subset, owns this dimension.
    pub fn has_raw_dimassoc(&self, dimension: Handle) -> bool {
        self.foreign.raw_dimassocs.contains_key(&dimension)
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
                Some((dimension, refs))
            })
            .collect();
        if records.is_empty() {
            return;
        }
        let attribute_owners: HashMap<_, _> =
            entities(self).flat_map(|e| e.kind.attributes().iter().filter_map(move |a| a.handle.map(|h| (h, e.handle)))).collect();
        let mut snapshots = BTreeMap::new();
        for (dimension, refs) in records {
            let Some(owner) = self.entity_including_blocks(dimension) else { continue };
            let inferred = match &owner.kind {
                EntityKind::Dimension(d) => d.assoc.clone(),
                _ => Vec::new(),
            };
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
            snapshots.insert(dimension, RawDimAssoc { owner_type: owner.kind.dxf_name().to_string(), inferred, targets });
        }
        std::sync::Arc::make_mut(&mut self.foreign).raw_dimassocs = snapshots;
    }

    /// Raw associations invalidated by model edits or loss/type change of resolved targets.
    /// Shared by the command boundary and the pure save boundary, including non-engine callers.
    pub fn stale_raw_dimassocs(&self) -> BTreeSet<Handle> {
        self.foreign
            .raw_dimassocs
            .iter()
            .filter_map(|(h, snapshot)| {
                let valid = self.entity_including_blocks(*h).is_some_and(|owner| {
                    let inferred = match &owner.kind {
                        EntityKind::Dimension(d) => d.assoc.as_slice(),
                        _ => &[],
                    };
                    owner.kind.dxf_name().eq_ignore_ascii_case(&snapshot.owner_type)
                        && inferred == snapshot.inferred
                        && snapshot.targets.iter().all(|(target, kind)| match kind {
                            TargetKind::Entity(name) => {
                                self.entity_including_blocks(*target).is_some_and(|e| e.kind.dxf_name().eq_ignore_ascii_case(name))
                            }
                            TargetKind::Attribute(owner) => {
                                self.entity_including_blocks(*owner).is_some_and(|e| e.kind.attributes().iter().any(|a| a.handle == Some(*target)))
                            }
                        })
                });
                (!valid).then_some(*h)
            })
            .collect()
    }
}

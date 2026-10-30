//! The document allocator owns entity and attribute-instance identities.

use crate::{Attrib, Drawing, EntityKind, EntityStore, Handle};

pub(crate) fn allocate_handle(next: &mut u64) -> Handle {
    let h = Handle(*next);
    *next = next.saturating_add(1);
    h
}

impl EntityKind {
    pub fn polyline_records(&self) -> impl Iterator<Item = &crate::PolylineRecord> {
        let (vertices, fit, mesh, seqend) = match self {
            Self::LwPolyline(p) => {
                (if p.saved_fit().is_some() { &[][..] } else { p.records.vertices.as_slice() }, p.saved_fit(), None, p.records.seqend.as_ref())
            }
            Self::Polyline3d(p) => (p.records.vertices.as_slice(), None, None, p.records.seqend.as_ref()),
            Self::PolylineMesh(mesh) => (&[][..], None, Some(mesh), mesh.seqend.as_ref()),
            _ => (&[][..], None, None, None),
        };
        fit.into_iter()
            .flat_map(|f| f.vertices.iter().map(|v| &v.record))
            .chain(vertices)
            .chain(mesh.into_iter().flat_map(|m| m.vertices.iter().map(|v| &v.vertex.record)))
            .chain(seqend)
    }

    pub fn edit_polyline_records(&mut self, mut edit: impl FnMut(&mut crate::PolylineRecord)) {
        let records = match self {
            Self::LwPolyline(p) => {
                if let Some(fit) = &mut p.fit {
                    for vertex in &mut fit.vertices {
                        edit(&mut vertex.record);
                    }
                }
                &mut p.records
            }
            Self::Polyline3d(p) => &mut p.records,
            Self::PolylineMesh(mesh) => {
                for vertex in &mut mesh.vertices {
                    edit(&mut vertex.vertex.record);
                }
                if let Some(end) = &mut mesh.seqend {
                    edit(end);
                }
                return;
            }
            _ => return,
        };
        for record in &mut records.vertices {
            edit(record);
        }
        if let Some(end) = &mut records.seqend {
            edit(end);
        }
    }

    /// The handles this entity holds beside its own: those of its attributes and of its polyline vertex and SEQEND records.
    fn owned_handles_mut(&mut self) -> Vec<&mut Handle> {
        let mut out: Vec<&mut Handle> = Vec::new();
        match self {
            Self::Insert(i) => out.extend(i.attribs.iter_mut().filter_map(|a| a.handle.as_mut())),
            Self::MLeader(m) => {
                out.extend(m.block.iter_mut().flat_map(|i| i.attribs.iter_mut()).filter_map(|a| a.handle.as_mut()));
            }
            Self::LwPolyline(p) => {
                if let Some(fit) = &mut p.fit {
                    out.extend(fit.vertices.iter_mut().filter_map(|v| v.record.handle.as_mut()));
                }
                out.extend(p.records.vertices.iter_mut().chain(p.records.seqend.as_mut()).filter_map(|r| r.handle.as_mut()));
            }
            Self::Polyline3d(p) => out.extend(p.records.vertices.iter_mut().chain(p.records.seqend.as_mut()).filter_map(|r| r.handle.as_mut())),
            Self::PolylineMesh(m) => {
                out.extend(m.vertices.iter_mut().filter_map(|v| v.vertex.record.handle.as_mut()));
                out.extend(m.seqend.as_mut().and_then(|r| r.handle.as_mut()));
            }
            _ => {}
        }
        out
    }

    /// Attribute instances owned by this entity (including multileader block content).
    pub fn attributes(&self) -> &[Attrib] {
        match self {
            Self::Insert(i) => &i.attribs,
            Self::MLeader(m) => m.block.as_ref().map_or(&[], |i| i.attribs.as_slice()),
            _ => &[],
        }
    }

    /// The same instances for identity allocation and instance edits; ATTDEFs are definitions.
    pub fn attributes_mut(&mut self) -> &mut [Attrib] {
        match self {
            Self::Insert(i) => &mut i.attribs,
            Self::MLeader(m) => m.block.as_mut().map_or(&mut [], |i| i.attribs.as_mut_slice()),
            _ => &mut [],
        }
    }
}

fn fill(kind: &mut EntityKind, allocate: &mut impl FnMut() -> Handle) {
    if let EntityKind::AttDef(definition) = kind {
        definition.handle = None;
    }
    for attribute in kind.attributes_mut() {
        if attribute.handle.is_none() {
            attribute.handle = Some(allocate());
        }
    }
}

fn fill_store(store: &mut EntityStore, allocate: &mut impl FnMut() -> Handle) {
    let missing: Vec<_> = store.iter().filter(|e| e.kind.attributes().iter().any(|a| a.handle.is_none())).map(|e| e.as_ref().clone()).collect();
    for mut entity in missing {
        fill(&mut entity.kind, allocate);
        store.replace(entity);
    }
}

impl Drawing {
    /// Check completion without copy-on-write: command exit must not clone a restored clean
    /// drawing merely to discover that every instance already has its identity.
    pub fn has_missing_attribute_handles(&self) -> bool {
        std::iter::once(&self.model)
            .chain(self.layouts.iter().map(|layout| &layout.entities))
            .chain(self.blocks.values().map(|block| &block.entities))
            .flat_map(|store| store.iter())
            .any(|entity| entity.kind.attributes().iter().any(|attribute| attribute.handle.is_none()))
    }

    /// Allocate identities for newly created attribute instances before they enter the database.
    pub fn allocate_attribute_handles(&mut self, kind: &mut EntityKind) {
        fill(kind, &mut || self.new_handle());
    }

    /// Finish imports and synchronization by allocating any newly introduced instances.
    pub fn allocate_missing_attribute_handles(&mut self) {
        let mut allocate = || allocate_handle(&mut self.handseed);
        fill_store(&mut self.model, &mut allocate);
        for layout in &mut self.layouts {
            fill_store(&mut layout.entities, &mut allocate);
        }
        for block in self.blocks.values_mut() {
            if block.entities.iter().any(|e| e.kind.attributes().iter().any(|a| a.handle.is_none())) {
                fill_store(&mut std::sync::Arc::make_mut(block).entities, &mut allocate);
            }
        }
    }
}

/// A handle two objects held, and the fresh one the later object got (see [`Drawing::with_unique_handles`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleRepair {
    pub old: Handle,
    pub new: Handle,
    /// Where the first holder, which keeps the handle, is: `model space`, `layout NAME` or `block NAME`.
    pub kept_in: String,
    /// Where the later holder, which got `new`, is.
    pub moved_in: String,
}

/// The handles of an entity: its own, those of its attributes, and those of its polyline vertex records.
fn object_handles(e: &mut crate::Entity) -> Vec<&mut Handle> {
    // Records with no handle yet get one when they are written.
    let mut out: Vec<&mut Handle> = vec![&mut e.handle];
    out.extend(e.kind.owned_handles_mut());
    out
}

impl Drawing {
    /// This drawing with no handle held twice, and what changed. A file names each object by its handle, so two entities with
    /// one handle (for example a block's entity and a model entity) make one of them unreadable: the DWG conversion drops it.
    /// The first holder in file order (model space, then the layouts, then the blocks) keeps the handle; every later holder gets a
    /// fresh one. `None` when no handle is held twice. A reference to a repeated handle names the first holder.
    pub fn with_unique_handles(&self) -> Option<(Drawing, Vec<HandleRepair>)> {
        let stores = || {
            std::iter::once(("model space".to_string(), &self.model))
                .chain(self.layouts.iter().map(|l| (format!("layout {}", l.name), &l.entities)))
                .chain(self.blocks.iter().map(|(n, b)| (format!("block {n}"), &b.entities)))
        };
        let mut all = std::collections::HashSet::new();
        let mut repeated = false;
        for (_, store) in stores() {
            for e in store.iter() {
                let mut e = e.as_ref().clone();
                repeated |= object_handles(&mut e).into_iter().any(|h| !all.insert(*h));
            }
        }
        if !repeated {
            return None;
        }
        let mut out = self.clone();
        let mut next = all.iter().map(|h| h.0).max().unwrap_or(0).max(self.handseed).saturating_add(1);
        let mut owners: std::collections::HashMap<Handle, String> = std::collections::HashMap::new();
        let mut repairs = Vec::new();
        let rebuild = |name: &str,
                       store: &EntityStore,
                       owners: &mut std::collections::HashMap<Handle, String>,
                       next: &mut u64,
                       repairs: &mut Vec<HandleRepair>| {
            let mut rebuilt = EntityStore::new();
            for e in store.iter() {
                let mut e = e.as_ref().clone();
                for h in object_handles(&mut e) {
                    if let Some(kept_in) = owners.get(h).cloned() {
                        let new = Handle(*next);
                        *next += 1;
                        repairs.push(HandleRepair { old: *h, new, kept_in, moved_in: name.to_string() });
                        *h = new;
                    }
                    owners.insert(*h, name.to_string());
                }
                rebuilt.push(e);
            }
            rebuilt
        };
        out.model = rebuild("model space", &self.model, &mut owners, &mut next, &mut repairs);
        for (i, layout) in self.layouts.iter().enumerate() {
            if let Some(l) = out.layouts.get_mut(i) {
                l.entities = rebuild(&format!("layout {}", layout.name), &layout.entities, &mut owners, &mut next, &mut repairs);
            }
        }
        for (name, block) in &self.blocks {
            if let Some(b) = out.blocks.get_mut(name) {
                std::sync::Arc::make_mut(b).entities = rebuild(&format!("block {name}"), &block.entities, &mut owners, &mut next, &mut repairs);
            }
        }
        out.handseed = out.handseed.max(next);
        Some((out, repairs))
    }
}

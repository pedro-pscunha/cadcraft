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

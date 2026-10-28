//! The document allocator owns entity and attribute-instance identities.

use crate::{Attrib, Drawing, EntityKind, EntityStore, Handle};

pub(crate) fn allocate_handle(next: &mut u64) -> Handle {
    let h = Handle(*next);
    *next = next.saturating_add(1);
    h
}

impl EntityKind {
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
    let hs: Vec<_> = store.iter().filter(|e| e.kind.attributes().iter().any(|a| a.handle.is_none())).map(|e| e.handle).collect();
    for h in hs {
        store.modify(h, |e| fill(&mut e.kind, allocate));
    }
}

impl Drawing {
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

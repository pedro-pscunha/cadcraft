//! Merge aliases apply to surviving references, independently of metadata inheritance.
use super::{ForeignExtra, is_pointer, owned_indices, owner_index, remap_tags};
use crate::{AssocSnap, Drawing, EntityKind, EntityStore, Handle, RawTag};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn references(tags: &[RawTag], map: &BTreeMap<Handle, Handle>) -> bool {
    tags.iter().any(|tag| is_pointer(tag.code) && Handle::parse_hex(&tag.value).is_some_and(|h| map.contains_key(&h)))
}
fn extra_references(extra: &ForeignExtra, map: &BTreeMap<Handle, Handle>) -> bool {
    extra.xdict.is_some_and(|h| map.contains_key(&h))
        || extra.reactors.iter().any(|h| map.contains_key(h))
        || references(&extra.xdata, map)
        || references(&extra.props, map)
}
fn kind_references(kind: &EntityKind, map: &BTreeMap<Handle, Handle>) -> bool {
    match kind {
        EntityKind::Dimension(dm) => dm
            .assoc
            .iter()
            .any(|link| map.contains_key(&link.handle) || matches!(link.snap,AssocSnap::Intersection{other} if map.contains_key(&other))),
        EntityKind::Hatch(h) => h.loops.iter().any(|boundary| boundary.sources.iter().any(|h| map.contains_key(h))),
        EntityKind::Viewport(v) => v.clip.is_some_and(|h| map.contains_key(&h)),
        EntityKind::Unknown(u) => references(&u.tags, map),
        _ => false,
    }
}
fn remap_kind(kind: &mut EntityKind, map: &BTreeMap<Handle, Handle>) {
    match kind {
        EntityKind::Dimension(dm) => {
            for link in &mut dm.assoc {
                link.handle = map.get(&link.handle).copied().unwrap_or(link.handle);
                if let AssocSnap::Intersection { other } = &mut link.snap {
                    *other = map.get(other).copied().unwrap_or(*other);
                }
            }
        }
        EntityKind::Hatch(h) => {
            for boundary in &mut h.loops {
                for h in &mut boundary.sources {
                    *h = map.get(h).copied().unwrap_or(*h);
                }
            }
        }
        EntityKind::Viewport(v) => v.clip = v.clip.map(|h| map.get(&h).copied().unwrap_or(h)),
        EntityKind::Unknown(u) => remap_tags(&mut u.tags, map),
        _ => {}
    }
}
fn remap_store(store: &mut EntityStore, map: &BTreeMap<Handle, Handle>) {
    let changed: Vec<_> = store.iter().filter(|e| references(&e.common.xdata, map) || kind_references(&e.kind, map)).map(|e| e.handle).collect();
    for h in changed {
        store.modify(h, |e| {
            remap_tags(&mut e.common.xdata, map);
            remap_kind(&mut e.kind, map);
        });
    }
}

impl Drawing {
    /// Complete a merge after result metadata was selected: discard metadata owned by erased
    /// inputs, then retarget every surviving pointer to any consumed identity to the result.
    pub fn remap_merged_references(&mut self, consumed: &[Handle], result: Handle) {
        let map: BTreeMap<_, _> = consumed.iter().filter(|h| **h != result).map(|h| (*h, result)).collect();
        if map.is_empty() {
            return;
        }
        remap_store(&mut self.model, &map);
        for layout in &mut self.layouts {
            remap_store(&mut layout.entities, &map);
        }
        for block in self.blocks.values_mut() {
            if block.entities.iter().any(|e| references(&e.common.xdata, &map) || kind_references(&e.kind, &map)) {
                remap_store(&mut Arc::make_mut(block).entities, &map);
            }
        }
        for group in &mut self.groups {
            let mut seen = BTreeSet::new();
            group.members = group.members.iter().map(|h| map.get(h).copied().unwrap_or(*h)).filter(|h| seen.insert(*h)).collect();
        }
        let index = owner_index(&self.foreign.objects);
        let removed: BTreeSet<_> = owned_indices(&self.foreign.objects, &index, map.keys().copied())
            .into_iter()
            .filter_map(|i| self.foreign.objects.get(i).map(|o| o.handle))
            .collect();
        let changes_foreign = !removed.is_empty()
            || self.foreign.entities.keys().any(|h| map.contains_key(h))
            || self.foreign.objects.iter().any(|o| references(&o.tags, &map))
            || self.foreign.entities.values().chain(self.foreign.records.values()).any(|e| extra_references(e, &map))
            || self.foreign.dictionary.iter().any(|(_, h)| map.contains_key(h))
            || references(&self.foreign.data_store, &map);
        if !changes_foreign {
            return;
        }
        let foreign = Arc::make_mut(&mut self.foreign);
        foreign.objects.retain(|object| !removed.contains(&object.handle));
        foreign.entities.retain(|h, _| !map.contains_key(h));
        foreign.raw_dimassocs.retain(|h, _| !map.contains_key(h));
        for object in &mut foreign.objects {
            if references(&object.tags, &map) {
                remap_tags(&mut object.tags, &map);
            }
        }
        for extra in foreign.entities.values_mut().chain(foreign.records.values_mut()) {
            extra.xdict = extra.xdict.map(|h| map.get(&h).copied().unwrap_or(h));
            for h in &mut extra.reactors {
                *h = map.get(h).copied().unwrap_or(*h);
            }
            remap_tags(&mut extra.xdata, &map);
            remap_tags(&mut extra.props, &map);
        }
        for (_, h) in &mut foreign.dictionary {
            *h = map.get(h).copied().unwrap_or(*h);
        }
        foreign.dictionary.retain(|(_, h)| !removed.contains(h));
        remap_tags(&mut foreign.data_store, &map);
        for snapshot in foreign.raw_dimassocs.values_mut() {
            *snapshot = snapshot.remapped(&map, &BTreeMap::new());
        }
    }
}

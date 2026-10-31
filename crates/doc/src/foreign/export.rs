//! Keep the metadata graph of an exported selection, not the source drawing's unrelated data.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{
    ForeignExtra, ForeignKey, ForeignObject, is_dictionary, is_pointer, object_owner, owner_index, remap_data_store, remap_model, remap_tags,
    strip_dictionary_entries,
};
use crate::{Drawing, DuplicationSource, Entity, EntityKind, EntityStore, Handle, RawTag};

fn role(kind: &str, name: &str) -> ForeignKey {
    (kind.into(), name.to_ascii_uppercase())
}

fn pointers(tags: &[RawTag]) -> impl Iterator<Item = Handle> + '_ {
    tags.iter().filter(|t| is_pointer(t.code)).filter_map(|t| Handle::parse_hex(&t.value)).filter(|h| h.0 != 0)
}

fn extra_pointers(extra: &ForeignExtra) -> impl Iterator<Item = Handle> + '_ {
    extra
        .xdict
        .into_iter()
        .chain(extra.reactors.iter().copied())
        .chain(pointers(&extra.xdata))
        .chain(pointers(&extra.props))
        .chain(pointers(&extra.subclass))
        .chain(pointers(&extra.layout))
}

fn object_pointers(object: &ForeignObject, full: bool) -> Vec<Handle> {
    let mut result = Vec::new();
    let mut grouped = false;
    let mut owner_seen = false;
    let mut entry = false;
    for tag in &object.tags {
        if tag.code == 102 {
            grouped = tag.value.trim().starts_with('{');
        }
        if tag.code == 330 && !grouped && !owner_seen {
            owner_seen = true;
            continue;
        }
        // A structural parent dictionary does not make its other entries dependencies.
        if !full && is_dictionary(&object.kind) {
            if tag.code == 3 {
                entry = true;
                continue;
            }
            if entry && matches!(tag.code, 350 | 360) {
                entry = false;
                continue;
            }
            entry = false;
        }
        if is_pointer(tag.code)
            && let Some(h) = Handle::parse_hex(&tag.value).filter(|h| h.0 != 0)
        {
            result.push(h);
        }
    }
    result
}

impl Drawing {
    /// Prepare a fresh namespace with the selection's owned metadata and referenced objects.
    pub fn prepare_export(&mut self, block: Option<&str>) {
        let source = &self.foreign;
        let entities: Vec<_> = std::iter::once(&self.model).chain(self.blocks.values().map(|b| &b.entities)).flat_map(|store| store.iter()).collect();
        let entity_roots: BTreeSet<_> = entities
            .iter()
            .flat_map(|e| {
                std::iter::once(e.handle)
                    .chain(e.kind.attributes().iter().filter_map(|a| a.handle))
                    .chain(e.kind.polyline_records().filter_map(|r| r.handle))
            })
            .collect();
        let mut roles = BTreeSet::new();
        roles.extend(self.layers.iter().map(|v| role("LAYER", &v.name)));
        roles.extend(self.linetypes.iter().map(|v| role("LTYPE", &v.name)));
        roles.extend(self.text_styles.iter().map(|v| role("STYLE", &v.name)));
        roles.extend(self.dim_styles.iter().map(|v| role("DIMSTYLE", &v.name)));
        roles.extend(self.mleader_styles.iter().map(|v| role("NOD/ACAD_MLEADERSTYLE", &v.name)));
        roles.extend(self.mline_styles.iter().map(|v| role("NOD/ACAD_MLINESTYLE", &v.name)));
        roles.extend(self.table_styles.iter().map(|v| role("NOD/ACAD_TABLESTYLE", &v.name)));
        roles.extend(self.image_definitions.iter().map(|v| role("NOD/ACAD_IMAGE_DICT", &v.name)));
        for name in self.blocks.keys().map(String::as_str).chain(block) {
            for kind in ["BLOCK", "ENDBLK", "BLOCK_RECORD"] {
                roles.insert(role(kind, name));
            }
        }
        let styles = entities
            .iter()
            .map(|e| e.common.plot_style.as_str())
            .chain(self.layers.iter().map(|l| l.plot_style.as_str()))
            .chain(entities.iter().flat_map(|e| e.kind.attributes().iter().filter_map(|a| a.props.plot_style.as_deref())))
            .chain(entities.iter().flat_map(|e| e.kind.polyline_records().map(|r| r.common.plot_style.as_str())));
        roles.extend(styles.map(|name| role("NOD/ACAD_PLOTSTYLENAME", name)));
        let objects: BTreeMap<_, _> = source.objects.iter().chain(&source.table_records).map(|o| (o.handle, o)).collect();
        let owned = owner_index(&source.objects);
        let mut pending: Vec<_> = entity_roots.iter().map(|h| (*h, true)).collect();
        pending.extend(source.targets.iter().filter(|(_, r)| roles.contains(*r)).map(|(h, _)| (*h, true)));
        for entity in &entities {
            pending.extend(pointers(&entity.common.xdata).map(|h| (h, true)));
            if let EntityKind::Unknown(unknown) = &entity.kind {
                pending.extend(pointers(&unknown.tags).map(|h| (h, true)));
            }
        }
        let mut scope = BTreeMap::new();
        while let Some((handle, full)) = pending.pop() {
            if scope.get(&handle).is_some_and(|previous| *previous || !full) {
                continue;
            }
            scope.insert(handle, full);
            if full {
                if let Some(extra) = source.entities.get(&handle).filter(|_| entity_roots.contains(&handle)) {
                    pending.extend(extra_pointers(extra).map(|h| (h, true)));
                }
                if let Some(key) = source.targets.get(&handle) {
                    roles.insert(key.clone());
                    if let Some(extra) = source.records.get(key) {
                        pending.extend(extra_pointers(extra).map(|h| (h, true)));
                    }
                }
                if entity_roots.contains(&handle) || objects.contains_key(&handle) {
                    for index in owned.get(&handle).into_iter().flatten() {
                        if let Some(child) = source.objects.get(*index) {
                            pending.push((child.handle, true));
                        }
                    }
                }
            }
            if let Some(object) = objects.get(&handle) {
                pending.extend(object_pointers(object, full).into_iter().map(|h| (h, true)));
                if let Some(owner) = object_owner(object.tags.iter().map(|t| (t.code, t.value.as_str()))) {
                    pending.push((owner, false));
                }
            }
        }
        let mut exported = (**source).clone();
        exported.objects.retain(|o| scope.contains_key(&o.handle));
        exported.table_records.retain(|o| scope.contains_key(&o.handle));
        for object in exported.objects.iter_mut().chain(&mut exported.table_records) {
            if scope.get(&object.handle) == Some(&false) && is_dictionary(&object.kind) {
                strip_dictionary_entries(&mut object.tags, |_, target| Handle::parse_hex(target).is_none_or(|h| !scope.contains_key(&h)));
            }
        }
        exported.dictionary.retain(|(_, h)| scope.contains_key(h));
        exported.entities.retain(|h, _| entity_roots.contains(h));
        exported.raw_dimassocs.retain(|h, _| entity_roots.contains(h));
        exported.records.retain(|key, _| roles.contains(key));
        self.foreign = Arc::new(exported);
        self.reidentify_export();
    }

    fn reidentify_export(&mut self) {
        let model_count = self.model.len();
        let counts: Vec<_> = self.blocks.values().map(|b| b.entities.len()).collect();
        let mut entities: Vec<Entity> =
            self.model.iter().chain(self.blocks.values().flat_map(|b| b.entities.iter())).map(|e| (**e).clone()).collect();
        let mut map = self.allocate_copy_roots(&mut entities);
        let handles: Vec<_> = self.foreign.objects.iter().chain(&self.foreign.table_records).map(|o| o.handle).collect();
        for old in handles {
            map.entry(old).or_insert_with(|| self.new_handle());
        }
        for entity in &mut entities {
            remap_model(&mut entity.kind, &map, DuplicationSource::SameDrawing, false);
            remap_tags(&mut entity.common.xdata, &map);
        }
        let owners = entities.iter().flat_map(|e| e.kind.attributes().iter().filter_map(move |a| a.handle.map(|h| (h, e.handle)))).collect();
        let mut copies = entities.into_iter();
        self.model = EntityStore::new();
        for entity in copies.by_ref().take(model_count) {
            self.model.push(entity);
        }
        for (block, count) in self.blocks.values_mut().zip(counts) {
            let block = Arc::make_mut(block);
            block.entities = EntityStore::new();
            for entity in copies.by_ref().take(count) {
                block.entities.push(entity);
            }
        }
        let foreign = Arc::make_mut(&mut self.foreign);
        for object in foreign.objects.iter_mut().chain(&mut foreign.table_records) {
            object.handle = map.get(&object.handle).copied().unwrap_or(object.handle);
            remap_tags(&mut object.tags, &map);
        }
        foreign.entities = std::mem::take(&mut foreign.entities)
            .into_iter()
            .map(|(h, mut e)| {
                remap_export_extra(&mut e, &map);
                (map.get(&h).copied().unwrap_or(h), e)
            })
            .collect();
        for extra in foreign.records.values_mut() {
            remap_export_extra(extra, &map);
        }
        for (_, handle) in &mut foreign.dictionary {
            *handle = map.get(handle).copied().unwrap_or(*handle);
        }
        foreign.targets = std::mem::take(&mut foreign.targets).into_iter().map(|(h, key)| (map.get(&h).copied().unwrap_or(h), key)).collect();
        foreign.raw_dimassocs = std::mem::take(&mut foreign.raw_dimassocs)
            .into_iter()
            .map(|(h, snapshot)| (map.get(&h).copied().unwrap_or(h), snapshot.remapped(&map, &owners)))
            .collect();
        remap_data_store(&mut foreign.data_store, &map);
    }
}

fn remap_export_extra(extra: &mut ForeignExtra, map: &BTreeMap<Handle, Handle>) {
    extra.xdict = extra.xdict.map(|h| map.get(&h).copied().unwrap_or(h));
    for handle in &mut extra.reactors {
        *handle = map.get(handle).copied().unwrap_or(*handle);
    }
    remap_tags(&mut extra.xdata, map);
    remap_tags(&mut extra.props, map);
    remap_tags(&mut extra.subclass, map);
    remap_tags(&mut extra.layout, map);
}

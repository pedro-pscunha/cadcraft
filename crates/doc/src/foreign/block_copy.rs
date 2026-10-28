//! Copy one block-edit namespace and restore its original definition's metadata from the host.
use super::{DuplicationSource, Foreign, ForeignKey, owned_indices, owner_index};
use crate::{Drawing, Entity, Handle};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// A fresh block-content copy and the identities needed to retarget editor selections/cursors.
pub struct BlockContentCopy {
    pub entities: Vec<Entity>,
    pub handles: BTreeMap<Handle, Handle>,
}

pub(super) struct BlockNamespace {
    pub(super) source: String,
    pub(super) target: String,
    pub(super) handles: BTreeMap<Handle, Handle>,
    pub(super) source_owned: BTreeSet<Handle>,
}

pub(super) fn block_role(key: &ForeignKey, name: &str) -> bool {
    matches!(key.0.as_str(), "BLOCK" | "BLOCK_RECORD" | "ENDBLK") && key.1.eq_ignore_ascii_case(name)
}

impl BlockNamespace {
    pub(super) fn close_map(copy: Option<&Self>, map: &mut std::borrow::Cow<'_, BTreeMap<Handle, Handle>>) {
        if let Some(copy) = copy {
            for old in &copy.source_owned {
                map.to_mut().entry(*old).or_insert(Handle(0));
            }
        }
    }

    pub(super) fn capture(copy: Option<&mut Self>, map: &BTreeMap<Handle, Handle>) {
        if let Some(copy) = copy {
            copy.handles.extend(map.iter().filter(|(_, new)| new.0 != 0).map(|(old, new)| (*old, *new)));
        }
    }

    pub(super) fn target_key(&self, key: &ForeignKey) -> ForeignKey {
        (key.0.clone(), self.target.to_ascii_uppercase())
    }
}

fn roots<'a>(entities: impl IntoIterator<Item = &'a Entity>) -> BTreeSet<Handle> {
    let mut roots = BTreeSet::new();
    for entity in entities {
        roots.insert(entity.handle);
        roots.extend(entity.kind.attributes().iter().filter_map(|attribute| attribute.handle));
    }
    roots
}
fn role_roots(foreign: &Foreign, name: &str) -> BTreeSet<Handle> {
    foreign.targets.iter().filter(|(_, key)| block_role(key, name)).map(|(h, _)| *h).collect()
}
fn namespace(foreign: &Foreign, roots: &BTreeSet<Handle>) -> BTreeSet<Handle> {
    let index = owner_index(&foreign.objects);
    roots
        .iter()
        .copied()
        .chain(owned_indices(&foreign.objects, &index, roots.iter().copied()).into_iter().filter_map(|i| foreign.objects.get(i).map(|o| o.handle)))
        .collect()
}

impl Drawing {
    /// Duplicate active block contents with the canonical remapper, including name-keyed
    /// record extras. The original definition keeps the host's metadata, not unsaved edits.
    pub fn duplicate_block_content(&mut self, entities: Vec<Entity>, source: &str, target: &str, host: &Drawing) -> crate::Result<BlockContentCopy> {
        if source.eq_ignore_ascii_case(target) {
            return Err(crate::DocError::Invalid("a block copy needs a different target name".into()));
        }
        let mut source_roots = roots(entities.iter());
        if let Some(block) = host.block(source) {
            source_roots.extend(roots(block.entities.iter().map(|entity| entity.as_ref())));
        }
        source_roots.extend(role_roots(&self.foreign, source));
        source_roots.extend(role_roots(&host.foreign, source));
        let mut original_namespace = namespace(&self.foreign, &source_roots);
        original_namespace.extend(namespace(&host.foreign, &source_roots));
        // Redefining an existing target replaces its named metadata too, not only its entities.
        let target_roles = role_roots(&self.foreign, target);
        let target_namespace = namespace(&self.foreign, &target_roles);
        if !target_namespace.is_empty() || self.foreign.records.keys().any(|key| block_role(key, target)) {
            let foreign = Arc::make_mut(&mut self.foreign);
            foreign.objects.retain(|o| !target_namespace.contains(&o.handle));
            foreign.records.retain(|key, _| !block_role(key, target));
        }
        let mut copy = BlockNamespace {
            source: source.to_string(),
            target: target.to_string(),
            handles: BTreeMap::new(),
            source_owned: original_namespace.clone(),
        };
        let entities = self.duplicate_entity_sets(vec![entities], DuplicationSource::SameDrawing, &BTreeMap::new(), Some(&mut copy));
        let foreign = Arc::make_mut(&mut self.foreign);
        foreign.objects.retain(|o| !original_namespace.contains(&o.handle));
        foreign.objects.extend(host.foreign.objects.iter().filter(|o| original_namespace.contains(&o.handle)).cloned());
        foreign.entities.retain(|h, _| !source_roots.contains(h));
        foreign.entities.extend(host.foreign.entities.iter().filter(|(h, _)| source_roots.contains(h)).map(|(h, e)| (*h, e.clone())));
        foreign.records.retain(|key, _| !block_role(key, source));
        foreign.records.extend(host.foreign.records.iter().filter(|(key, _)| block_role(key, source)).map(|(key, e)| (key.clone(), e.clone())));
        foreign.targets.retain(|h, _| !original_namespace.contains(h));
        foreign.targets.extend(host.foreign.targets.iter().filter(|(h, _)| original_namespace.contains(h)).map(|(h, key)| (*h, key.clone())));
        foreign.raw_dimassocs.retain(|h, _| !source_roots.contains(h));
        foreign
            .raw_dimassocs
            .extend(host.foreign.raw_dimassocs.iter().filter(|(h, _)| source_roots.contains(h)).map(|(h, snapshot)| (*h, snapshot.clone())));
        Ok(BlockContentCopy { entities, handles: copy.handles })
    }

    /// A rename moves named roles without cloning record identities or owned object graphs.
    pub fn rename_block_foreign_records(&mut self, source: &str, target: &str) {
        if !self.foreign.records.keys().any(|key| block_role(key, source)) && !self.foreign.targets.values().any(|key| block_role(key, source)) {
            return;
        }
        let foreign = Arc::make_mut(&mut self.foreign);
        let keys: Vec<_> = foreign.records.keys().filter(|key| block_role(key, source)).cloned().collect();
        for key in keys {
            if let Some(extra) = foreign.records.remove(&key) {
                foreign.records.insert((key.0, target.to_ascii_uppercase()), extra);
            }
        }
        for key in foreign.targets.values_mut().filter(|key| block_role(key, source)) {
            key.1 = target.to_ascii_uppercase();
        }
    }
}

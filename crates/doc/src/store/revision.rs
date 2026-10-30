//! Checked revision facts maintained with the entity store, not by geometry consumers.
use super::{SHARDS, shard_of};
use crate::{Entity, EntityKind, Handle};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
struct Shard {
    non_legacy: BTreeSet<Handle>,
    blocks: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub(super) struct RevisionSummary {
    shards: [Arc<Shard>; SHARDS],
}
impl Default for RevisionSummary {
    fn default() -> Self {
        let empty = Arc::new(Shard::default());
        Self { shards: std::array::from_fn(|_| Arc::clone(&empty)) }
    }
}

enum Entry<'a> {
    Legacy,
    NonLegacy,
    Block(&'a str),
}
fn entry(entity: &Entity) -> Entry<'_> {
    if entity.require_legacy_geometry().is_err() {
        Entry::NonLegacy
    } else if let EntityKind::Insert(insert) = &entity.kind {
        Entry::Block(&insert.block)
    } else {
        Entry::Legacy
    }
}

impl RevisionSummary {
    pub(super) fn record(&mut self, entity: &Entity) {
        match entry(entity) {
            Entry::Legacy => {}
            Entry::NonLegacy => {
                Arc::make_mut(&mut self.shards[shard_of(entity.handle)]).non_legacy.insert(entity.handle);
            }
            Entry::Block(name) => {
                let blocks = &mut Arc::make_mut(&mut self.shards[shard_of(entity.handle)]).blocks;
                if let Some(count) = blocks.get_mut(name) {
                    *count += 1;
                } else {
                    blocks.insert(name.to_owned(), 1);
                }
            }
        }
    }
    pub(super) fn remove(&mut self, entity: &Entity) {
        match entry(entity) {
            Entry::Legacy => {}
            Entry::NonLegacy => {
                Arc::make_mut(&mut self.shards[shard_of(entity.handle)]).non_legacy.remove(&entity.handle);
            }
            Entry::Block(name) => {
                let blocks = &mut Arc::make_mut(&mut self.shards[shard_of(entity.handle)]).blocks;
                if let Some(count) = blocks.get_mut(name) {
                    *count -= 1;
                    if *count == 0 {
                        blocks.remove(name);
                    }
                }
            }
        }
    }
    pub(super) fn first_non_legacy(&self) -> Option<Handle> {
        self.shards.iter().find_map(|shard| shard.non_legacy.first().copied())
    }
    pub(super) fn blocks(&self) -> impl Iterator<Item = &str> {
        self.shards.iter().flat_map(|shard| shard.blocks.keys().map(String::as_str))
    }
}

//! Generated definitions retain every referencing owner, including ambiguous source identities.
use super::identity::{Placement, Record, RecordIdentity, RecordIndex};
use crate::activation::{Family, records::Container};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Owner {
    pub identity: RecordIdentity,
    pub family: Family,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnerSet {
    pub owners: BTreeSet<Owner>,
    pub ambiguous: bool,
}

impl OwnerSet {
    pub fn summary(&self) -> String {
        let families: BTreeSet<_> = self.owners.iter().map(|owner| owner.family).collect();
        let kind = if self.ambiguous {
            "ambiguous"
        } else if self.owners.len() > 1 {
            "shared"
        } else {
            "single"
        };
        format!("{kind}:{families:?}:owners={}", self.owners.len())
    }
}

pub(super) struct Ownership {
    blocks: BTreeMap<Container, OwnerSet>,
}

impl Ownership {
    pub fn new(index: &RecordIndex) -> Self {
        let mut blocks = BTreeMap::new();
        for record in index.iter() {
            let Some(family @ (Family::Table | Family::Dimension)) = super::family_of_kind(&record.kind) else { continue };
            let names: Vec<_> = record.groups.iter().filter(|tag| tag.code == 2).map(|tag| tag.str().trim().to_ascii_uppercase()).collect();
            let identity = index.identity(record);
            let ambiguous = names.len() != 1 || matches!(identity, RecordIdentity::Occurrence(_));
            for name in names.iter().filter(|name| !name.is_empty()) {
                let owner = Owner { identity: identity.clone(), family };
                let set = blocks.entry(Container::block(name)).or_insert_with(|| OwnerSet { owners: BTreeSet::new(), ambiguous: false });
                set.owners.insert(owner);
                set.ambiguous |= ambiguous || index.definition_count(name) != 1;
            }
        }
        Self { blocks }
    }

    pub fn get(&self, record: &Record) -> Option<&OwnerSet> {
        match &record.position.placement {
            Placement::Container(container) => self.blocks.get(container),
            Placement::UnscopedBlocks => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(owners: &str) -> RecordIndex {
        RecordIndex::new(format!("0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*T1\n0\nLINE\n5\nB1\n0\nENDBLK\n0\nBLOCK\n2\n*D2\n0\nLINE\n5\nB2\n0\nENDBLK\n0\nBLOCK\n2\n*T99\n0\nLINE\n5\nB3\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{owners}0\nENDSEC\n0\nEOF\n").as_bytes())
    }

    #[test]
    fn shared_graphics_keep_all_owner_families_and_identities() {
        let records = index("0\nACAD_TABLE\n5\nA1\n2\n*t1\n0\nDIMENSION\n5\nA2\n2\n*T1\n0\nACAD_TABLE\n5\nA3\n2\n*T1\n");
        let ownership = Ownership::new(&records);
        let block_line = records.unique_handle(cadcraft_doc::Handle(0xB1)).unwrap();
        let owners = ownership.get(block_line).unwrap();
        assert_eq!(owners.owners.len(), 3);
        assert!(owners.owners.iter().any(|owner| owner.family == Family::Table));
        assert!(owners.owners.iter().any(|owner| owner.family == Family::Dimension));
        assert!(!owners.ambiguous);
    }

    #[test]
    fn alias_owners_and_owner_order_do_not_collapse_the_set() {
        let first = "0\nARC_DIMENSION\n5\nA1\n2\n*T1\n";
        let second = "0\nLARGE_RADIAL_DIMENSION\n5\nA2\n2\n*T1\n";
        let forward = index(&format!("{first}{second}"));
        let reversed = index(&format!("{second}{first}"));
        let a = Ownership::new(&forward);
        let b = Ownership::new(&reversed);
        let left = a.get(forward.unique_handle(cadcraft_doc::Handle(0xB1)).unwrap()).unwrap();
        let right = b.get(reversed.unique_handle(cadcraft_doc::Handle(0xB1)).unwrap()).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.owners.len(), 2);
        assert!(left.owners.iter().all(|owner| owner.family == Family::Dimension));
    }

    #[test]
    fn repeated_definition_names_are_ambiguous_for_every_body_occurrence() {
        let records = RecordIndex::new(b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*T1\n0\nLINE\n5\nB1\n0\nENDBLK\n0\nBLOCK\n2\n*t1\n0\nLINE\n5\nB2\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nACAD_TABLE\n5\nA1\n2\n*T1\n0\nENDSEC\n0\nEOF\n");
        let ownership = Ownership::new(&records);
        assert_eq!(records.definition_count("*T1"), 2);
        for handle in [0xB1, 0xB2] {
            assert!(ownership.get(records.unique_handle(cadcraft_doc::Handle(handle)).unwrap()).unwrap().ambiguous);
        }
        assert!(!records.named_target("BLOCK_RECORD", "*T1"));
    }

    #[test]
    fn duplicated_or_missing_owner_handles_remain_distinct_and_explicitly_ambiguous() {
        for handles in ["", "5\nA1\n"] {
            let records = index(&format!("0\nACAD_TABLE\n{handles}2\n*T1\n0\nACAD_TABLE\n{handles}2\n*T1\n"));
            let ownership = Ownership::new(&records);
            let owners = ownership.get(records.unique_handle(cadcraft_doc::Handle(0xB1)).unwrap()).unwrap();
            assert_eq!(owners.owners.len(), 2);
            assert!(owners.ambiguous);
        }
    }

    #[test]
    fn ambiguous_owner_references_mark_every_candidate_definition_without_name_exemptions() {
        let records = index("0\nACAD_TABLE\n5\nA1\n2\n*T1\n2\n*D2\n");
        let ownership = Ownership::new(&records);
        for handle in [0xB1, 0xB2] {
            let owners = ownership.get(records.unique_handle(cadcraft_doc::Handle(handle)).unwrap()).unwrap();
            assert!(owners.ambiguous);
            assert_eq!(owners.owners.len(), 1);
        }
        assert!(ownership.get(records.unique_handle(cadcraft_doc::Handle(0xB3)).unwrap()).is_none());
    }
}

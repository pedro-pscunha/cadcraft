//! Physical record positions are oracle identities, not activation node identifiers.
use crate::activation::records::Container;
use cadcraft_doc::Handle;
use cadcraft_dxf::{Tag, records, sections};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::dxf_read::legacy_equivalence) enum Placement {
    Container(Container),
    UnscopedBlocks,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::dxf_read::legacy_equivalence) struct Position {
    pub placement: Placement,
    /// Raw record ordinal in the container, including unknown records and compound followers.
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::dxf_read::legacy_equivalence) enum RecordIdentity {
    Handle(Handle),
    Occurrence(Position),
}

#[derive(Clone, Debug)]
pub(super) struct Record {
    pub position: Position,
    pub handle: Option<Handle>,
    pub kind: String,
    pub groups: Vec<Tag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum MetadataIdentity {
    Named { kind: String, name: String },
    Handle { kind: String, handle: Handle },
}

pub(super) struct RecordIndex {
    metadata: BTreeMap<Handle, Vec<MetadataIdentity>>,
    definitions: BTreeSet<String>,
    records: Vec<Record>,
    positions: BTreeMap<Position, usize>,
    handles: BTreeMap<Handle, Vec<usize>>,
}

impl RecordIndex {
    pub fn new(bytes: &[u8]) -> Self {
        let tags = cadcraft_dxf::parse(bytes).unwrap();
        let mut index = Self {
            metadata: BTreeMap::new(),
            definitions: BTreeSet::new(),
            records: Vec::new(),
            positions: BTreeMap::new(),
            handles: BTreeMap::new(),
        };
        for section in sections(&tags).into_iter().filter(|section| !matches!(section.name.as_str(), "ENTITIES" | "BLOCKS")) {
            for (kind, groups) in records(&section.tags) {
                let Some(handle) = groups.iter().find(|tag| matches!(tag.code, 5 | 105)).and_then(|tag| Handle::parse_hex(&tag.str())) else {
                    continue;
                };
                let identity = match groups.iter().find(|tag| tag.code == 2) {
                    Some(name) => MetadataIdentity::Named { kind, name: name.str().trim().to_ascii_uppercase() },
                    None => MetadataIdentity::Handle { kind, handle },
                };
                index.metadata.entry(handle).or_default().push(identity);
            }
        }
        let mut ordinals: BTreeMap<Placement, u32> = BTreeMap::new();
        for section in sections(&tags).into_iter().filter(|section| matches!(section.name.as_str(), "ENTITIES" | "BLOCKS")) {
            let mut placement = if section.name == "ENTITIES" { Placement::Container(Container::Entities) } else { Placement::UnscopedBlocks };
            for (kind, groups) in records(&section.tags) {
                if kind == "BLOCK" {
                    let name = groups.iter().find(|tag| tag.code == 2).map(Tag::str).unwrap_or_default();
                    index.definitions.insert(name.trim().to_ascii_uppercase());
                    placement = Placement::Container(Container::block(&name));
                    continue;
                }
                if kind == "ENDBLK" {
                    placement = Placement::UnscopedBlocks;
                    continue;
                }
                let ordinal = ordinals.entry(placement.clone()).or_default();
                let position = Position { placement: placement.clone(), ordinal: *ordinal };
                *ordinal = ordinal.checked_add(1).unwrap();
                let handle = groups.iter().find(|tag| tag.code == 5).and_then(|tag| Handle::parse_hex(&tag.str())).filter(|handle| handle.0 != 0);
                let offset = index.records.len();
                index.positions.insert(position.clone(), offset);
                if let Some(handle) = handle {
                    index.handles.entry(handle).or_default().push(offset);
                }
                index.records.push(Record { position, handle, kind, groups });
            }
        }
        index
    }

    pub fn named_target(&self, kind: &str, name: &str) -> bool {
        if kind == "BLOCK_RECORD" {
            return self.definitions.contains(name) || matches!(name, "*MODEL_SPACE" | "*PAPER_SPACE");
        }
        if matches!((kind, name), ("LAYER", "0") | ("LTYPE", "BYLAYER" | "BYBLOCK" | "CONTINUOUS") | ("STYLE", "STANDARD")) {
            return true;
        }
        self.metadata.values().flatten().filter(|identity| matches!(identity, MetadataIdentity::Named { kind: target_kind, name: target_name } if target_kind == kind && target_name == name)).count() == 1
    }

    pub fn metadata(&self, handle: Handle) -> Option<&MetadataIdentity> {
        let candidates = self.metadata.get(&handle)?;
        (candidates.len() == 1).then(|| &candidates[0])
    }

    pub fn all(&self) -> &[Record] {
        &self.records
    }

    pub fn handle_count(&self, handle: Handle) -> usize {
        self.handles.get(&handle).map_or(0, Vec::len)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.records.iter().filter(|record| super::family_of_kind(&record.kind).is_some())
    }

    pub fn identity(&self, record: &Record) -> RecordIdentity {
        match record.handle {
            Some(handle) if self.handles.get(&handle).is_some_and(|matches| matches.len() == 1) => RecordIdentity::Handle(handle),
            _ => RecordIdentity::Occurrence(record.position.clone()),
        }
    }

    pub fn unique_handle(&self, handle: Handle) -> Option<&Record> {
        let matches = self.handles.get(&handle)?;
        (matches.len() == 1).then(|| &self.records[matches[0]])
    }

    /// Unique handles survive record reordering. Ambiguous or absent source handles use their physical occurrence.
    pub fn corresponding(&self, source: &RecordIndex, record: &Record) -> Option<&Record> {
        match source.identity(record) {
            RecordIdentity::Handle(handle) => self.unique_handle(handle),
            RecordIdentity::Occurrence(position) => {
                let target = self.positions.get(&position).and_then(|offset| self.records.get(*offset))?;
                let reserved = target.handle.and_then(|handle| source.unique_handle(handle));
                reserved.is_none_or(|owner| owner.position == record.position).then_some(target)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(body: &str) -> RecordIndex {
        RecordIndex::new(format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n").as_bytes())
    }

    #[test]
    fn absent_and_duplicate_source_handles_match_renumbered_occurrences() {
        for handles in ["", "5\nA1\n", "5\n0\n"] {
            let source = index(&format!("0\nLINE\n{handles}10\n1\n0\nLINE\n{handles}10\n2\n"));
            let target = index("0\nLINE\n5\nB1\n10\n1\n0\nLINE\n5\nB2\n10\n2\n");
            let identities: Vec<_> = source.iter().map(|record| source.identity(record)).collect();
            assert_ne!(identities[0], identities[1]);
            for (record, handle) in source.iter().zip([Handle(0xB1), Handle(0xB2)]) {
                assert!(matches!(source.identity(record), RecordIdentity::Occurrence(_)));
                assert_eq!(target.corresponding(&source, record).unwrap().handle, Some(handle));
            }
        }
    }

    #[test]
    fn occurrence_fallback_cannot_claim_another_sources_unique_record() {
        let source = index("0\nLINE\n0\nLINE\n5\nA1\n");
        let target = index("0\nLINE\n5\nA1\n");
        let records: Vec<_> = source.iter().collect();
        assert!(target.corresponding(&source, records[0]).is_none());
        assert!(target.corresponding(&source, records[1]).is_some());
    }

    #[test]
    fn unique_handles_match_reordered_records_but_not_ambiguous_output() {
        let source = index("0\nLINE\n5\nA1\n0\nCIRCLE\n5\nA2\n");
        let reordered = index("0\nCIRCLE\n5\nA2\n0\nLINE\n5\nA1\n");
        let ambiguous = index("0\nLINE\n5\nA1\n0\nCIRCLE\n5\nA1\n");
        let line = source.unique_handle(Handle(0xA1)).unwrap();
        assert_eq!(reordered.corresponding(&source, line).unwrap().kind, "LINE");
        assert!(ambiguous.corresponding(&source, line).is_none());
    }

    #[test]
    fn missing_occurrences_are_not_replaced_and_unknown_records_keep_their_place() {
        let source = index("0\nACME_PROXY\n0\nLINE\n0\nLINE\n");
        let target = index("0\nACME_PROXY\n0\nLINE\n5\nB1\n");
        let records: Vec<_> = source.iter().collect();
        assert_eq!(records[0].position.ordinal, 1);
        assert_eq!(records[1].position.ordinal, 2);
        assert!(target.corresponding(&source, records[0]).is_some());
        assert!(target.corresponding(&source, records[1]).is_none());
    }
}

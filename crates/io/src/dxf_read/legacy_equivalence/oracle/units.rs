//! Whole source units include their ordered structural followers.
mod relations;
use super::identity::{Position, Record, RecordIdentity, RecordIndex};
pub(super) use relations::Relations;
use std::collections::BTreeMap;

pub(super) struct Unit<'a> {
    pub records: Vec<&'a Record>,
    pub closed: bool,
}

impl Unit<'_> {
    pub fn parent(&self) -> &Record {
        self.records[0]
    }
}

pub(super) struct UnitIndex<'a> {
    pub index: &'a RecordIndex,
    units: Vec<Unit<'a>>,
    parents: BTreeMap<Position, usize>,
}

impl<'a> UnitIndex<'a> {
    pub fn new(index: &'a RecordIndex) -> Self {
        let records = index.all();
        let mut units = Vec::new();
        let mut parents = BTreeMap::new();
        let mut at = 0;
        while let Some(parent) = records.get(at) {
            let follower = match parent.kind.as_str() {
                "INSERT" => Some("ATTRIB"),
                "POLYLINE" => Some("VERTEX"),
                _ => None,
            };
            let mut end = at + 1;
            if let Some(follower) = follower {
                while records.get(end).is_some_and(|record| record.kind == follower && record.position.placement == parent.position.placement) {
                    end += 1;
                }
            }
            let has_end = follower.is_some()
                && records.get(end).is_some_and(|record| record.kind == "SEQEND" && record.position.placement == parent.position.placement);
            let requires_end = parent.kind == "POLYLINE"
                || end > at + 1
                || parent.kind == "INSERT" && parent.groups.iter().any(|tag| tag.code == 66 && tag.i64() != 0);
            if has_end {
                end += 1;
            }
            parents.insert(parent.position.clone(), units.len());
            units.push(Unit { records: records[at..end].iter().collect(), closed: !requires_end || has_end });
            at = end;
        }
        Self { index, units, parents }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Unit<'a>> {
        self.units.iter().filter(|unit| super::family_of_kind(&unit.parent().kind).is_some())
    }

    pub fn corresponding(&self, source: &UnitIndex<'_>, unit: &Unit<'_>) -> Option<&Unit<'a>> {
        let parent = self.index.corresponding(source.index, unit.parent())?;
        self.parents.get(&parent.position).and_then(|offset| self.units.get(*offset))
    }

    pub fn same_shape(&self, source: &UnitIndex<'_>, unit: &Unit<'_>, target: &Unit<'_>) -> bool {
        unit.closed
            && target.closed
            && unit.records.len() == target.records.len()
            && unit.records.iter().zip(&target.records).enumerate().all(|(slot, (left, right))| {
                if left.kind != right.kind {
                    return false;
                }
                if slot == 0 {
                    return true;
                }
                match source.index.identity(left) {
                    RecordIdentity::Handle(handle) if self.index.handle_count(handle) != 0 => {
                        self.index.unique_handle(handle).is_some_and(|record| record.position == right.position)
                    }
                    _ => right.handle.and_then(|handle| source.index.unique_handle(handle)).is_none_or(|owner| owner.position == left.position),
                }
            })
    }
}

pub(super) fn geometry_conserved(
    source: &Unit<'_>,
    actual: &Unit<'_>,
    expected: &Unit<'_>,
    version: cadcraft_dxf::Version,
    operation: super::Operation,
) -> bool {
    source.records.len() == actual.records.len()
        && source.records.len() == expected.records.len()
        && source.records.iter().zip(&actual.records).zip(&expected.records).all(|((source, actual), expected)| {
            let fields = super::fields(&source.kind, &source.groups, version);
            let scale = fields
                .iter()
                .filter(|((code, _), _)| (10..=49).contains(code))
                .filter_map(|(_, value)| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .map(f64::abs)
                .fold(32.0, f64::max)
                * 2.0;
            source.kind == actual.kind
                && source.kind == expected.kind
                && super::equivalent(
                    &source.kind,
                    &super::reference_fields(&source.kind, &source.groups, &expected.groups, version, operation),
                    &super::fields(&source.kind, &actual.groups, version),
                    scale,
                )
                && (!matches!(operation, super::Operation::Identity)
                    || super::equivalent(&source.kind, &fields, &super::fields(&source.kind, &actual.groups, version), scale))
        })
}

pub(super) const INSERT_SOURCE: &[u8] = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\nB0\n2\nDEF\n70\n0\n10\n0\n20\n0\n30\n0\n0\nLINE\n5\nB2\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n0\nENDBLK\n5\nB1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nA1\n66\n1\n2\nDEF\n10\n0\n20\n0\n30\n0\n41\n1\n42\n1\n43\n1\n50\n0\n70\n1\n71\n1\n0\nATTRIB\n5\nA2\n330\nA1\n10\n1\n20\n2\n30\n0\n40\n1\n1\nVALUE\n2\nTAG\n7\nStandard\n70\n0\n0\nSEQEND\n5\nA3\n330\nA1\n0\nENDSEC\n0\nEOF\n";

const POLYLINE_SOURCE: &[u8] = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\nA1\n100\nAcDbEntity\n100\nAcDb3dPolyline\n66\n1\n10\n0\n20\n0\n30\n0\n70\n8\n0\nVERTEX\n5\nA2\n330\nA1\n100\nAcDbEntity\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n1\n20\n2\n30\n3\n70\n32\n0\nVERTEX\n5\nA3\n330\nA1\n100\nAcDbEntity\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n4\n20\n5\n30\n6\n70\n32\n0\nSEQEND\n5\nA4\n330\nA1\n0\nENDSEC\n0\nEOF\n";

fn saved_unit_matches(source: &[u8], target: &[u8], kind: &str) -> bool {
    let original = RecordIndex::new(source);
    let written = RecordIndex::new(target);
    let original_units = UnitIndex::new(&original);
    let written_units = UnitIndex::new(&written);
    let unit = original_units.iter().find(|unit| unit.parent().kind == kind).unwrap();
    let Some(actual) = written_units.corresponding(&original_units, unit) else { return false };
    Relations::new(&original_units, &written_units).conserved(&original_units, &written_units, unit, actual)
        && geometry_conserved(unit, actual, unit, cadcraft_dxf::Version::R2018, super::Operation::Identity)
}

#[test]
fn complete_units_accept_consistent_follower_identity_remapping() {
    for (source, kind) in [(INSERT_SOURCE, "INSERT"), (POLYLINE_SOURCE, "POLYLINE")] {
        assert!(saved_unit_matches(source, source, kind));
        let target =
            String::from_utf8(source.to_vec()).unwrap().replace("5\nA2\n", "5\nC2\n").replace("5\nA3\n", "5\nC3\n").replace("5\nA4\n", "5\nC4\n");
        assert!(saved_unit_matches(source, target.as_bytes(), kind));
    }
}

#[test]
fn follower_reference_remapping_and_implicit_parent_owners_preserve_relations() {
    let source = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap().replace("0\nSEQEND\n", "1001\nTEST_REFERENCES\n1005\nA2\n0\nSEQEND\n");
    let remapped = source.replace("5\nA2\n", "5\nC2\n").replace("1005\nA2\n", "1005\nC2\n");
    assert!(saved_unit_matches(source.as_bytes(), remapped.as_bytes(), "INSERT"));
    let dangling = remapped.replace("1005\nC2\n", "1005\nA2\n");
    assert!(!saved_unit_matches(source.as_bytes(), dangling.as_bytes(), "INSERT"));
    let source = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap().replace("5\nA1\n", "").replace("330\nA1\n", "");
    let target = source.replace("0\nINSERT\n", "0\nINSERT\n5\nC1\n").replace("5\nA2\n", "5\nA2\n330\nC1\n").replace("5\nA3\n", "5\nA3\n330\nC1\n");
    assert!(saved_unit_matches(source.as_bytes(), target.as_bytes(), "INSERT"));
}

#[test]
fn attribute_payload_and_named_references_cannot_change() {
    let source = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap();
    for target in [
        source.replace("1\nVALUE\n", "1\nCHANGED\n"),
        source.replace("2\nDEF\n10\n", "2\nMISSING\n10\n"),
        source.replace("7\nStandard\n", "7\nMISSING\n"),
    ] {
        assert!(!saved_unit_matches(INSERT_SOURCE, target.as_bytes(), "INSERT"));
    }
}

#[test]
fn follower_omission_reordering_and_vertex_mutation_fail_as_whole_units() {
    let source = String::from_utf8(POLYLINE_SOURCE.to_vec()).unwrap();
    let first = "0\nVERTEX\n5\nA2\n330\nA1\n100\nAcDbEntity\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n1\n20\n2\n30\n3\n70\n32\n";
    let second = "0\nVERTEX\n5\nA3\n330\nA1\n100\nAcDbEntity\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n4\n20\n5\n30\n6\n70\n32\n";
    let reordered = source.replace(&format!("{first}{second}"), &format!("{second}{first}"));
    let omitted = source.replace(first, "");
    let moved = source.replace("10\n1\n20\n2\n", "10\n99\n20\n2\n");
    let no_end = source.replace("0\nSEQEND\n5\nA4\n330\nA1\n", "");
    for target in [reordered, omitted, moved, no_end] {
        assert!(!saved_unit_matches(POLYLINE_SOURCE, target.as_bytes(), "POLYLINE"));
    }
}

#[test]
fn attribute_sequences_cannot_be_reordered_or_omitted() {
    let original = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap();
    let body = original.split("0\nATTRIB\n").nth(1).unwrap().split("0\nSEQEND\n").next().unwrap();
    let first = format!("0\nATTRIB\n{body}");
    let second = first.replace("5\nA2\n", "5\nA4\n").replace("1\nVALUE\n", "1\nOTHER\n").replace("2\nTAG\n", "2\nSECOND\n");
    let source = original.replace(&first, &format!("{first}{second}"));
    assert!(saved_unit_matches(source.as_bytes(), source.as_bytes(), "INSERT"));
    let reordered = source.replace(&format!("{first}{second}"), &format!("{second}{first}"));
    let omitted = source.replace(&first, "");
    assert!(!saved_unit_matches(source.as_bytes(), reordered.as_bytes(), "INSERT"));
    assert!(!saved_unit_matches(source.as_bytes(), omitted.as_bytes(), "INSERT"));
}

#[test]
fn equal_object_payloads_do_not_make_reference_targets_interchangeable() {
    let source = String::from_utf8(INSERT_SOURCE.to_vec())
        .unwrap()
        .replace("0\nSEQEND\n", "1001\nTEST_REFERENCES\n1005\nD1\n0\nSEQEND\n")
        .replace("0\nEOF\n", "0\nSECTION\n2\nOBJECTS\n0\nXRECORD\n5\nD1\n2\nSAME_PAYLOAD\n0\nXRECORD\n5\nD2\n2\nSAME_PAYLOAD\n0\nENDSEC\n0\nEOF\n");
    assert!(saved_unit_matches(source.as_bytes(), source.as_bytes(), "INSERT"));
    let wrong = source.replace("1005\nD1\n", "1005\nD2\n");
    assert!(!saved_unit_matches(source.as_bytes(), wrong.as_bytes(), "INSERT"));
}

#[test]
fn reactor_and_xdata_references_cannot_be_dropped_or_retargeted() {
    let source =
        String::from_utf8(INSERT_SOURCE.to_vec()).unwrap().replace("5\nA2\n330\nA1\n", "5\nA2\n330\nA1\n102\n{ACAD_REACTORS\n330\nB2\n102\n}\n");
    assert!(saved_unit_matches(source.as_bytes(), source.as_bytes(), "INSERT"));
    let dropped = source.replace("102\n{ACAD_REACTORS\n330\nB2\n102\n}\n", "");
    let retargeted = source.replace("{ACAD_REACTORS\n330\nB2\n", "{ACAD_REACTORS\n330\nA1\n");
    assert!(!saved_unit_matches(source.as_bytes(), dropped.as_bytes(), "INSERT"));
    assert!(!saved_unit_matches(source.as_bytes(), retargeted.as_bytes(), "INSERT"));
    let source = source.replace("0\nSEQEND\n", "1001\nTEST_REFERENCES\n1005\nB2\n0\nSEQEND\n");
    let dangling = source.replace("1005\nB2\n", "1005\nFFFF\n");
    assert!(saved_unit_matches(source.as_bytes(), source.as_bytes(), "INSERT"));
    assert!(!saved_unit_matches(source.as_bytes(), dangling.as_bytes(), "INSERT"));
}

#[test]
fn polyline_followers_reach_every_actual_legacy_edit_as_one_unit() {
    for operation in super::Operation::ALL {
        let measured = super::measure(POLYLINE_SOURCE, operation);
        assert_eq!(measured.len(), 1);
        assert_eq!(measured.get(&super::RecordIdentity::Handle(cadcraft_doc::Handle(0xA1))), Some(&true), "{operation:?}: {measured:?}");
    }
}

#[test]
fn insert_attributes_reach_every_actual_legacy_edit_as_one_unit() {
    let drawing = crate::read_dxf(INSERT_SOURCE).unwrap();
    let complete = crate::write_dxf(&drawing);
    for operation in super::Operation::ALL {
        let measured = super::measure(complete.as_bytes(), operation);
        assert_eq!(measured.len(), 2);
        assert!(!measured.contains_key(&super::RecordIdentity::Handle(cadcraft_doc::Handle(0xA2))));
        let result = measured.get(&super::RecordIdentity::Handle(cadcraft_doc::Handle(0xA1))).unwrap();
        if matches!(operation, super::Operation::Identity) {
            assert!(*result);
        }
        eprintln!("INSERT_UNIT operation={operation:?} lossless={result}");
    }
}

#[test]
fn malformed_source_owners_are_not_accepted_as_a_valid_unit() {
    let wrong = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap().replace("330\nA1\n", "330\nB2\n");
    assert!(!saved_unit_matches(wrong.as_bytes(), wrong.as_bytes(), "INSERT"));
}

#[test]
fn explicit_paper_space_ownership_is_compared_without_a_model_space_fallback() {
    let source = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap()
        .replace("0\nSECTION\n2\nBLOCKS\n", "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n0\nBLOCK_RECORD\n5\nD1\n2\n*Paper_Space1\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n")
        .replace("5\nA1\n66\n", "5\nA1\n330\nD1\n67\n1\n66\n");
    assert!(saved_unit_matches(source.as_bytes(), source.as_bytes(), "INSERT"));
}

#[test]
fn changing_follower_owners_cannot_pass_a_compound_comparison() {
    let source = RecordIndex::new(INSERT_SOURCE);
    let changed = String::from_utf8(INSERT_SOURCE.to_vec()).unwrap().replace("330\nA1\n", "330\nB2\n");
    let target = RecordIndex::new(changed.as_bytes());
    let source_units = UnitIndex::new(&source);
    let target_units = UnitIndex::new(&target);
    let unit = source_units.iter().find(|unit| unit.parent().kind == "INSERT").unwrap();
    let written = target_units.corresponding(&source_units, unit).unwrap();
    let relations = Relations::new(&source_units, &target_units);
    assert!(!relations.conserved(&source_units, &target_units, unit, written));
}

#[test]
fn compound_followers_receive_one_parent_verdict() {
    let measured = super::measure(INSERT_SOURCE, super::Operation::Identity);
    assert_eq!(measured.len(), 2, "one INSERT unit and one block LINE");
    assert!(measured.contains_key(&super::RecordIdentity::Handle(cadcraft_doc::Handle(0xA1))));
    assert!(!measured.contains_key(&super::RecordIdentity::Handle(cadcraft_doc::Handle(0xA2))));
}

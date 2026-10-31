//! Plan v3, 4.5: the Raw path end to end, on real DXF files in `tests/data/activation/`: a malformed spatial SPLINE, a spatial LEADER, a
//! forced canonical-read failure, the read note, and read -> Raw -> edit refusal -> save (a DXF keeps the source records, a DWG save
//! refuses what it cannot check). The need of a record is the stand-in of `activation_fixtures` (a tilted or spatial record needs
//! Canonical; the xdata string UNREADABLE and a SPLINE with a wrong control count cannot be read) until the section 3 predicate lands.

use std::path::PathBuf;

use cadcraft_doc::{Drawing, EntityKind, GeometryRevision, Handle};
use cadcraft_dxf::{Tag, parse, records, sections};
use cadcraft_geom::{Mat3, Mat4, Vec2};
use cadcraft_io::activation::{FamilySet, ReaderMode, ReaderPolicy};

use crate::activation_fixtures::{attrib, block, file, insert, line, seqend, stand_in};

fn dir() -> PathBuf {
    crate::activation_fixtures::dir()
}

/// (name, text) of the end-to-end fixtures.
fn raw_cases() -> Vec<(&'static str, String)> {
    let spline = "0\nSPLINE\n5\nA1\n8\n0\n70\n8\n71\n3\n73\n5\n10\n0\n20\n0\n30\n2\n10\n1\n20\n1\n30\n3\n".to_string();
    let leader =
        "0\nLEADER\n5\nA1\n8\n0\n3\nStandard\n71\n1\n72\n0\n73\n3\n74\n0\n75\n0\n76\n2\n10\n0\n20\n0\n30\n1\n10\n5\n20\n5\n30\n2\n".to_string();
    let unreadable = "0\nCIRCLE\n5\nA1\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n210\n0\n220\n1\n230\n1\n1001\nSTAND_IN\n1000\nUNREADABLE\n".to_string();
    vec![
        ("r1_malformed_spatial_spline", file("", &[], &[spline, line("A2")])),
        ("r2_spatial_leader", file("", &[], &[leader, line("A2")])),
        ("r3_forced_canonical_read_failure", file("", &[], &[unreadable, line("A2")])),
        (
            "r4_raw_insert_edit_refusal_and_save",
            file("", &[block("DEF", 0, &[line("B1")])], &[insert("E1", "DEF", true), attrib("E2", false), seqend("E3"), line("E4")]),
        ),
    ]
}

fn policy() -> ReaderPolicy {
    ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE).with_raw_path()
}

fn read(name: &str) -> (Drawing, String) {
    let text = std::fs::read_to_string(dir().join(format!("{name}.dxf"))).unwrap_or_else(|e| panic!("{name}: {e}"));
    (cadcraft_io::read_with_need(text.as_bytes(), "x.dxf", policy(), &stand_in).unwrap(), text)
}

fn raw_parents(d: &Drawing) -> Vec<(String, Handle)> {
    d.model
        .iter()
        .filter_map(|e| if let EntityKind::Raw(raw) = &e.kind { Some((raw.parent().kind().to_string(), raw.parent().handle())) } else { None })
        .collect()
}

#[test]
fn the_files_on_disk_are_the_generated_ones() {
    for (name, text) in raw_cases() {
        assert_eq!(std::fs::read_to_string(dir().join(format!("{name}.dxf"))).unwrap_or_default(), text, "{name}: run write_raw_fixtures");
    }
}

#[test]
#[ignore = "rewrites tests/data/activation/r*.dxf"]
fn write_raw_fixtures() {
    for (name, text) in raw_cases() {
        std::fs::write(dir().join(format!("{name}.dxf")), text).unwrap();
    }
}

#[test]
fn a_malformed_spatial_spline_is_kept_raw_beside_its_legacy_neighbour() {
    let (d, _) = read("r1_malformed_spatial_spline");
    assert_eq!(raw_parents(&d), [("SPLINE".to_string(), Handle(0xA1))]);
    assert!(d.model.iter().any(|e| matches!(e.kind, EntityKind::Line(_)) && e.geometry_revision == GeometryRevision::LegacyProjectedV1));
    assert!(d.read_notes.iter().any(|n| n.contains("SPLINE A1") && n.contains("canonical reading fails")), "{:?}", d.read_notes);
}

#[test]
fn a_spatial_leader_is_kept_raw() {
    let (d, _) = read("r2_spatial_leader");
    assert_eq!(raw_parents(&d), [("LEADER".to_string(), Handle(0xA1))]);
    assert!(d.read_notes.iter().any(|n| n.contains("LEADER A1") && n.contains("kept as read")), "{:?}", d.read_notes);
}

#[test]
fn a_forced_canonical_read_failure_is_kept_raw_with_its_reason() {
    let (d, _) = read("r3_forced_canonical_read_failure");
    assert_eq!(raw_parents(&d), [("CIRCLE".to_string(), Handle(0xA1))]);
    let note = d.read_notes.iter().find(|n| n.contains("CIRCLE A1")).expect("the read note");
    assert!(note.contains("canonical reading fails") && note.contains("no canonical reader"), "{note}");
}

#[test]
fn without_the_raw_path_the_same_files_read_legacy_and_say_nothing() {
    for (name, _) in raw_cases() {
        let text = std::fs::read_to_string(dir().join(format!("{name}.dxf"))).unwrap();
        let plain = cadcraft_io::read(text.as_bytes(), "x.dxf").unwrap();
        assert!(raw_parents(&plain).is_empty() && !plain.read_notes.iter().any(|n| n.contains("kept as read")), "{name}");
    }
}

fn unit_records(text: &str, handles: &[u64]) -> Vec<(String, Vec<Tag>)> {
    let tags = parse(text.as_bytes()).unwrap();
    let all: Vec<_> = sections(&tags).into_iter().filter(|s| s.name == "ENTITIES").flat_map(|s| records(&s.tags)).collect();
    handles
        .iter()
        .map(|h| {
            all.iter()
                .find(|(_, body)| body.iter().any(|t| t.code == 5 && Handle::parse_hex(&t.str()) == Some(Handle(*h))))
                .unwrap_or_else(|| panic!("record {h:X}"))
                .clone()
        })
        .collect()
}

/// A record's groups without its owner (group 330), which a save sets to the block record it writes.
fn without_owner(body: &[Tag]) -> Vec<(i32, String)> {
    body.iter().filter(|t| t.code != 330).map(|t| (t.code, t.str())).collect()
}

#[test]
fn read_raw_edit_refused_then_a_dxf_save_keeps_the_source_records() {
    let (mut d, source) = read("r4_raw_insert_edit_refusal_and_save");
    assert_eq!(raw_parents(&d), [("INSERT".to_string(), Handle(0xE1))]);
    // The edit of the raw unit is refused with its handle, and the drawing stays as it was.
    let before = d.clone();
    let shift = Mat4::from_mat3(&Mat3::translate(Vec2::new(4.0, 5.0)));
    let error = d.transform_entity(Handle(0xE1), &shift).expect_err("a raw unit is read-only").to_string();
    assert!(error.contains("E1") || error.contains("INSERT"), "{error}");
    assert_eq!(d, before);
    // Another entity moves; the unit is saved as read.
    d.transform_entity(Handle(0xE4), &shift).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    let (from, to) = (unit_records(&source, &[0xE1, 0xE2, 0xE3]), unit_records(&saved, &[0xE1, 0xE2, 0xE3]));
    for ((kind_from, body_from), (kind_to, body_to)) in from.iter().zip(&to) {
        assert_eq!(kind_from, kind_to);
        assert_eq!(without_owner(body_from), without_owner(body_to), "{kind_from} keeps its source groups");
    }
}

#[test]
fn a_dwg_save_that_cannot_check_a_raw_unit_refuses_and_names_it_while_the_dxf_save_keeps_it() {
    // 4.3: the DWG route may write a raw unit only if an independent read of the DWG finds every value of its records. These
    // hand-made files carry no tables for the layers, styles and blocks their groups name, so the check cannot find them: the DWG
    // save refuses, with the unit and the reason, and produces nothing. The DXF save keeps the unit.
    for (name, _) in raw_cases() {
        let (mut d, _) = read(name);
        let spare = d.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle).expect("a line to edit");
        d.transform_entity(spare, &Mat4::from_mat3(&Mat3::translate(Vec2::new(4.0, 5.0)))).unwrap();
        let unit = d.model.iter().find_map(|e| if let EntityKind::Raw(raw) = &e.kind { Some(raw.parent().handle()) } else { None }).unwrap();
        let error = cadcraft_io::write(&d, "x.dwg").expect_err(name).to_string();
        assert!(error.contains("Save refused for raw unit") && error.contains(&format!("{:X}", unit.0)), "{name}: {error}");
        assert_eq!(unit_records(&cadcraft_io::write_dxf(&d), &[unit.0]).len(), 1, "{name}: the DXF save keeps the unit");
    }
}

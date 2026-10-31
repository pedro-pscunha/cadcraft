//! Plan v3, 4.5: the Raw path end to end, on real DXF files in `tests/data/activation/`: a malformed spatial SPLINE, a spatial LEADER, a
//! forced canonical-read failure, the read note, and read -> Raw -> edit refusal -> save (a DXF keeps the source records, a DWG save
//! refuses what it cannot check). The static Legacy classifier decides need. Fixture readability remains separate.

use std::path::PathBuf;

use cadcraft_doc::{DocError, Drawing, EntityKind, GeometryRevision, Handle, Space};
use cadcraft_dxf::{Tag, parse, records, sections};
use cadcraft_geom::{Bounds2, Mat3, Mat4, Vec2};

use crate::activation_fixtures::read_fixtures::read_fixture as read;
use crate::activation_fixtures::{attrib, block, file, insert, line, seqend};

#[path = "../support/raw_compounds.rs"]
mod support;

fn dir() -> PathBuf {
    crate::activation_fixtures::dir()
}

/// The file of a built drawing that holds a raw INSERT unit (tilted, two ATTRIBs, SEQEND) or a raw heavy POLYLINE unit (tilted, two
/// VERTEXes, SEQEND) beside a LINE: CADCraft's own full DXF, with the
/// tables, the block record and the registered application its groups name.
fn full_tables_text(kind: &str) -> String {
    cadcraft_io::write_dxf(&support::drawing(kind))
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
        ("r5_raw_insert_with_full_tables", full_tables_text("INSERT")),
        ("r6_raw_polyline_with_full_tables", full_tables_text("POLYLINE")),
    ]
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
    for chain in CHAINS.iter().filter(|chain| !chain.tables) {
        let name = chain.name;
        let (mut d, _) = read(name);
        let spare = d.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle).expect("a line to edit");
        d.transform_entity(spare, &Mat4::from_mat3(&Mat3::translate(Vec2::new(4.0, 5.0)))).unwrap();
        let unit = d.model.iter().find_map(|e| if let EntityKind::Raw(raw) = &e.kind { Some(raw.parent().handle()) } else { None }).unwrap();
        let error = cadcraft_io::write(&d, "x.dwg").expect_err(name).to_string();
        assert!(error.contains("Save refused for raw unit") && error.contains(&format!("{:X}", unit.0)), "{name}: {error}");
        assert_eq!(unit_records(&cadcraft_io::write_dxf(&d), &[unit.0]).len(), 1, "{name}: the DXF save keeps the unit");
    }
}

/// Source groups of the records of one handle, in order.
fn groups_of(text: &str, handle: u64) -> (String, Vec<(i32, String)>) {
    let (kind, body) = unit_records(text, &[handle]).remove(0);
    (kind, body.iter().map(|t| (t.code, t.str())).collect())
}

/// A group in a form that does not depend on how a writer spells it: handles as numbers, reals to nine places, integers as integers.
fn normal((code, value): &(i32, String)) -> (i32, String) {
    let value = value.trim();
    let text = match *code {
        5 | 105 | 330..=369 | 390..=399 | 1005 => u64::from_str_radix(value, 16).map_or_else(|_| value.to_string(), |h| format!("#{h:X}")),
        10..=59 | 110..=149 | 210..=239 | 1010..=1059 => value.parse::<f64>().map_or_else(|_| value.to_string(), |v| format!("{v:.9}")),
        60..=99 | 160..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1071 => {
            value.parse::<i64>().map_or_else(|_| value.to_string(), |v| v.to_string())
        }
        _ => value.to_string(),
    };
    (*code, text)
}
#[test]
fn a_raw_unit_of_a_file_with_full_tables_is_saved_to_dwg_and_reads_back_with_its_source_groups() {
    // The file is CADCraft's own save of a drawing with the unit, so the layers, styles, the block record SYMBOL and the application
    // RAW_APP its groups name are all there. The need of the INSERT or POLYLINE is the stand-in's (its plane is tilted).
    for chain in CHAINS.iter().filter(|chain| chain.tables) {
        dwg_reads_back_the_unit(chain);
    }
}

fn dwg_reads_back_the_unit(chain: &Chain) {
    let (d, source) = read(chain.name);
    assert_eq!(raw_parents(&d), [(chain.kind.to_string(), Handle(chain.unit[0]))]);
    // The old-header INSERT fixture contains R2010 fields. This positive save must retain them.
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let bytes = cadcraft_io::write_with(&d, "x.dwg", &options).expect("a unit whose values the independent DWG read finds is written");
    let dxf = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&bytes).unwrap()).unwrap();
    // The DXF save is the reference: it keeps every source group but the owner.
    let dxf_save = cadcraft_io::write_dxf(&d);
    for &handle in chain.unit {
        let (kind, from) = groups_of(&source, handle);
        assert_eq!(
            groups_of(&dxf_save, handle).1.iter().filter(|g| g.0 != 330).collect::<Vec<_>>(),
            from.iter().filter(|g| g.0 != 330).collect::<Vec<_>>(),
            "DXF {kind} {handle:X}"
        );
        let back: Vec<_> = groups_of(&dxf, handle).1.iter().map(normal).collect();
        // Every source group is in the DWG's record, once its spelling is normalised, except: the owner (330 outside a 102 group) and the
        // reactor, extension dictionary and 102 markers, which the DWG sets to its own block record and renumbers with the metadata
        // objects; and the INSERT defaults that the DWG does not repeat (scale 1 and rotation 0).
        let default = |g: &(i32, String)| {
            matches!(g, (41..=43, v) if v.trim().parse::<f64>() == Ok(1.0)) || matches!(g, (50, v) if v.trim().parse::<f64>() == Ok(0.0))
        };
        let missing: Vec<_> = from.iter().filter(|g| ![330, 360, 102].contains(&g.0) && !default(g) && !back.contains(&normal(g))).collect();
        assert!(missing.is_empty(), "{kind} {handle:X}: groups lost in the DWG: {missing:?}");
    }
}

/// One read fixture and the records of its raw unit (parent first), the LINE beside it, and whether the file has the tables the DWG check needs.
struct Chain {
    name: &'static str,
    kind: &'static str,
    unit: &'static [u64],
    line: u64,
    tables: bool,
}

const CHAINS: [Chain; 6] = [
    Chain { name: "r1_malformed_spatial_spline", kind: "SPLINE", unit: &[0xA1], line: 0xA2, tables: false },
    Chain { name: "r2_spatial_leader", kind: "LEADER", unit: &[0xA1], line: 0xA2, tables: false },
    Chain { name: "r3_forced_canonical_read_failure", kind: "CIRCLE", unit: &[0xA1], line: 0xA2, tables: false },
    Chain { name: "r4_raw_insert_edit_refusal_and_save", kind: "INSERT", unit: &[0xE1, 0xE2, 0xE3], line: 0xE4, tables: false },
    Chain { name: "r5_raw_insert_with_full_tables", kind: "INSERT", unit: &[0x200, 0x201, 0x202, 0x203], line: 0x90, tables: true },
    Chain { name: "r6_raw_polyline_with_full_tables", kind: "POLYLINE", unit: &[0x200, 0x201, 0x202, 0x203], line: 0x90, tables: true },
];

fn model_extents(d: &Drawing) -> Bounds2 {
    cadcraft_render::space_extents(d, &Space::Model).unwrap_or_else(|e| panic!("the model has extents: {e}"))
}

#[test]
fn every_read_fixture_runs_the_whole_chain() {
    let shift = Mat4::from_mat3(&Mat3::translate(Vec2::new(4.0, 5.0)));
    for chain in &CHAINS {
        let name = chain.name;
        let (mut d, source) = read(name);
        let unit = Handle(chain.unit[0]);
        // Read: one Raw unit of the right family, and its LINE read Legacy.
        assert_eq!(raw_parents(&d), [(chain.kind.to_string(), unit)], "{name}");
        let spare = d.entity(Handle(chain.line)).unwrap_or_else(|| panic!("{name}: the LINE"));
        assert!(matches!(spare.kind, EntityKind::Line(_)) && spare.geometry_revision == GeometryRevision::LegacyProjectedV1, "{name}");
        // Edit refusal: typed, with the unit's handle and family, and the drawing stays as it was.
        let before = d.clone();
        let refused = d.transform_entity(unit, &shift).expect_err(name);
        assert!(matches!(&refused, DocError::RawReadOnly { handle, family, .. } if *handle == unit && family == chain.kind), "{name}: {refused:?}");
        assert_eq!(d, before, "{name}");
        // Edit of another entity: the LINE moves, the unit does not.
        d.transform_entity(Handle(chain.line), &shift).unwrap();
        let before_unit = before.entity(unit).unwrap();
        assert_eq!(d.entity(unit).unwrap(), before_unit, "{name}");
        // Exact DXF save: every record of the unit keeps its source groups, and the saved file reads back with the same unit and the moved LINE.
        let saved = cadcraft_io::write_dxf(&d);
        let (from, to) = (unit_records(&source, chain.unit), unit_records(&saved, chain.unit));
        for ((kind_from, body_from), (kind_to, body_to)) in from.iter().zip(&to) {
            assert_eq!(kind_from, kind_to, "{name}");
            assert_eq!(without_owner(body_from), without_owner(body_to), "{name}: {kind_from} keeps its source groups");
        }
        let again = cadcraft_io::read_with_need(
            saved.as_bytes(),
            "x.dxf",
            crate::activation_fixtures::read_fixtures::raw_policy(),
            &crate::activation_fixtures::source_predicate(saved.as_bytes()),
        )
        .unwrap_or_else(|e| panic!("{name}: the saved file reads: {e}"));
        assert_eq!(raw_parents(&again), raw_parents(&d), "{name}");
        assert_eq!(again.entity(Handle(chain.line)).map(|e| &e.kind), d.entity(Handle(chain.line)).map(|e| &e.kind), "{name}: the moved LINE");
        // Display and bounds: the unit is an unresolved record with its reason, and it adds nothing to the extents.
        let list = cadcraft_render::build(&d, &Space::Model, &Default::default());
        let [shown] = list.unresolved.as_slice() else { panic!("{name}: {:?}", list.unresolved) };
        assert_eq!(shown.handle, unit, "{name}");
        let EntityKind::Raw(raw) = &before_unit.kind else { panic!("{name}") };
        assert_eq!(shown.reason, raw.reason(), "{name}");
        let moved_line = [d.entity(Handle(chain.line)).unwrap().as_ref()];
        assert_eq!(model_extents(&d), cadcraft_render::entities_extents(&d, moved_line, 0).unwrap(), "{name}: the extents are the LINE's");
        assert_eq!(list.bounds, model_extents(&d), "{name}: the display bounds are the extents");
        // Export refusal: PNG, SVG and PDF refuse and name the unit, with the extents or with a window that hides it.
        let window = Bounds2::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));
        for out in ["x.png", "x.svg", "x.pdf"] {
            for window in [None, Some(window)] {
                let error = cadcraft_io::write_framed(&d, out, window).expect_err(name).to_string();
                assert!(error.contains(&format!("{:X}", unit.0)) && error.contains("unresolved"), "{name} {out}: {error}");
            }
        }
        // DWG: a file with the tables its groups name is written and an independent read finds the unit's values (the test below), any other refuses.
        let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
        let dwg = cadcraft_io::write_with(&d, "x.dwg", &options);
        if chain.tables {
            assert!(dwg.is_ok(), "{name}: {:?}", dwg.err());
        } else {
            let error = dwg.expect_err(name).to_string();
            assert!(error.contains("Save refused for raw unit") && error.contains(&format!("{:X}", unit.0)), "{name}: {error}");
        }
    }
}

//! Plan v3, 4.5: what the editing commands do with the units of the read fixtures (`crates/io/tests/data/activation/r1..r6`), read with the
//! raw path on. The io tests run the same files through read, edit refusal, save, display and export; here the commands that copy and
//! bind: COPY, MOVE, ROTATE, SCALE, MIRROR, EXPLODE, BLOCK, COPYCLIP, CUTCLIP, ERASE, and the XREF attach and bind of a host that holds a unit.

use cadcraft_doc::{Drawing, EntityKind, Handle};
use cadcraft_dxf::{parse, records, sections};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

#[path = "../../io/tests/support/read_fixtures.rs"]
mod read_fixtures;
#[path = "support/temp_dir.rs"]
mod temp_dir;

use read_fixtures::{dir, read_fixture};

/// A fixture, the handle of its raw unit and the handle of the LINE beside it.
const FIXTURES: [(&str, u64, u64); 6] = [
    ("r1_malformed_spatial_spline", 0xA1, 0xA2),
    ("r2_spatial_leader", 0xA1, 0xA2),
    ("r3_forced_canonical_read_failure", 0xA1, 0xA2),
    ("r4_raw_insert_edit_refusal_and_save", 0xE1, 0xE4),
    ("r5_raw_insert_with_full_tables", 0x200, 0x90),
    ("r6_raw_polyline_with_full_tables", 0x200, 0x90),
];

fn hooks() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn session(d: Drawing) -> Session {
    let mut s = Session::empty();
    s.open_drawing(d, "x.dxf", None);
    s
}

fn hex(handle: u64) -> String {
    format!("{handle:X}")
}

fn raw_handles(d: &Drawing) -> Vec<Handle> {
    d.model.iter().filter(|e| matches!(e.kind, EntityKind::Raw(_))).map(|e| e.handle).collect()
}

/// The handles of the records of the ENTITIES section of a DXF text.
fn entity_record_handles(text: &str) -> Vec<Handle> {
    let tags = parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| records(&s.tags))
        .filter_map(|(_, body)| body.iter().find(|t| t.code == 5).and_then(|t| Handle::parse_hex(&t.str())))
        .collect()
}

#[test]
fn every_command_that_copies_moves_or_wraps_a_unit_refuses_and_changes_nothing() {
    for (name, unit, line) in FIXTURES {
        let commands = |handles: Vec<String>| {
            [
                ("copy", json!({"handles": handles, "delta": [1, 1]})),
                ("move", json!({"handles": handles, "delta": [1, 1]})),
                ("rotate", json!({"handles": handles, "base": [0, 0], "angle": 90})),
                ("scale", json!({"handles": handles, "base": [0, 0], "factor": 2})),
                ("mirror", json!({"handles": handles, "p1": [0, 0], "p2": [0, 1]})),
                ("explode", json!({"handles": handles})),
                ("block", json!({"name": "B1", "base": [0, 0], "handles": handles})),
                ("copyclip", json!({"handles": handles})),
                ("cutclip", json!({"handles": handles})),
            ]
        };
        for handles in [vec![hex(unit)], vec![hex(line), hex(unit)]] {
            for (command, params) in commands(handles) {
                let mut s = session(read_fixture(name).0);
                let before = s.doc().unwrap().clone();
                let error = s.execute(command, &params).expect_err(&format!("{name} {command}")).to_string();
                assert!(error.contains(&hex(unit)) && error.contains("read-only"), "{name} {command}: {error}");
                assert_eq!(s.doc().unwrap(), &before, "{name} {command}: the drawing is as it was");
                assert!(s.clipboard.is_empty(), "{name} {command}: nothing reached the clipboard");
            }
        }
    }
}

#[test]
fn the_line_beside_a_unit_copies_and_reaches_the_clipboard_and_the_unit_stays_once() {
    for (name, unit, line) in FIXTURES {
        let mut s = session(read_fixture(name).0);
        let copied = s.execute("copy", &json!({"handles": [hex(line)], "delta": [1, 1]})).unwrap();
        assert_eq!(copied["handles"].as_array().map(Vec::len), Some(1), "{name}");
        s.execute("copyclip", &json!({"handles": [hex(line)]})).unwrap();
        assert_eq!(s.clipboard.len(), 1, "{name}");
        assert!(matches!(s.clipboard[0].kind, EntityKind::Line(_)), "{name}");
        assert_eq!(raw_handles(s.doc().unwrap()), [Handle(unit)], "{name}: still one unit");
    }
}

#[test]
fn erase_takes_the_whole_unit_by_its_handle_and_the_save_writes_none_of_its_records() {
    for (name, unit, _) in FIXTURES {
        let d = read_fixture(name).0;
        let EntityKind::Raw(raw) = &d.entity(Handle(unit)).unwrap().kind else { panic!("{name}") };
        let unit_records: Vec<Handle> = raw.records().map(|record| record.handle()).collect();
        let mut s = session(d);
        s.execute("erase", &json!({"handles": [hex(unit)]})).unwrap();
        assert!(raw_handles(s.doc().unwrap()).is_empty(), "{name}");
        let saved = cadcraft_io::write_dxf(s.doc().unwrap());
        let left = entity_record_handles(&saved);
        assert!(unit_records.iter().all(|h| !left.contains(h)), "{name}: {unit_records:?} left in {left:?}");
        // The unit comes back with undo, whole.
        s.execute("undo", &json!({})).unwrap();
        assert_eq!(raw_handles(s.doc().unwrap()), [Handle(unit)], "{name}");
    }
}

#[test]
fn an_xref_of_a_fixture_binds_without_touching_the_unit_the_host_holds() {
    hooks();
    // The host holds the unit of r4 (read raw). It references r6, which the xref loader reads through the io hooks, with the raw path off
    // (stage 0): that read is Legacy, so no unit enters the host through the xref, and the bind does not touch the host's own.
    let folder = temp_dir::TempDir::new("raw-xref-bind");
    let xref = folder.file("poly.dxf", std::fs::read(dir().join("r6_raw_polyline_with_full_tables.dxf")).unwrap());
    let (host, source) = read_fixture("r4_raw_insert_edit_refusal_and_save");
    let mut s = session(host);
    s.execute("xattach", &json!({"path": xref, "at": [100, 0]})).unwrap();
    let before = s.doc().unwrap().entity(Handle(0xE1)).unwrap().clone();
    assert!(s.doc().unwrap().block("poly").is_some_and(|b| b.entities.iter().all(|e| !matches!(e.kind, EntityKind::Raw(_)))));
    s.execute("-xref", &json!({"option": "bind", "names": "poly"})).unwrap();
    let d = s.doc().unwrap();
    assert!(d.blocks.values().all(|b| b.entities.iter().all(|e| !matches!(e.kind, EntityKind::Raw(_)))), "no unit entered a block through the xref");
    assert_eq!(raw_handles(d), [Handle(0xE1)]);
    assert_eq!(d.entity(Handle(0xE1)).unwrap(), &before);
    // The save still writes the host's unit as it was read.
    let saved = cadcraft_io::write_dxf(d);
    let kept = |text: &str| -> Vec<Vec<(i32, String)>> {
        let tags = parse(text.as_bytes()).unwrap();
        sections(&tags)
            .into_iter()
            .filter(|s| s.name == "ENTITIES")
            .flat_map(|s| records(&s.tags))
            .filter(|(_, body)| body.iter().any(|t| t.code == 5 && ["E1", "E2", "E3"].contains(&t.str().trim())))
            .map(|(_, body)| body.iter().filter(|t| t.code != 330).map(|t| (t.code, t.str())).collect())
            .collect()
    };
    assert_eq!(kept(&saved), kept(&source));
}

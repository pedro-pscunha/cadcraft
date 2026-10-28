//! All entity duplication paths assign fresh instance identities before they reach a writer.

use std::collections::HashSet;
use std::sync::Arc;

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Block, Common, EntityKind, Handle, Insert, Space};
use cadcraft_engine::geom::Vec3;
use serde_json::json;

fn session() -> Session {
    let d = cadcraft_io::read_dxf(include_bytes!("../../io/tests/data/attrib_fields.dxf")).unwrap();
    let mut s = Session::new();
    s.open_drawing(d, "attributes.dxf", None);
    s
}

fn assert_fresh(s: &Session, stage: &str) {
    let d = s.doc().unwrap();
    let entities: Vec<_> = std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|st| st.iter())
        .collect();
    let mut used: HashSet<_> = entities.iter().map(|e| e.handle).collect();
    for e in entities {
        for a in e.kind.attributes() {
            let h = a.handle.expect("instance identity assigned before serialization");
            assert!(used.insert(h), "{stage}: attribute {h:?} has two owners");
            if e.handle != Handle(0x31) {
                assert!(![Handle(0x32), Handle(0x33)].contains(&h), "{stage}: new owner reused source attribute {h:?}");
            }
        }
    }
    let text = cadcraft_io::write_dxf(d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let mut used = HashSet::new();
    for section in cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name != "HEADER") {
        for (kind, body) in cadcraft_dxf::records(&section.tags) {
            for t in body.iter().filter(|t| matches!(t.code, 5 | 105)) {
                assert!(used.insert(Handle::parse_hex(&t.str()).unwrap()), "{stage}: duplicate file handle on {kind}");
            }
        }
    }
}

fn hooks() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn pasted(command: &str) {
    let mut s = session();
    s.execute("copyclip", &json!({"handles":["31"]})).unwrap();
    s.execute(command, &json!({"at":[20,0]})).unwrap();
    assert_fresh(&s, command);
}

#[test]
fn pasteclip_duplicates_attribute_instances() {
    pasted("pasteclip");
}

#[test]
fn pasteblock_duplicates_attribute_instances() {
    pasted("pasteblock");
}

#[test]
fn insert_file_drops_source_identity_without_aliasing_host_attributes() {
    hooks();
    let file = std::env::temp_dir().join(format!("cadcraft-attrib-import-{}.dxf", std::process::id()));
    std::fs::write(&file, include_bytes!("../../io/tests/data/attrib_fields.dxf")).unwrap();
    let mut s = session();
    s.execute("insert", &json!({"name":"IMPORTED", "file":file, "at":[20,0]})).unwrap();
    assert_fresh(&s, "INSERT-file");
    std::fs::remove_file(file).unwrap();
}

#[test]
fn block_retain_allocates_fresh_instances() {
    let mut s = session();
    s.execute("block", &json!({"name":"RETAINED", "handles":["31"], "keep":"retain", "base":[0,0]})).unwrap();
    assert_fresh(&s, "BLOCK Retain");
}

#[test]
fn associative_array_allocates_fresh_instances() {
    let mut s = session();
    s.execute("arrayrect", &json!({"handles":["31"], "rows":2, "cols":1, "rowSpacing":10, "colSpacing":10})).unwrap();
    assert_fresh(&s, "ARRAY");
}

#[test]
fn explode_nested_attributed_insert_duplicates_its_instances() {
    let mut s = session();
    let d = s.doc_mut().unwrap();
    let original = d.remove_entity(Handle(0x31)).unwrap();
    let mut outer = Block::new("OUTER");
    outer.entities.push((*original).clone());
    d.blocks.insert("OUTER".into(), Arc::new(outer));
    let h = d
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::Insert(Insert {
                block: "OUTER".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: vec![],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        )
        .unwrap();
    s.execute("explode", &json!({"handles":[h.hex()]})).unwrap();
    assert_fresh(&s, "EXPLODE nested INSERT");
}

#[test]
fn grip_stretch_copy_duplicates_attribute_instances() {
    let mut s = session();
    s.execute("grip.stretch", &json!({"grips":[["31",0]],"delta":[20,0],"copy":true})).unwrap();
    assert_fresh(&s, "grip stretch-copy");
}

#[test]
fn refedit_projection_keeps_original_identity_and_saves_only_one_field_graph() {
    let mut s = session();
    let d = s.doc_mut().unwrap();
    let original = d.remove_entity(Handle(0x31)).unwrap();
    let mut outer = Block::new("OUTER");
    outer.entities.push((*original).clone());
    d.blocks.insert("OUTER".into(), Arc::new(outer));
    let root = d
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::Insert(Insert {
                block: "OUTER".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: vec![],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        )
        .unwrap();
    s.execute("refedit", &json!({"handle":root.hex()})).unwrap();
    for close in [false, true] {
        if close {
            s.execute("refclose", &json!({"save":true})).unwrap();
        }
        for dwg in [false, true] {
            let text = if dwg {
                let bytes = cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap();
                cadcraft_io::write_dxf(&cadcraft_io::read(&bytes, "x.dwg").unwrap())
            } else {
                cadcraft_io::write_dxf(s.doc().unwrap())
            };
            let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
            let mut used = HashSet::new();
            let records: Vec<_> =
                cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name != "HEADER").flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
            for (kind, body) in &records {
                for t in body.iter().filter(|t| matches!(t.code, 5 | 105)) {
                    assert!(used.insert(t.str()), "REFEDIT close={close} DWG={dwg}: duplicate {kind} handle");
                }
            }
            let attributes: Vec<_> = records.iter().filter(|(k, _)| k == "ATTRIB").collect();
            assert_eq!(attributes.len(), 2, "REFEDIT close={close} DWG={dwg}: both projection and original were written");
            let live = &attributes.iter().find(|(_, t)| t.iter().any(|t| t.code == 1 && t.str() == "live")).unwrap().1;
            assert!(live.iter().any(|t| t.code == 5 && t.str() == "33"), "REFEDIT close={close} DWG={dwg}: original attribute identity changed");
            assert!(live.iter().any(|t| t.code == 1005 && t.str() == "33"), "REFEDIT close={close} DWG={dwg}: original self reference changed");
            assert_eq!(records.iter().filter(|(k, _)| k == "FIELD").count(), 2, "REFEDIT close={close} DWG={dwg}: FIELD graph was lost or doubled");
        }
    }
}

#[test]
fn copying_plain_geometry_does_not_clone_unrelated_foreign_storage() {
    let mut s = session();
    let snapshot = s.doc().unwrap().foreign.clone();
    s.execute("copy", &json!({"handles":["30"],"delta":[20,0],"count":20})).unwrap();
    assert!(Arc::ptr_eq(&snapshot, &s.doc().unwrap().foreign), "copying geometry without foreign extras must not clone the complete Foreign store");
}

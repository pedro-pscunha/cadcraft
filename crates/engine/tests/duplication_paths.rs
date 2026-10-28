//! All entity duplication paths assign fresh instance identities before they reach a writer.

use std::collections::HashSet;
use std::sync::Arc;

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Insert, Space};
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
    let mut used = HashSet::new();
    for e in &entities {
        assert_ne!(e.handle, Handle(0), "{stage}: zero entity handle");
        assert!(used.insert(e.handle), "{stage}: entity {:?} has two owners", e.handle);
    }
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
                assert!(used.insert(Handle::parse_hex(&t.str()).unwrap()), "{stage}: duplicate file handle {} on {kind}", t.str());
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

fn handle(v: &serde_json::Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

/// The FIELD fixture's live attribute owns two dictionaries and two FIELD objects. Every copy
/// must own its own graph, including the reactor and the attribute's 1005 self-reference.
fn assert_field_copy(d: &Drawing, source: Handle, copy: Handle, stage: &str) {
    let old = d.foreign.entities.get(&source).unwrap();
    let new = d.foreign.entities.get(&copy).expect("copied attribute retained foreign data");
    assert_ne!(new.xdict, old.xdict, "{stage}: copied dictionary aliases the source");
    assert_ne!(new.reactors, old.reactors, "{stage}: copied reactor aliases the source");
    assert!(
        new.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(copy)),
        "{stage}: xdata must reference the copied attribute"
    );
    let mut owner = copy;
    let mut object = new.xdict.unwrap();
    for kind in ["DICTIONARY", "DICTIONARY", "FIELD", "FIELD"] {
        let node = d.foreign.objects.iter().find(|o| o.handle == object).unwrap();
        assert_eq!(node.kind, kind, "{stage}: copied FIELD graph");
        assert!(
            node.tags.iter().any(|t| t.code == 330 && Handle::parse_hex(&t.value) == Some(owner)),
            "{stage}: foreign owner must reference the copy"
        );
        owner = object;
        if let Some(next) = node.tags.iter().find(|t| t.code == 360) {
            object = Handle::parse_hex(&next.value).unwrap();
        }
    }
    assert_eq!(new.reactors, [object], "{stage}: reactor must reference the copied FIELD");
}

fn assert_insert_copy(d: &Drawing, source: &Entity, copy: &Entity, same_drawing: bool, stage: &str) {
    let source_handles: HashSet<_> = std::iter::once(source.handle).chain(source.kind.attributes().iter().filter_map(|a| a.handle)).collect();
    assert!(!source_handles.contains(&copy.handle), "{stage}: copied entity retained source identity");
    assert_eq!(copy.kind.attributes().len(), 2, "{stage}: copied INSERT lost attributes");
    for (a, b) in source.kind.attributes().iter().zip(copy.kind.attributes()) {
        let old = a.handle.unwrap();
        let new = b.handle.expect("copied attribute has identity");
        assert!(!source_handles.contains(&new), "{stage}: copied attribute retained source identity");
        assert_eq!(a.text.value, b.text.value, "{stage}: copied attribute value");
        if a.text.value == "live" {
            if same_drawing {
                assert_field_copy(d, old, new, stage);
            } else {
                // External raw namespaces are deliberately not adopted. In particular, handle 33
                // in an imported file must not borrow the host's unrelated FIELD graph.
                assert!(!d.foreign.entities.contains_key(&new), "{stage}: imported attribute aliases host foreign data");
            }
        }
    }
}

fn assert_dwg_handles(s: &Session, stage: &str) {
    let bytes = cadcraft_io::write(s.doc().unwrap(), "copy.dwg").unwrap();
    let mut read = Session::new();
    read.open_drawing(cadcraft_io::read(&bytes, "copy.dwg").unwrap(), "copy.dwg", None);
    assert_fresh(&read, stage);
}

fn fixture_file(tag: &str, d: Option<&Drawing>) -> std::path::PathBuf {
    hooks();
    let path = std::env::temp_dir().join(format!("cadcraft-copy-path-{}-{tag}.dxf", std::process::id()));
    let bytes = d.map_or_else(|| include_bytes!("../../io/tests/data/attrib_fields.dxf").to_vec(), |d| cadcraft_io::write_dxf(d).into_bytes());
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Move the fixture INSERT into a nested block without duplicating it. The foreign graph keeps
/// the same owner; subsequent commands, not fixture setup, must allocate the new identities.
fn nest_fixture(s: &mut Session) {
    let d = s.doc_mut().unwrap();
    let original = d.remove_entity(Handle(0x31)).unwrap();
    let mut b = Block::new("CARRIER");
    b.entities.push((*original).clone());
    d.blocks.insert(b.name.clone(), Arc::new(b));
    s.execute("insert", &json!({"name":"CARRIER", "at":[0,0]})).unwrap();
}

fn transformed_copy(command: &str, params: serde_json::Value, count: usize) {
    let mut s = session();
    let source = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let r = s.execute(command, &params).unwrap();
    assert_eq!(s.doc().unwrap().entity(source.handle).unwrap(), &source, "{command}: the source must be retained unchanged");
    let copies = r["handles"].as_array().unwrap();
    assert_eq!(copies.len(), count);
    for h in copies {
        assert_insert_copy(s.doc().unwrap(), &source, s.doc().unwrap().entity(handle(h)).unwrap(), true, command);
    }
    assert_fresh(&s, command);
    assert_dwg_handles(&s, command);
}

#[test]
fn mirror_keep_source_duplicates_attribute_field_graphs() {
    transformed_copy("mirror", json!({"handles":["31"], "p1":[10,0], "p2":[10,10], "erase":false}), 1);
}

#[test]
fn copy_duplicates_attribute_field_graphs() {
    transformed_copy("copy", json!({"handles":["31"], "delta":[20,0], "count":2}), 2);
}

#[test]
fn import_duplicates_model_and_nested_attribute_instances() {
    let mut external = session();
    nest_fixture(&mut external);
    let path = fixture_file("import", Some(external.doc().unwrap()));
    let mut s = session();
    let source = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let r = s.execute("import", &json!({"path":path, "at":[20,0]})).unwrap();
    assert_eq!(r["imported"], 2);
    let nested = s.doc().unwrap().block("CARRIER").unwrap().entities.iter().next().unwrap();
    assert_insert_copy(s.doc().unwrap(), &source, nested, false, "IMPORT block");
    assert_fresh(&s, "IMPORT");
    assert_dwg_handles(&s, "IMPORT");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn import_duplicates_top_level_attribute_instances() {
    let path = fixture_file("import-model", None);
    let mut s = session();
    let source = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let r = s.execute("import", &json!({"path":path, "at":[20,0]})).unwrap();
    assert_eq!(r["imported"], 2);
    let d = s.doc().unwrap();
    let copy =
        r["handles"].as_array().unwrap().iter().map(handle).filter_map(|h| d.entity(h)).find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap();
    assert_insert_copy(d, &source, copy, false, "IMPORT model");
    assert_fresh(&s, "IMPORT model");
    assert_dwg_handles(&s, "IMPORT model");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn xref_load_duplicates_top_level_attribute_instances() {
    let path = fixture_file("xref-model", None);
    let mut s = session();
    let source = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let r = s.execute("xattach", &json!({"path":path, "at":[20,0]})).unwrap();
    let d = s.doc().unwrap();
    let loaded = d.block(r["name"].as_str().unwrap()).unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap();
    assert_insert_copy(d, &source, loaded, false, "XREF model");
    assert_fresh(&s, "XREF model");
    assert_dwg_handles(&s, "XREF model");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn xref_load_and_xbind_duplicate_nested_attribute_instances() {
    let mut external = session();
    nest_fixture(&mut external);
    let path = fixture_file("xref", Some(external.doc().unwrap()));
    let mut s = session();
    let source = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let r = s.execute("xattach", &json!({"path":path, "at":[20,0]})).unwrap();
    let dependent = format!("{}|CARRIER", r["name"].as_str().unwrap());
    let loaded = s.doc().unwrap().block(&dependent).unwrap().entities.iter().next().unwrap().clone();
    assert_insert_copy(s.doc().unwrap(), &source, &loaded, false, "XREF load");
    assert_fresh(&s, "XREF load");
    let r = s.execute("xbind", &json!({"type":"block", "names":dependent})).unwrap();
    assert!(r["bound"].as_array().is_some_and(|a| !a.is_empty()), "XBIND must bind a block: {r}");
    let bound = s.doc().unwrap().blocks.values().find(|b| b.name.contains("$0$CARRIER")).unwrap().entities.iter().next().unwrap();
    assert_insert_copy(s.doc().unwrap(), &loaded, bound, false, "XBIND");
    assert_fresh(&s, "XBIND");
    assert_dwg_handles(&s, "XBIND");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn layout_copy_duplicates_attribute_field_graphs() {
    let mut s = session();
    s.execute("layout.new", &json!({"name":"Source"})).unwrap();
    let d = s.doc_mut().unwrap();
    let original = d.remove_entity(Handle(0x31)).unwrap();
    d.space_mut(&Space::Paper("Source".into())).unwrap().push((*original).clone());
    s.execute("layout.copy", &json!({"from":"Source", "to":"Copy"})).unwrap();
    let d = s.doc().unwrap();
    let copy = d.layout("Copy").unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap();
    assert_insert_copy(d, &original, copy, true, "LAYOUT copy");
    assert_fresh(&s, "LAYOUT copy");
    assert_dwg_handles(&s, "LAYOUT copy");
}

#[test]
fn wblock_duplicates_model_and_nested_attribute_instances() {
    for nested in [false, true] {
        for extension in ["dxf", "dwg"] {
            hooks();
            let mut s = session();
            let original = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
            if nested {
                nest_fixture(&mut s);
            }
            let path = std::env::temp_dir().join(format!("cadcraft-copy-wblock-{}-{nested}.{extension}", std::process::id()));
            let handles: Vec<_> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
            s.execute("wblock", &json!({"path":path, "handles":handles})).unwrap();
            let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path.to_string_lossy()).unwrap();
            let copies: Vec<_> =
                d.model.iter().chain(d.blocks.values().flat_map(|b| b.entities.iter())).filter(|e| e.kind.attributes().len() == 2).collect();
            assert_eq!(copies.len(), 1, "WBLOCK must write its attributed INSERT");
            assert_insert_copy(&d, &original, copies[0], false, "WBLOCK");
            let mut read = Session::new();
            read.open_drawing(d, "written", None);
            assert_fresh(&read, "WBLOCK");
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn compare_export_duplicates_group_tinted_and_compared_block_attributes() {
    let mut s = session();
    let original = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
    let top = s.execute("copy", &json!({"handles":["31"], "delta":[20,0]})).unwrap()["handles"][0].clone();
    nest_fixture(&mut s);
    s.open_drawing(s.doc().unwrap().clone(), "changed.dxf", None);
    s.execute("move", &json!({"handles":[top], "delta":[10,0]})).unwrap();
    // A changed nested INSERT forces copy_blocks as well as paint's tinted_block path.
    let d = s.doc_mut().unwrap();
    let b = Arc::make_mut(d.blocks.get_mut("CARRIER").unwrap());
    let mut nested = (**b.entities.iter().next().unwrap()).clone();
    if let EntityKind::Insert(i) = &mut nested.kind {
        i.insert.x = 10.0;
    }
    b.entities = Default::default();
    b.entities.push(nested);
    s.execute("compare", &json!({"document":"attributes.dxf"})).unwrap();
    s.execute("compareexport", &json!({})).unwrap();
    let d = s.doc().unwrap();
    let copies: Vec<_> = d
        .model
        .iter()
        .chain(d.blocks.values().flat_map(|b| b.entities.iter()))
        .filter(|e| e.handle != original.handle && e.kind.attributes().len() == 2)
        .collect();
    assert!(copies.len() >= 4, "COMPAREEXPORT must copy top-level and nested INSERTs from both drawings");
    for copy in copies {
        assert_insert_copy(d, &original, copy, false, "COMPAREEXPORT");
    }
    assert_fresh(&s, "COMPAREEXPORT");
    assert_dwg_handles(&s, "COMPAREEXPORT");
}

#[test]
fn dynamic_block_apply_duplicates_nested_attribute_field_graphs() {
    let text: String = include_str!("../../io/tests/data/dynblock_door.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (code, value) = line.split_once(' ').unwrap_or((line, ""));
            format!("{code}\n{value}\n")
        })
        .collect();
    let mut d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let fixture = session();
    let fixture = fixture.doc().unwrap();
    // Give the added fixture a disjoint namespace; don't use the duplicator under test to set up
    // the source graph. The dynamic fixture already uses 30..32 for the Door's geometry.
    let renumber = |h: Handle| Handle(h.0 + 0x1000);
    let mut source = (**fixture.entity(Handle(0x31)).unwrap()).clone();
    source.handle = renumber(source.handle);
    for attribute in source.kind.attributes_mut() {
        attribute.handle = attribute.handle.map(renumber);
    }
    Arc::make_mut(d.blocks.get_mut("Door").unwrap()).entities.push(source.clone());
    let mut label = (**fixture.block("LABEL").unwrap()).clone();
    let entities = label
        .entities
        .iter()
        .map(|e| {
            let mut e = (**e).clone();
            e.handle = renumber(e.handle);
            e
        })
        .collect::<Vec<_>>();
    label.entities = Default::default();
    for e in entities {
        label.entities.push(e);
    }
    d.blocks.insert("LABEL".into(), Arc::new(label));
    let mut line = (**fixture.entity(Handle(0x30)).unwrap()).clone();
    line.handle = renumber(line.handle);
    d.model.push(line);
    let foreign = Arc::make_mut(&mut d.foreign);
    foreign.classes.extend(fixture.foreign.classes.clone());
    for object in fixture.foreign.objects.iter().filter(|o| (0x40..=0x43).contains(&o.handle.0)) {
        let mut object = object.clone();
        object.handle = renumber(object.handle);
        for t in &mut object.tags {
            if matches!(t.code, 5 | 330 | 331 | 360) {
                t.value = renumber(Handle::parse_hex(&t.value).unwrap()).hex();
            }
        }
        foreign.objects.push(object);
    }
    for (h, extra) in &fixture.foreign.entities {
        let mut extra = extra.clone();
        extra.xdict = extra.xdict.map(renumber);
        extra.reactors.iter_mut().for_each(|h| *h = renumber(*h));
        for t in extra.xdata.iter_mut().filter(|t| t.code == 1005) {
            t.value = renumber(Handle::parse_hex(&t.value).unwrap()).hex();
        }
        foreign.entities.insert(renumber(*h), extra);
    }
    d.handseed = 0x2000;
    let mut s = Session::new();
    s.open_drawing(d, "dynamic.dxf", None);
    let h = s.execute("insert", &json!({"name":"Door", "at":[100,0]})).unwrap()["handle"].clone();
    assert_fresh(&s, "dynamic source before apply");
    s.execute("blocks.reference.set", &json!({"handle":h, "name":"Width", "value":"30"})).unwrap();
    let d = s.doc().unwrap();
    let EntityKind::Insert(i) = &d.entity(handle(&h)).unwrap().kind else { panic!("dynamic reference") };
    assert_ne!(i.block, "Door");
    let copy = d.block(&i.block).unwrap().entities.iter().find(|e| e.kind.attributes().len() == 2).unwrap();
    assert_insert_copy(d, &source, copy, true, "dynamic apply");
    assert_fresh(&s, "dynamic apply");
    assert_dwg_handles(&s, "dynamic apply");
}

#[test]
fn lisp_end_block_assigns_nonzero_unique_entity_handles() {
    cadcraft_engine::cmd::file::set_entity_codec(cadcraft_engine::cmd::file::EntityCodec {
        groups: cadcraft_io::entity_groups,
        entity: cadcraft_io::entity_from_groups,
    });
    // entmake's INSERT codec doesn't accept ATTRIB/SEQEND children; the public LISP path can
    // exercise entity allocation here but cannot supply an existing attributed INSERT graph.
    let mut s = session();
    let r = s
        .execute(
            "lisp",
            &json!({"code":r#"(progn
        (entmake '((0 . "BLOCK") (2 . "LISP_COPY") (70 . 0) (10 0.0 0.0 0.0)))
        (entmake '((0 . "LINE") (10 0.0 0.0 0.0) (11 10.0 0.0 0.0)))
        (entmake '((0 . "LINE") (10 0.0 1.0 0.0) (11 10.0 1.0 0.0)))
        (entmake '((0 . "ENDBLK"))))"#}),
        )
        .unwrap();
    assert!(r.get("error").is_none(), "{r}");
    assert_eq!(r["value"], "\"LISP_COPY\"");
    assert_eq!(s.doc().unwrap().block("LISP_COPY").unwrap().entities.len(), 2);
    assert_fresh(&s, "LISP end_block");
    assert_dwg_handles(&s, "LISP end_block");
}

#[test]
fn non_insert_explode_does_not_attach_source_xdata_or_dictionary_to_pieces() {
    let mut s = session();
    let h = s.execute("pline", &json!({"points":[[0,0],[10,0],[10,10]]})).unwrap()["handle"].clone();
    let h = handle(&h);
    let d = s.doc_mut().unwrap();
    d.modify_entity(h, |e| {
        e.common.xdata =
            vec![cadcraft_engine::doc::RawTag { code: 1001, value: "COPY_TEST".into() }, cadcraft_engine::doc::RawTag { code: 1005, value: h.hex() }];
    })
    .unwrap();
    // This object has an owned xdict as well as xdata. Exploding a primitive deliberately
    // drops both instead of assigning the old object's metadata to every segment.
    let dictionary = d.new_handle();
    let record = d.new_handle();
    let tag = |code, value: String| cadcraft_engine::doc::RawTag { code, value };
    let foreign = Arc::make_mut(&mut d.foreign);
    foreign.entities.insert(h, cadcraft_engine::doc::foreign::ForeignExtra { xdict: Some(dictionary), ..Default::default() });
    foreign.objects.extend([
        cadcraft_engine::doc::foreign::ForeignObject {
            handle: dictionary,
            kind: "DICTIONARY".into(),
            tags: vec![
                tag(5, dictionary.hex()),
                tag(330, h.hex()),
                tag(100, "AcDbDictionary".into()),
                tag(3, "COPY_TEST".into()),
                tag(350, record.hex()),
            ],
        },
        cadcraft_engine::doc::foreign::ForeignObject {
            handle: record,
            kind: "XRECORD".into(),
            tags: vec![tag(5, record.hex()), tag(330, dictionary.hex()), tag(100, "AcDbXrecord".into()), tag(1, "source metadata".into())],
        },
    ]);
    let r = s.execute("explode", &json!({"handles":[h.hex()]})).unwrap();
    let copies = r["handles"].as_array().unwrap();
    assert_eq!(copies.len(), 2);
    for copy in copies {
        let copy = handle(copy);
        assert_ne!(copy, h);
        let d = s.doc().unwrap();
        assert!(d.entity(copy).unwrap().common.xdata.is_empty(), "EXPLODE piece inherited source xdata");
        assert!(!d.foreign.entities.contains_key(&copy), "EXPLODE piece inherited source dictionary");
    }
    assert_fresh(&s, "non-INSERT EXPLODE");
    assert_dwg_handles(&s, "non-INSERT EXPLODE");
}

#[test]
fn array_edit_copy_paths_duplicate_attribute_field_graphs() {
    for action in ["replace", "base", "source"] {
        let mut s = session();
        let original = s.doc().unwrap().entity(Handle(0x31)).unwrap().clone();
        let replacement = s.execute("copy", &json!({"handles":["31"], "delta":[20,0]})).unwrap()["handles"][0].clone();
        let r = s.execute("arrayrect", &json!({"handles":["31"], "rows":1, "cols":2, "colSpacing":10})).unwrap();
        let array = r["handle"].clone();
        let params = match action {
            "replace" => json!({"handle":array, "replace":{"handles":[replacement], "base":[0,0], "items":[[1,0,0]]}}),
            "base" => json!({"handle":array, "base":[3,4]}),
            _ => json!({"handle":array, "source":{}}),
        };
        let before: HashSet<_> = s.doc().unwrap().blocks.values().flat_map(|b| b.entities.iter()).map(|e| e.handle).collect();
        let previous_attributes: HashSet<_> = s
            .doc()
            .unwrap()
            .blocks
            .values()
            .flat_map(|b| b.entities.iter())
            .chain(s.doc().unwrap().model.iter())
            .flat_map(|e| e.kind.attributes().iter().filter_map(|a| a.handle))
            .collect();
        let mut edited_attributes = HashSet::new();
        s.execute("arrayedit", &params).unwrap();
        assert_fresh(&s, action);
        if action == "source" {
            let edited = s.selection().to_vec();
            assert_eq!(edited.len(), 1);
            let e = s.doc().unwrap().entity(edited[0]).unwrap();
            assert_insert_copy(s.doc().unwrap(), &original, e, true, "ARRAY open_source");
            edited_attributes.extend(e.kind.attributes().iter().filter_map(|a| a.handle));
            s.execute("arrayclose", &json!({"save":true})).unwrap();
            assert_fresh(&s, "ARRAY close_source");
        }
        let d = s.doc().unwrap();
        let copies: Vec<_> = d.blocks.values().flat_map(|b| b.entities.iter()).filter(|e| e.kind.attributes().len() == 2).collect();
        assert!(copies.iter().any(|e| !before.contains(&e.handle)), "{action}: ARRAYEDIT must create a new attributed source block");
        for copy in copies {
            assert_insert_copy(d, &original, copy, true, action);
            if !before.contains(&copy.handle) {
                for h in copy.kind.attributes().iter().filter_map(|a| a.handle) {
                    assert!(!previous_attributes.contains(&h), "{action}: new source reused a previous attribute identity");
                    assert!(!edited_attributes.contains(&h), "ARRAY close_source reused an editing attribute identity");
                }
            }
        }
        assert_dwg_handles(&s, action);
    }
}

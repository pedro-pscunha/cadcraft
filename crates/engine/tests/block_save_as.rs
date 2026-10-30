//! BSAVEAS owns a new namespace across the editor's complete history, not only today's model.
use cadcraft_engine::Session;
use cadcraft_engine::doc::{Drawing, EntityKind, Handle};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn line(s: &mut Session, y: i32) -> Handle {
    Handle::parse_hex(s.execute("line", &json!({"points":[[0,y],[10,y]]})).unwrap()["handles"][0].as_str().unwrap()).unwrap()
}
fn block(s: &mut Session, name: &str, handles: &[Handle]) -> String {
    s.execute("block", &json!({"name":name,"handles":handles.iter().map(|h|h.hex()).collect::<Vec<_>>(),"base":[0,0]})).unwrap()["insert"]
        .as_str()
        .unwrap()
        .to_string()
}
fn identities(d: &Drawing, name: &str) -> BTreeSet<Handle> {
    d.block(name)
        .unwrap()
        .entities
        .iter()
        .flat_map(|e| std::iter::once(e.handle).chain(e.kind.attributes().iter().filter_map(|a| a.handle)))
        .collect()
}
fn assert_file_unique(d: &Drawing) {
    let tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(d).as_bytes()).unwrap();
    let mut used = BTreeSet::new();
    for section in cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name != "HEADER") {
        for (kind, body) in cadcraft_dxf::records(&section.tags) {
            // Later group 5 values in SORTENTSTABLE are sort keys, not object identities.
            if let Some(t) = body.iter().find(|t| matches!(t.code, 5 | 105)) {
                let h = Handle::parse_hex(&t.str()).unwrap();
                assert!(h.0 != 0 && used.insert(h), "duplicate/zero identity {h:?} in {kind}");
            }
        }
    }
}
fn roundtrips(s: &Session) -> Vec<Drawing> {
    let d = s.state().unwrap().drawing();
    assert_file_unique(d);
    [false, true]
        .into_iter()
        .map(|dwg| {
            let bytes = if dwg { cadcraft_io::write(d, "x.dwg").unwrap() } else { cadcraft_io::write_dxf(d).into_bytes() };
            let back = cadcraft_io::read(&bytes, if dwg { "x.dwg" } else { "x.dxf" }).unwrap();
            assert_file_unique(&back);
            back
        })
        .collect()
}
fn assert_distinct(d: &Drawing) {
    assert!(identities(d, "ORIGINAL").is_disjoint(&identities(d, "COPY")), "BSAVEAS reused source definition identities");
}

#[test]
fn save_as_creates_distinct_definitions_and_later_save_never_reintroduces_source_ids() {
    let mut s = Session::new();
    let h = line(&mut s, 0);
    block(&mut s, "ORIGINAL", &[h]);
    let original = s.doc().unwrap().block("ORIGINAL").unwrap().clone();
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().block("ORIGINAL").unwrap(), &original);
    for d in roundtrips(&s) {
        assert_distinct(&d);
        assert_eq!(d.block("ORIGINAL").unwrap().entities.len(), 1);
        assert_eq!(d.block("COPY").unwrap().entities.len(), 1);
    }
}

#[test]
fn save_as_keeps_nonzero_base_coordinates_and_replaces_an_existing_target() {
    let mut s = Session::new();
    let h = line(&mut s, 0);
    s.execute("block", &json!({"name":"ORIGINAL","handles":[h.hex()],"base":[10,20]})).unwrap();
    let old = s.doc().unwrap().block("ORIGINAL").unwrap().clone();
    let other = line(&mut s, 3);
    block(&mut s, "COPY", &[other]);
    let replaced = identities(s.doc().unwrap(), "COPY");
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    s.execute("bsaveas", &json!({"name":"COPY","replace":true})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().block("ORIGINAL").unwrap(), &old);
    assert!(identities(s.doc().unwrap(), "COPY").is_disjoint(&replaced));
    for d in roundtrips(&s) {
        assert_distinct(&d);
        let b = d.block("COPY").unwrap();
        assert!((b.base.x - 10.0).abs() < 1e-9 && (b.base.y - 20.0).abs() < 1e-9);
        let EntityKind::Line(line) = &b.entities.iter().next().unwrap().kind else { panic!("line") };
        assert!(line.a.x.abs() < 1e-9 && line.a.y.abs() < 1e-9 && (line.b.x - 10.0).abs() < 1e-9);
    }
}

#[test]
fn save_as_is_an_editor_history_boundary_and_host_undo_remains_coherent() {
    let mut s = Session::new();
    let a = line(&mut s, 0);
    let b = line(&mut s, 1);
    block(&mut s, "ORIGINAL", &[a, b]);
    let host = s.state().unwrap().doc.clone();
    let undo_depth = s.state().unwrap().undo.len();
    let original = s.doc().unwrap().block("ORIGINAL").unwrap().clone();
    let old = identities(s.doc().unwrap(), "ORIGINAL");
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    let shown: Vec<_> = s.doc().unwrap().model.iter().map(|e| e.handle).collect();
    s.set_selection(vec![shown[0]]);
    s.execute("move", &json!({"handles":[shown[0].hex()],"delta":[2,0]})).unwrap();
    s.execute("erase", &json!({"handles":[shown[1].hex()]})).unwrap();
    s.set_selection(vec![shown[0]]);
    let redo_only = line(&mut s, 2);
    s.execute("u", &json!({})).unwrap();
    s.set_selection(vec![shown[0]]);
    assert!(!s.state().unwrap().undo.is_empty() && !s.state().unwrap().redo.is_empty());
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    assert!(s.state().unwrap().undo.is_empty() && s.state().unwrap().redo.is_empty(), "BSAVEAS begins a new editor history");
    let new_a = s.doc().unwrap().model.iter().next().unwrap().handle;
    assert!(!old.contains(&new_a));
    assert_eq!(s.selection(), vec![new_a]);
    let copied = s.state().unwrap().doc.clone();
    s.execute("u", &json!({})).unwrap();
    s.execute("redo", &json!({})).unwrap();
    assert!(Arc::ptr_eq(&s.state().unwrap().doc, &copied), "old editor states must not be resurrected");
    let new_line = line(&mut s, 3);
    assert_ne!(new_line, redo_only);
    s.execute("u", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().block("ORIGINAL").unwrap(), &original);
    for d in roundtrips(&s) {
        assert_distinct(&d);
        assert_eq!(d.block("ORIGINAL").unwrap().entities.len(), 2);
        assert_eq!(d.block("COPY").unwrap().entities.len(), 2);
    }
    let steps = s.state().unwrap().undo.len() - undo_depth;
    assert!(steps >= 1);
    s.execute("undo", &json!({"count":steps})).unwrap();
    assert!(Arc::ptr_eq(&s.state().unwrap().doc, &host), "host undo must restore the state before entering BEDIT");
    assert!(s.doc().unwrap().block("COPY").is_none());
}

fn attribute_source() -> Session {
    let text = include_str!("../../io/tests/data/attrib_fields.dxf")
        .replace("1\nplain\n100\nAcDbAttribute\n2\nLENGTH\n", "1\nplain\n100\nAcDbAttribute\n2\nPLAIN\n");
    let d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let mut s = Session::new();
    s.open_drawing(d, "fields.dxf", None);
    block(&mut s, "ORIGINAL", &[Handle(0x30), Handle(0x31)]);
    let d = cadcraft_io::read_dxf(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    s.open_drawing(d, "clean-fields.dxf", None);
    s
}
fn field_owners(d: &Drawing) -> BTreeMap<Handle, usize> {
    let owners: BTreeMap<_, _> = d
        .foreign
        .objects
        .iter()
        .filter_map(|o| cadcraft_engine::doc::foreign::object_owner(o.tags.iter().map(|t| (t.code, t.value.as_str()))).map(|owner| (o.handle, owner)))
        .collect();
    let mut out = BTreeMap::new();
    for field in d.foreign.objects.iter().filter(|o| o.kind == "FIELD") {
        let mut h = field.handle;
        for _ in 0..64 {
            if let Some(parent) = owners.get(&h) {
                h = *parent;
            } else {
                break;
            }
        }
        *out.entry(h).or_insert(0) += 1;
    }
    out
}

#[test]
fn save_as_preserves_attribute_field_graphs_and_the_original_definition() {
    let mut s = attribute_source();
    let original = s.doc().unwrap().block("ORIGINAL").unwrap().clone();
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    let nested = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
    s.execute("attedit", &json!({"handle":nested.hex(),"values":{"LENGTH":"edited"}})).unwrap();
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    s.execute("u", &json!({})).unwrap();
    let nested = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
    s.execute("attedit", &json!({"handle":nested.hex(),"values":{"LENGTH":"edited-again"}})).unwrap();
    s.execute("u", &json!({})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("redo", &json!({})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().block("ORIGINAL").unwrap(), &original);
    for d in roundtrips(&s) {
        assert_distinct(&d);
        let owners = field_owners(&d);
        assert_eq!(owners.len(), 2);
        assert!(owners.values().all(|n| *n == 2), "both independent ATTRIB graphs must survive: {owners:?}");
        for name in ["ORIGINAL", "COPY"] {
            let block = d.block(name).unwrap();
            let insert = block.entities.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i) } else { None }).unwrap();
            let live = insert
                .attribs
                .iter()
                .find(|a| a.tag.eq_ignore_ascii_case("LENGTH"))
                .unwrap_or_else(|| panic!("{name}: missing LENGTH in {:?}", insert.attribs));
            assert_eq!(live.text.value, if name == "ORIGINAL" { "live" } else { "edited-again" });
            assert_eq!(owners.get(&live.handle.unwrap()), Some(&2));
            let line = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
            assert!(
                d.foreign
                    .objects
                    .iter()
                    .filter(|o| o.kind == "FIELD")
                    .flat_map(|o| &o.tags)
                    .any(|t| t.code == 331 && Handle::parse_hex(&t.value) == Some(line)),
                "{name}: FIELD target did not stay in its own copied namespace"
            );
        }
    }
}

#[test]
fn save_as_clones_named_record_metadata_and_restores_the_original_graph() {
    use cadcraft_engine::doc::{RawTag, foreign::ForeignObject};
    let mut s = Session::new();
    let h = line(&mut s, 0);
    block(&mut s, "ORIGINAL", &[h]);
    let target = s.doc().unwrap().block("ORIGINAL").unwrap().entities.iter().next().unwrap().handle;
    let d = s.doc_mut().unwrap();
    let symbol = d.new_handle();
    let dictionary = d.new_handle();
    let record = d.new_handle();
    let tag = |code, value: String| RawTag { code, value };
    let foreign = Arc::make_mut(&mut d.foreign);
    foreign.targets.insert(symbol, ("BLOCK_RECORD".into(), "ORIGINAL".into()));
    foreign.records.entry(("BLOCK_RECORD".into(), "ORIGINAL".into())).or_default().xdict = Some(dictionary);
    foreign.objects.push(ForeignObject {
        handle: dictionary,
        kind: "DICTIONARY".into(),
        tags: vec![
            tag(5, dictionary.hex()),
            tag(330, symbol.hex()),
            tag(100, "AcDbDictionary".into()),
            tag(3, "COPY_PROBE".into()),
            tag(350, record.hex()),
        ],
    });
    foreign.objects.push(ForeignObject {
        handle: record,
        kind: "XRECORD".into(),
        tags: vec![
            tag(5, record.hex()),
            tag(330, dictionary.hex()),
            tag(100, "AcDbXrecord".into()),
            tag(280, "1".into()),
            tag(1, "original".into()),
            tag(1001, "COPY_META".into()),
            tag(1005, target.hex()),
        ],
    });
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    line(&mut s, 1);
    Arc::make_mut(&mut s.doc_mut().unwrap().foreign)
        .objects
        .iter_mut()
        .find(|o| o.handle == record)
        .unwrap()
        .tags
        .iter_mut()
        .find(|t| t.code == 1)
        .unwrap()
        .value = "edited".into();
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    for d in roundtrips(&s) {
        let mut dicts = BTreeSet::new();
        for name in ["ORIGINAL", "COPY"] {
            let dictionary =
                d.foreign.records.get(&("BLOCK_RECORD".into(), name.into())).and_then(|e| e.xdict).expect("copied named-record dictionary");
            assert!(dicts.insert(dictionary), "named records share a dictionary");
            let object = d.foreign.objects.iter().find(|o| o.handle == dictionary).unwrap();
            let owner = cadcraft_engine::doc::foreign::object_owner(object.tags.iter().map(|t| (t.code, t.value.as_str()))).unwrap();
            assert_eq!(
                d.foreign.targets.get(&owner),
                Some(&("BLOCK_RECORD".into(), name.into())),
                "dictionary owner is bound to the wrong block role"
            );
            let entry = object.tags.iter().find(|t| matches!(t.code, 350 | 360)).and_then(|t| Handle::parse_hex(&t.value)).unwrap();
            let record = d.foreign.objects.iter().find(|o| o.handle == entry).unwrap();
            assert!(record.tags.iter().any(|t| t.code == 1 && t.value == if name == "ORIGINAL" { "original" } else { "edited" }));
            let links: BTreeSet<_> = record.tags.iter().filter(|t| t.code == 1005).filter_map(|t| Handle::parse_hex(&t.value)).collect();
            assert!(links.is_subset(&identities(&d, name)), "{name}: record points outside its own copied namespace");
        }
    }
}

fn dynamic_source() -> Session {
    use cadcraft_engine::doc::{RawTag, foreign::ForeignObject};
    let mut s = Session::new();
    let h = line(&mut s, 0);
    block(&mut s, "ORIGINAL", &[h]);
    let controlled = s.doc().unwrap().block("ORIGINAL").unwrap().entities.iter().next().unwrap().handle;
    let d = s.doc_mut().unwrap();
    let symbol = d.new_handle();
    let dictionary = d.new_handle();
    let graph = d.new_handle();
    let parameter = d.new_handle();
    let f = Arc::make_mut(&mut d.foreign);
    f.targets.insert(symbol, ("BLOCK_RECORD".into(), "ORIGINAL".into()));
    f.records.entry(("BLOCK_RECORD".into(), "ORIGINAL".into())).or_default().xdict = Some(dictionary);
    let obj = |handle: Handle, kind: &str, tags: Vec<(i32, String)>| ForeignObject {
        handle,
        kind: kind.into(),
        tags: tags.into_iter().map(|(code, value)| RawTag { code, value }).collect(),
    };
    f.objects.push(obj(
        dictionary,
        "DICTIONARY",
        vec![(5, dictionary.hex()), (330, symbol.hex()), (100, "AcDbDictionary".into()), (3, "ACAD_ENHANCEDBLOCK".into()), (360, graph.hex())],
    ));
    f.objects.push(obj(
        graph,
        "ACAD_EVALUATION_GRAPH",
        vec![(5, graph.hex()), (330, dictionary.hex()), (100, "AcDbEvalGraph".into()), (91, "0".into()), (95, "1".into()), (360, parameter.hex())],
    ));
    f.objects.push(obj(
        parameter,
        "BLOCKVISIBILITYPARAMETER",
        vec![
            (5, parameter.hex()),
            (330, graph.hex()),
            (100, "AcDbEvalExpr".into()),
            (90, "1".into()),
            (100, "AcDbBlockElement".into()),
            (300, "Visibility State".into()),
            (100, "AcDbBlockParameter".into()),
            (280, "1".into()),
            (100, "AcDbBlock1PtParameter".into()),
            (1010, "2".into()),
            (1020, "3".into()),
            (100, "AcDbBlockVisibilityParameter".into()),
            (301, "Visibility1".into()),
            (302, "".into()),
            (93, "1".into()),
            (331, controlled.hex()),
            (92, "2".into()),
            (303, "Open".into()),
            (94, "1".into()),
            (332, controlled.hex()),
            (95, "0".into()),
            (303, "Closed".into()),
            (94, "0".into()),
            (95, "0".into()),
        ],
    ));
    s
}

#[test]
fn save_as_clones_dynamic_record_graphs_and_their_controlled_handles() {
    let mut s = dynamic_source();
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_file_unique(s.doc().unwrap());
    assert_distinct(s.doc().unwrap());
    for name in ["ORIGINAL", "COPY"] {
        let expected = s.doc().unwrap().block(name).unwrap().entities.iter().next().unwrap().handle.hex();
        let descriptor = s.execute("blocks.dynamic", &json!({"name":name})).unwrap();
        assert_eq!(
            descriptor["parameters"][0]["kind"]["controlled"],
            json!([expected]),
            "{name}: dynamic metadata was dropped or still controls the original"
        );
    }
    let old = s.doc().unwrap().foreign.records.get(&("BLOCK_RECORD".into(), "ORIGINAL".into())).unwrap().xdict;
    let new = s.doc().unwrap().foreign.records.get(&("BLOCK_RECORD".into(), "COPY".into())).unwrap().xdict;
    assert_ne!(old, new);
    s.execute("rename", &json!({"table":"block","from":"COPY","to":"RENAMED"})).unwrap();
    let descriptor = s.execute("blocks.dynamic", &json!({"name":"RENAMED"})).unwrap();
    assert_eq!(descriptor["parameters"].as_array().unwrap().len(), 1, "RENAME lost name-keyed block data");
    assert_file_unique(s.doc().unwrap());
}

fn field_references_in(d: &Drawing, name: &str) -> BTreeSet<Handle> {
    let ids = identities(d, name);
    let owners: BTreeMap<_, _> = d
        .foreign
        .objects
        .iter()
        .filter_map(|o| cadcraft_engine::doc::foreign::object_owner(o.tags.iter().map(|t| (t.code, t.value.as_str()))).map(|owner| (o.handle, owner)))
        .collect();
    let mut references = BTreeSet::new();
    for field in d.foreign.objects.iter().filter(|o| o.kind == "FIELD") {
        let mut root = field.handle;
        for _ in 0..64 {
            if let Some(parent) = owners.get(&root) {
                root = *parent;
            } else {
                break;
            }
        }
        if ids.contains(&root) {
            references.extend(field.tags.iter().filter(|t| t.code == 331).filter_map(|t| Handle::parse_hex(&t.value)));
        }
    }
    references
}

#[test]
fn erased_source_roots_do_not_escape_copy_fields_but_external_references_survive() {
    let mut expected = Vec::new();
    for copying in [false, true] {
        let mut s = attribute_source();
        let external = line(&mut s, 9);
        let erased = s.doc().unwrap().block("ORIGINAL").unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
        s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
        s.execute("erase", &json!({"handles":[erased.hex()]})).unwrap();
        let nested = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
        s.doc_mut()
            .unwrap()
            .modify_entity(nested, |e| {
                e.common.xdata.extend([
                    cadcraft_engine::doc::RawTag { code: 1001, value: "EXTERNAL_PROBE".into() },
                    cadcraft_engine::doc::RawTag { code: 1005, value: external.hex() },
                ]);
            })
            .unwrap();
        if copying {
            s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
        } else {
            s.execute("bsave", &json!({})).unwrap();
        }
        s.execute("bclose", &json!({"save":true})).unwrap();
        for (format, d) in roundtrips(&s).into_iter().enumerate() {
            let name = if copying { "COPY" } else { "ORIGINAL" };
            let refs = field_references_in(&d, name);
            assert!(!refs.is_empty(), "fixture must retain the surviving FIELD references");
            if copying {
                assert_eq!(refs, expected[format], "copy FIELD rebound to restored ORIGINAL instead of matching plain BSAVE");
            } else {
                expected.push(refs);
            }
            let nested = d.block(name).unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap();
            assert!(
                nested.common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(external)),
                "legitimate external reference was removed"
            );
        }
    }
}

#[test]
fn erased_source_roots_do_not_escape_copied_dynamic_graphs() {
    let mut expected = Vec::new();
    for copying in [false, true] {
        let mut s = dynamic_source();
        let erased = s.doc().unwrap().block("ORIGINAL").unwrap().entities.iter().next().unwrap().handle;
        s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
        s.execute("erase", &json!({"handles":[erased.hex()]})).unwrap();
        if copying {
            s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
        } else {
            s.execute("bsave", &json!({})).unwrap();
        }
        s.execute("bclose", &json!({"save":true})).unwrap();
        for (format, d) in roundtrips(&s).into_iter().enumerate() {
            let key = ("BLOCK_RECORD".into(), if copying { "COPY".into() } else { "ORIGINAL".into() });
            let dictionary = d.foreign.records.get(&key).unwrap().xdict.unwrap();
            let mut owned = BTreeSet::from([dictionary]);
            for _ in 0..64 {
                let before = owned.len();
                for object in &d.foreign.objects {
                    if cadcraft_engine::doc::foreign::object_owner(object.tags.iter().map(|t| (t.code, t.value.as_str())))
                        .is_some_and(|h| owned.contains(&h))
                    {
                        owned.insert(object.handle);
                    }
                }
                if before == owned.len() {
                    break;
                }
            }
            let refs: BTreeSet<_> = d
                .foreign
                .objects
                .iter()
                .filter(|o| owned.contains(&o.handle))
                .flat_map(|o| &o.tags)
                .filter(|t| matches!(t.code, 331 | 332))
                .filter_map(|t| Handle::parse_hex(&t.value))
                .collect();
            if copying {
                assert_eq!(refs, expected[format], "copy dynamic graph controls a restored original entity");
            } else {
                expected.push(refs);
            }
        }
    }
}

#[test]
fn rename_is_not_allowed_to_split_the_active_editor_namespace() {
    let mut s = Session::new();
    let h = line(&mut s, 0);
    block(&mut s, "ORIGINAL", &[h]);
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    let before = s.state().unwrap().doc.clone();
    let result = s.execute("rename", &json!({"table":"block","from":"ORIGINAL","to":"RENAMED"}));
    assert!(result.is_err(), "RENAME must not create an untracked second name while BEDIT is active");
    assert!(Arc::ptr_eq(&s.state().unwrap().doc, &before));
}

#[test]
fn rename_during_single_insert_refedit_is_refused_without_losing_the_edit() {
    let mut s = Session::new();
    let h = line(&mut s, 0);
    let insert = block(&mut s, "A", &[h]);
    assert_eq!(s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::Insert(_))).count(), 1);
    s.execute("refedit", &json!({"handle":insert})).unwrap();
    let working = s.doc().unwrap().refedit.as_ref().unwrap().items[0].copy;
    s.execute("move", &json!({"handles":[working.hex()],"delta":[2,0]})).unwrap();
    let before = s.state().unwrap().doc.clone();
    let result = s.execute("rename", &json!({"table":"block","from":"A","to":"B"}));
    assert!(result.is_err(), "RENAME escaped the one-insert working-set guard");
    assert!(Arc::ptr_eq(&before, &s.state().unwrap().doc));
    s.execute("refclose", &json!({"save":true})).unwrap();
    assert!(s.doc().unwrap().block("B").is_none());
    let EntityKind::Line(line) = &s.doc().unwrap().block("A").unwrap().entities.iter().next().unwrap().kind else { panic!("line") };
    assert!((line.a.x - 2.0).abs() < 1e-9);
}

#[test]
fn wblock_from_editor_block_redefinition_and_external_rename_keep_unique_identities() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
    let mut s = Session::new();
    let h = line(&mut s, 0);
    block(&mut s, "ORIGINAL", &[h]);
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    let selected: Vec<_> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    for extension in ["dxf", "dwg"] {
        let path = std::env::temp_dir().join(format!("cadcraft-editor-wblock-{}.{}", std::process::id(), extension));
        s.execute("wblock", &json!({"path":path,"handles":selected})).unwrap();
        let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path).unwrap();
        assert_file_unique(&d);
        std::fs::remove_file(path).unwrap();
    }
    s.execute("bclose", &json!({"save":false})).unwrap();
    let replacement = line(&mut s, 3);
    block(&mut s, "ORIGINAL", &[replacement]);
    assert_file_unique(s.doc().unwrap());
    let ids = identities(s.doc().unwrap(), "ORIGINAL");
    s.execute("rename", &json!({"table":"block","from":"ORIGINAL","to":"COPY"})).unwrap();
    assert!(s.doc().unwrap().block("ORIGINAL").is_none());
    assert_eq!(identities(s.doc().unwrap(), "COPY"), ids);
    for d in roundtrips(&s) {
        assert_eq!(d.block("COPY").unwrap().entities.len(), 1);
    }
}

#[test]
fn save_as_clones_raw_associations_without_pointing_at_original_geometry() {
    let mut s = Session::new();
    let line = line(&mut s, 0);
    let dim = s.execute("dimlinear", &json!({"object":line.hex(),"at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    block(&mut s, "ORIGINAL", &[line, Handle::parse_hex(&dim).unwrap()]);
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut own = false;
    let mut inside = false;
    tags.retain(|t| {
        if t.code == 0 {
            own = false;
            inside = t.str() == "DIMASSOC";
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        !(own || inside && t.code == 301)
    });
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    s.open_drawing(d, "raw-block.dxf", None);
    let original = s.doc().unwrap().block("ORIGINAL").unwrap().clone();
    s.execute("bedit", &json!({"name":"ORIGINAL"})).unwrap();
    let old_last = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    s.last_dim = Some(old_last);
    s.execute("bsaveas", &json!({"name":"COPY"})).unwrap();
    let new_last = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    assert_ne!(new_last, old_last);
    assert_eq!(s.last_dim, Some(new_last));
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save":true})).unwrap();
    assert_eq!(s.doc().unwrap().block("ORIGINAL").unwrap(), &original);
    for d in roundtrips(&s) {
        assert_distinct(&d);
        assert_eq!(d.foreign.objects.iter().filter(|o| o.kind == "DIMASSOC").count(), 2);
        for name in ["ORIGINAL", "COPY"] {
            let block = d.block(name).unwrap();
            let dim = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
            let line = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
            let raw = d
                .foreign
                .objects
                .iter()
                .find(|o| {
                    o.kind == "DIMASSOC"
                        && cadcraft_engine::doc::foreign::dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str())))
                            .is_some_and(|(_, h)| h == dim)
                })
                .unwrap();
            assert!(
                raw.tags.iter().filter(|t| t.code == 331).all(|t| Handle::parse_hex(&t.value) == Some(line)),
                "{name}: raw association still names original geometry"
            );
        }
    }
}

//! Explicit dimension association commands replace raw associations; unrelated edits do not.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use cadcraft_engine::doc::{Handle, RawTag};
use serde_json::json;

fn session() -> (Session, String, String, Handle) {
    let mut s = Session::new();
    let line = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"object":line, "at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    let source = cadcraft_io::write_dxf(s.doc().unwrap());
    let mut tags = cadcraft_dxf::parse(source.as_bytes()).unwrap();
    let mut in_assoc = false;
    for tag in &mut tags {
        if tag.code == 0 {
            in_assoc = tag.str() == "DIMASSOC";
        } else if in_assoc && tag.code == 72 {
            *tag = Tag::i(72, 8);
        }
    }
    let original = objects(&source);
    let raw =
        original.iter().find(|(k, _)| k == "DIMASSOC").unwrap().1.iter().find(|t| t.code == 5).map(|t| Handle::parse_hex(&t.str()).unwrap()).unwrap();
    let handle = tags.iter().position(|t| t.code == 5 && t.str() == line).unwrap();
    assert_eq!(
        tags[..handle].iter().rev().find(|t| t.code == 0).map(Tag::str).as_deref(),
        Some("LINE"),
        "fixture handle must identify the model-space LINE, not a line in the dimension block"
    );
    tags.splice(handle + 1..handle + 1, [Tag::s(102, "{ACAD_REACTORS"), Tag::s(330, raw.hex()), Tag::s(102, "}")]);
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(
        d.foreign.entities.get(&Handle::parse_hex(&line).unwrap()).is_some_and(|x| x.reactors.contains(&raw)),
        "fixture reactor must be read from the source DXF: line={line}, raw={raw:?}, extras={:?}",
        d.foreign.entities
    );
    let mut s = Session::new();
    s.open_drawing(d, "raw.dxf", None);
    (s, dim, line, raw)
}

fn objects(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let secs = sections(&tags);
    records(&secs.iter().find(|s| s.name == "OBJECTS").unwrap().tags)
}

fn saved(s: &Session) -> Vec<String> {
    let dxf = cadcraft_io::write_dxf(s.doc().unwrap());
    let back = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let mut out = vec![dxf, cadcraft_io::write_dxf(&back)];
    let dwg = cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    out.push(cadcraft_io::write_dxf(&back));
    out
}

fn assert_retired(s: &Session, raw: Handle) {
    let foreign = &s.doc().unwrap().foreign;
    assert!(foreign.objects.iter().all(|o| o.handle != raw), "raw DIMASSOC was not retired");
    assert!(foreign.entities.values().chain(foreign.records.values()).all(|x| !x.reactors.contains(&raw)), "retired reactor edge remains");
}

#[test]
fn copying_a_raw_dimension_without_its_geometry_drops_the_cloned_association() {
    let (mut s, dim, _, _) = session();
    let copy = s.execute("copy", &json!({"handles":[dim], "delta":[20,0]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let h = Handle::parse_hex(&copy).unwrap();
    assert!(
        s.doc().unwrap().foreign.entities.get(&h).is_none_or(|x| x.xdict.is_none() && x.reactors.is_empty()),
        "copied dimension retained a dictionary or reactor outside its cloned graph"
    );
    for text in saved(&s) {
        let objects = objects(&text);
        let associations: Vec<_> = objects.iter().filter(|(k, _)| k == "DIMASSOC").collect();
        assert_eq!(associations.len(), 1, "only the original dimension keeps its association when geometry was not copied");
        assert!(!associations[0].1.iter().any(|t| t.code == 330 && t.str() == copy), "copy still owns a raw association to original geometry");
    }
}

#[test]
fn copying_raw_dimension_with_geometry_remaps_the_whole_copy_set() {
    let (mut s, dim, line, _) = session();
    let result = s.execute("copy", &json!({"handles":[line, dim], "delta":[20,0]})).unwrap();
    let line = result["handles"][0].as_str().unwrap();
    let dim = result["handles"][1].as_str().unwrap();
    for text in saved(&s) {
        let objects = objects(&text);
        let assoc = &objects
            .iter()
            .find(|(k, t)| k == "DIMASSOC" && t.iter().any(|t| t.code == 330 && t.str() == dim))
            .expect("copied dimension keeps the association to its copied geometry")
            .1;
        assert_eq!(
            assoc.iter().filter(|t| t.code == 331).map(Tag::str).collect::<Vec<_>>(),
            vec![line.to_string(); 2],
            "all raw references must follow the copied LINE"
        );
    }
}

#[test]
fn dimdisassociate_retires_raw_object_dictionary_and_reactors() {
    let (mut s, dim, _, raw) = session();
    let result = s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    assert_eq!(result["disassociated"], 1);
    let dim = Handle::parse_hex(&dim).unwrap();
    assert!(s.doc().unwrap().foreign.entities.get(&dim).is_none_or(|x| x.xdict.is_none()), "empty association dictionary remains attached");
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(k, _)| k != "DIMASSOC"));
    }
}

#[test]
fn dimdisassociate_matches_normalized_raw_subclass_names() {
    let (mut s, dim, _, raw) = session();
    let f = std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().foreign);
    let object = f.objects.iter_mut().find(|o| o.handle == raw).unwrap();
    object.tags.iter_mut().find(|t| t.code == 100).unwrap().value = " AcDbDimAssoc ".into();
    s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
}

#[test]
fn dimreassociate_replaces_raw_association_with_the_models_links() {
    let (mut s, dim, _, raw) = session();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    for text in saved(&s) {
        let objects = objects(&text);
        let assocs: Vec<_> = objects.iter().filter(|(k, _)| k == "DIMASSOC").collect();
        assert_eq!(assocs.len(), 1);
        let snaps: Vec<_> = assocs[0].1.iter().filter(|t| t.code == 72).map(Tag::i64).collect();
        assert_eq!(snaps, [13, 1], "raw unsupported snaps overrode the new model links");
    }
}

#[test]
fn unrelated_dimension_edits_keep_the_raw_association() {
    let (mut s, dim, _, raw) = session();
    s.execute("dimoverride", &json!({"handles":[dim], "text":"label"})).unwrap();
    assert!(s.doc().unwrap().foreign.objects.iter().any(|o| o.handle == raw));
    for text in saved(&s) {
        let objects = objects(&text);
        let assoc = &objects.iter().find(|(k, _)| k == "DIMASSOC").unwrap().1;
        assert_eq!(assoc.iter().filter(|t| t.code == 72).map(Tag::i64).collect::<Vec<_>>(), [8, 8]);
    }
}

#[test]
fn one_command_retires_all_selected_raw_associations() {
    let (mut s, dim, line, _) = session();
    let copy = s.execute("copy", &json!({"handles":[line, dim],"delta":[20,0]})).unwrap()["handles"][1].as_str().unwrap().to_string();
    let result = s.execute("dimdisassociate", &json!({"handles":[dim,copy]})).unwrap();
    assert_eq!(result["disassociated"], 2, "both raw-only associations must be counted once");
    assert!(s.doc().unwrap().foreign.objects.iter().all(|o| o.kind != "DIMASSOC"), "one batch left an association behind");
    for text in saved(&s) {
        assert!(objects(&text).iter().all(|(kind, _)| kind != "DIMASSOC"), "retired batch reappeared after save/reopen");
    }
}

#[test]
fn retiring_associations_does_not_rewrite_unrelated_object_storage() {
    let (mut s, dim, _, _) = session();
    let d = s.doc_mut().unwrap();
    let dimension = Handle::parse_hex(&dim).unwrap();
    let dictionary = d.foreign.entities.get(&dimension).unwrap().xdict.unwrap();
    let keep = d.new_handle();
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    f.objects
        .iter_mut()
        .find(|o| o.handle == dictionary)
        .unwrap()
        .tags
        .extend([RawTag { code: 3, value: "KEEP".into() }, RawTag { code: 350, value: keep.hex() }]);
    let mut tags = Vec::with_capacity(2048);
    tags.extend([
        RawTag { code: 5, value: keep.hex() },
        RawTag { code: 330, value: dictionary.hex() },
        RawTag { code: 100, value: "AcDbXrecord".into() },
        RawTag { code: 1, value: "unrelated".into() },
    ]);
    f.objects.push(cadcraft_engine::doc::foreign::ForeignObject { handle: keep, kind: "XRECORD".into(), tags });
    assert_eq!(std::sync::Arc::strong_count(&d.foreign), 1, "capacity check must not be confounded by legitimate undo copy-on-write");
    d.retire_raw_dimassocs(&std::collections::BTreeSet::from([dimension]));
    let kept = d.foreign.objects.iter().find(|o| o.handle == keep).unwrap();
    assert!(kept.tags.capacity() >= 2048, "unrelated object tags were reallocated by association retirement");
}

#[test]
fn retiring_raw_dimassoc_keeps_unrelated_dictionary_entries() {
    let (mut s, dim, _, raw) = session();
    let d = s.doc_mut().unwrap();
    let dim_handle = Handle::parse_hex(&dim).unwrap();
    let dictionary = d.foreign.entities.get(&dim_handle).unwrap().xdict.unwrap();
    let keep = d.new_handle();
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    let dict = f.objects.iter_mut().find(|o| o.handle == dictionary).unwrap();
    dict.tags.extend([RawTag { code: 3, value: "KEEP".into() }, RawTag { code: 350, value: keep.hex() }]);
    f.objects.push(cadcraft_engine::doc::foreign::ForeignObject {
        handle: keep,
        kind: "XRECORD".into(),
        tags: vec![
            RawTag { code: 5, value: keep.hex() },
            RawTag { code: 330, value: dictionary.hex() },
            RawTag { code: 100, value: "AcDbXrecord".into() },
            RawTag { code: 280, value: "1".into() },
            RawTag { code: 1, value: "unrelated".into() },
        ],
    });
    s.execute("dimdisassociate", &json!({"handles":[dim]})).unwrap();
    assert_retired(&s, raw);
    assert_eq!(s.doc().unwrap().foreign.entities.get(&dim_handle).unwrap().xdict, Some(dictionary));
    for text in saved(&s) {
        let objects = objects(&text);
        assert!(objects.iter().all(|(k, _)| k != "DIMASSOC"));
        assert!(objects.iter().any(|(k, t)| k == "XRECORD" && t.iter().any(|t| t.code == 1 && t.str() == "unrelated")));
    }
}

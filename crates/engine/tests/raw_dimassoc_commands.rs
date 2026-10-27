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
    let mut d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    let raw = d.foreign.objects.iter().find(|o| o.kind == "DIMASSOC").unwrap().handle;
    // A geometry reactor must not be left pointing to the retired raw association.
    std::sync::Arc::make_mut(&mut d.foreign).entities.entry(Handle::parse_hex(&line).unwrap()).or_default().reactors.push(raw);
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

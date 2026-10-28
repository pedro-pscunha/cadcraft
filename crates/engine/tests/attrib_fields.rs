//! Attribute foreign data follows the attribute through block-management commands and copies.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use serde_json::json;

fn session() -> Session {
    let source = include_str!("../../io/tests/data/attrib_fields.dxf");
    let preceding = "1\nplain\n100\nAcDbAttribute\n2\nLENGTH\n";
    assert_eq!(source.matches(preceding).count(), 1, "fixture must rename exactly the preceding plain attribute, not its ATTDEF or live peer");
    let source = source.replace(preceding, "1\nplain\n100\nAcDbAttribute\n2\nPLAIN\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let mut s = Session::new();
    s.open_drawing(d, "fields.dxf", None);
    s
}

fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

fn assert_live_fields(s: &Session) {
    let text = cadcraft_io::write_dxf(s.doc().unwrap());
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let secs = sections(&tags);
    let ents = records(&secs.iter().find(|s| s.name == "ENTITIES").unwrap().tags);
    let attr = &ents.iter().find(|(k, t)| k == "ATTRIB" && val(t, 1) == "live").unwrap().1;
    assert!(attr.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "live attribute lost its dictionary");
    assert_eq!(val(attr, 1005), val(attr, 5), "xdata still points at this attribute");
    let objs = records(&secs.iter().find(|s| s.name == "OBJECTS").unwrap().tags);
    assert_eq!(objs.iter().filter(|(k, _)| k == "FIELD").count(), 2);
    let dict = &objs.iter().find(|(k, t)| k == "DICTIONARY" && val(t, 5) == val(attr, 360)).unwrap().1;
    assert_eq!(val(dict, 330), val(attr, 5));
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_eq!(back.foreign.objects.iter().filter(|o| o.kind == "FIELD").count(), 2);
}

fn assert_copy_graphs(text: &str, copies: usize) {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let secs = sections(&tags);
    let ents = records(&secs.iter().find(|s| s.name == "ENTITIES").unwrap().tags);
    let attrs: Vec<_> = ents.iter().filter(|(k, t)| k == "ATTRIB" && val(t, 1) == "live").map(|(_, t)| t).collect();
    assert_eq!(attrs.len(), copies);
    let handles: std::collections::HashSet<_> = attrs.iter().map(|a| val(a, 5)).collect();
    assert_eq!(handles.len(), copies, "each copied attribute needs its own identity");
    let dictionaries: std::collections::HashSet<_> = attrs.iter().map(|a| val(a, 360)).collect();
    assert_eq!(dictionaries.len(), copies, "owners must never share an extension dictionary");
    let objs = records(&secs.iter().find(|s| s.name == "OBJECTS").unwrap().tags);
    assert_eq!(objs.iter().filter(|(k, _)| k == "FIELD").count(), copies * 2);
    let object = |h: String| &objs.iter().find(|(_, t)| val(t, 5) == h).unwrap().1;
    for a in attrs {
        assert_eq!(val(a, 1005), val(a, 5), "copied xdata points to its own attribute");
        let dict = object(val(a, 360));
        assert_eq!(val(dict, 330), val(a, 5));
        let field_dict = object(val(dict, 360));
        assert_eq!(val(field_dict, 330), val(dict, 5));
        let parent = object(val(field_dict, 360));
        assert_eq!(val(parent, 330), val(field_dict, 5));
        let child = object(val(parent, 360));
        assert_eq!(val(child, 330), val(parent, 5));
        assert_eq!(val(child, 331), "30", "external geometry is not cloned with the attribute");
        assert!(a.iter().any(|t| t.code == 330 && t.str() == val(child, 5)), "reactor follows the cloned FIELD");
    }
}

#[test]
fn copied_insert_owns_a_distinct_attribute_field_graph() {
    let mut s = session();
    s.execute("copy", &json!({"handles":["31"], "delta":[20,0]})).unwrap();
    assert_copy_graphs(&cadcraft_io::write_dxf(s.doc().unwrap()), 2);
}

#[test]
fn copied_attribute_fields_survive_erasing_original_and_reopening() {
    let mut s = session();
    s.execute("copy", &json!({"handles":["31"], "delta":[20,0]})).unwrap();
    s.execute("erase", &json!({"handles":["31"]})).unwrap();
    let saved = cadcraft_io::write_dxf(s.doc().unwrap());
    let tags = cadcraft_dxf::parse(saved.as_bytes()).unwrap();
    assert!(!tags.iter().any(|t| t.code == 5 && t.str() == "33"), "the copy must not reuse the original attribute's handle");
    assert_copy_graphs(&saved, 1);
    let d = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_copy_graphs(&cadcraft_io::write_dxf(&d), 1);
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let d = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    assert_copy_graphs(&cadcraft_io::write_dxf(&d), 1);
}

#[test]
fn properties_do_not_expose_decimal_attribute_identity() {
    let mut s = session();
    let properties = s.execute("properties", &json!({"handles":["31"]})).unwrap();
    assert_eq!(properties["count"], 1, "fixture INSERT must be selected");
    let attributes = properties["objects"][0]["geometry"]["attribs"].as_array().expect("INSERT attribute properties");
    assert_eq!(attributes.len(), 2, "fixture attributes must be serialized");
    assert!(attributes.iter().any(|a| a["text"]["value"] == "live"), "live attribute must be in the response");
    assert!(
        attributes.iter().all(|a| a.get("handle").is_none()),
        "internal attribute identities must not appear as decimal properties: {properties}"
    );
}

#[test]
fn attsync_keeps_live_attribute_fields() {
    let mut s = session();
    s.execute("attsync", &json!({"name":"LABEL"})).unwrap();
    assert_live_fields(&s);
}

#[test]
fn battman_rename_keeps_live_attribute_fields() {
    let mut s = session();
    s.execute("battman", &json!({"name":"LABEL", "action":"edit", "tag":"LENGTH", "newTag":"DISTANCE"})).unwrap();
    assert_live_fields(&s);
}

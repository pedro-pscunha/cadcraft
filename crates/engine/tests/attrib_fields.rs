//! Attribute foreign data follows the attribute through block-management commands and copies.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use serde_json::json;

fn session() -> Session {
    let source = include_str!("../../io/tests/data/attrib_fields.dxf")
        .replace("1\nplain\n100\nAcDbAttribute\n2\nLENGTH\n", "1\nplain\n100\nAcDbAttribute\n2\nPLAIN\n");
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

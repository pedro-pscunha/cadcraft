use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

fn attribute_session() -> Session {
    let d = cadcraft_io::read_dxf(include_bytes!("../../io/tests/data/attrib_fields.dxf")).unwrap();
    let mut s = Session::new();
    s.open_drawing(d, "attributes.dxf", None);
    s
}

#[test]
fn blockreplace_assigns_new_attribute_identities_before_the_command_returns() {
    let mut s = attribute_session();
    let d = s.doc_mut().unwrap();
    let mut replacement = (**d.block("LABEL").unwrap()).clone();
    replacement.name = "REPLACEMENT".into();
    let definition = replacement.entities.iter().find_map(|e| if let EntityKind::AttDef(a) = &e.kind { Some(a.clone()) } else { None }).unwrap();
    let h = d.new_handle();
    replacement.entities.push(cadcraft_engine::doc::Entity::new(h, EntityKind::AttDef(cadcraft_engine::doc::Attrib::new("EXTRA", definition.text))));
    d.blocks.insert("REPLACEMENT".into(), std::sync::Arc::new(replacement));
    s.execute("blockreplace", &json!({"block":"LABEL","with":"REPLACEMENT"})).unwrap();
    let EntityKind::Insert(insert) = &s.doc().unwrap().entity(Handle(0x31)).unwrap().kind else { panic!("insert") };
    let extra = insert.attribs.iter().find(|a| a.tag == "EXTRA").unwrap();
    assert!(extra.handle.is_some(), "writer backstops must not own an attribute's persistent identity");
    let h = extra.handle;
    s.execute("line", &json!({"points":[[50,0],[60,0]]})).unwrap();
    let saved = cadcraft_io::read_dxf(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let EntityKind::Insert(insert) = &saved.entity(Handle(0x31)).unwrap().kind else { panic!("insert") };
    assert_eq!(insert.attribs.iter().find(|a| a.tag == "EXTRA").unwrap().handle, h);
}

#[test]
fn repeated_selection_handles_do_not_duplicate_explode_or_repeat_move() {
    let mut s = Session::new();
    let h = s.execute("rectang", &json!({"p1":[0,0],"p2":[10,10]})).unwrap()["handle"].as_str().unwrap().to_string();
    let result = s.execute("explode", &json!({"handles":[h,h]})).unwrap();
    let pieces = result["handles"].as_array().unwrap();
    assert_eq!(pieces.len(), 4, "selection is a set, not repeated work: {result}");
    let first = pieces[0].as_str().unwrap();
    let h = Handle::parse_hex(first).unwrap();
    let EntityKind::Line(before) = s.doc().unwrap().entity(h).unwrap().kind.clone() else { panic!("line") };
    s.execute("move", &json!({"handles":[first,h.0],"delta":[2,0]})).unwrap();
    let EntityKind::Line(after) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("line") };
    assert_eq!(after.a.x, before.a.x + 2.0, "hex/numeric aliases must be selected once");
}

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, Line, RawCompound, RawRecord, RawTag};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

fn raw_entity() -> Entity {
    let parent = RawRecord::new("INSERT".into(), vec![RawTag { code: 5, value: "200".into() }, RawTag { code: 330, value: "0".into() }]).unwrap();
    Entity::from_raw(RawCompound::new(parent, vec![], Common::default(), "The source has no exact geometry adapter.".into()).unwrap())
}

fn session() -> Session {
    let mut drawing = Drawing::new_metric();
    drawing.model.push(Entity::new(Handle(0x90), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(4.0, 0.0, 0.0) })));
    drawing.model.push(raw_entity());
    let mut session = Session::empty();
    session.open_drawing(drawing, "raw.dxf", None);
    session
}

#[test]
fn mixed_copyclip_refuses_before_it_changes_the_clipboard_or_document() {
    let mut session = session();
    let before = session.doc().unwrap().clone();
    let error = session.execute("copyclip", &json!({"handles": [0x90, 0x200]})).unwrap_err().to_string();
    assert!(error.contains("200") && error.contains("INSERT") && error.contains("read-only"), "{error}");
    assert!(session.clipboard.is_empty());
    assert_eq!(session.doc().unwrap(), &before);
}

#[test]
fn pasteblock_refuses_an_opaque_clipboard_unit_before_it_changes_the_document() {
    let mut session = session();
    session.clipboard = vec![raw_entity()];
    let before = session.doc().unwrap().clone();
    let error = session.execute("pasteblock", &json!({"at": [1.0, 2.0]})).unwrap_err().to_string();
    assert!(error.contains("200") && error.contains("read-only"), "{error}");
    assert_eq!(session.doc().unwrap(), &before);
}

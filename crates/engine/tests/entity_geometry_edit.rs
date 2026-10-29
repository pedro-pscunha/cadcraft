use cadcraft_doc::{Common, EntityKind, Space, Unknown};
use cadcraft_engine::Session;
use serde_json::json;

#[test]
fn a_failed_typed_transform_restores_the_current_input_step() {
    let mut session = Session::new();
    session.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap();
    session
        .doc_mut()
        .unwrap()
        .add(&Space::Model, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "SHAPE".into(), tags: Vec::new() }))
        .unwrap();
    let before = session.doc().unwrap().model.iter().map(|entity| entity.as_ref().clone()).collect::<Vec<_>>();
    for input in ["move", "all", "", "0,0", "2,3"] {
        session.cmdline(input).unwrap();
    }
    assert!(session.log.iter().any(|line| line.contains("supported geometry transform")));
    assert_eq!(session.doc().unwrap().model.iter().map(|entity| entity.as_ref().clone()).collect::<Vec<_>>(), before);
}

#[test]
fn a_failed_multi_entity_transform_restores_the_document_and_selection() {
    let mut session = Session::new();
    let line = session.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let unknown = session
        .doc_mut()
        .unwrap()
        .add(&Space::Model, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "SHAPE".into(), tags: Vec::new() }))
        .unwrap();
    let line_handle = cadcraft_doc::Handle::parse_hex(&line).unwrap();
    session.set_selection(vec![line_handle]);
    let before = session.doc().unwrap().model.iter().map(|entity| entity.as_ref().clone()).collect::<Vec<_>>();
    let seed = session.doc().unwrap().handseed;
    let result = session.execute("move", &json!({"handles": [line, unknown.hex()], "delta": [2, 3]}));
    assert!(result.is_err());
    assert_eq!(session.doc().unwrap().model.iter().map(|entity| entity.as_ref().clone()).collect::<Vec<_>>(), before);
    assert_eq!(session.doc().unwrap().handseed, seed);
    assert_eq!(session.selection(), &[line_handle]);
}

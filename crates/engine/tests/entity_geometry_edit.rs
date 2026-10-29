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
fn unmigrated_grip_operations_refuse_canonical_geometry_atomically() {
    let mut accepted = Vec::new();
    for operation in ["single", "multiple", "option"] {
        let mut session = Session::new();
        let line = session.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        let handle = cadcraft_doc::Handle::parse_hex(&line).unwrap();
        session.doc_mut().unwrap().modify_entity(handle, |entity| entity.geometry_revision = cadcraft_doc::GeometryRevision::CanonicalV2).unwrap();
        let before = session.doc().unwrap().model.clone();
        let result = match operation {
            "single" => session.execute("grip.stretch", &json!({"handle": line, "index": 0, "to": [2, 3]})),
            "multiple" => session.execute("grip.stretch", &json!({"grips": [[line, 0]], "delta": [2, 3]})),
            _ => session.execute("grip.option", &json!({"handle": line, "index": 0, "option": "lengthen", "value": 2})),
        };
        if result.is_ok() {
            accepted.push(operation);
        } else {
            assert_eq!(session.doc().unwrap().model, before, "{operation}");
        }
    }
    assert!(accepted.is_empty(), "canonical edits entered legacy grip code: {accepted:?}");
}

#[test]
fn nonlinear_curve_edits_refuse_canonical_targets_and_cutting_edges() {
    let mut accepted = Vec::new();
    for (command, canonical_target) in [("trim", true), ("trim", false), ("extend", true), ("extend", false), ("offset", true)] {
        let mut session = Session::new();
        let end = if command == "extend" { 2 } else { 10 };
        let target = session.execute("line", &json!({"points": [[0, 0], [end, 0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        let edge = session.execute("line", &json!({"points": [[5, -2], [5, 2]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        let canonical = if canonical_target { &target } else { &edge };
        session
            .doc_mut()
            .unwrap()
            .modify_entity(cadcraft_doc::Handle::parse_hex(canonical).unwrap(), |entity| {
                entity.geometry_revision = cadcraft_doc::GeometryRevision::CanonicalV2;
            })
            .unwrap();
        let before = session.doc().unwrap().model.clone();
        let params = if command == "offset" {
            json!({"handle": target, "distance": 1, "side": [1, 1], "erase": true})
        } else {
            let pick = if command == "extend" { [2, 0] } else { [1, 0] };
            json!({"handle": target, "pick": pick, "edges": [edge]})
        };
        let result = session.execute(command, &params);
        match result {
            Ok(_) => accepted.push((command, canonical_target)),
            Err(error) => {
                assert!(error.to_string().contains("canonical"), "{command}, target={canonical_target}");
                assert_eq!(session.doc().unwrap().model, before, "{command}, target={canonical_target}");
            }
        }
    }
    assert!(accepted.is_empty(), "canonical inputs entered legacy curve edits: {accepted:?}");
}

#[test]
fn a_mixed_crossing_edit_refuses_before_it_changes_a_legacy_target() {
    for command in ["trim", "extend"] {
        let mut session = Session::new();
        let end = if command == "extend" { 2 } else { 10 };
        session.execute("line", &json!({"points": [[0, 0], [end, 0]]})).unwrap();
        let canonical = session.execute("line", &json!({"points": [[0, 1], [end, 1]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        let edge = session.execute("line", &json!({"points": [[5, -2], [5, 3]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        session
            .doc_mut()
            .unwrap()
            .modify_entity(cadcraft_doc::Handle::parse_hex(&canonical).unwrap(), |entity| {
                entity.geometry_revision = cadcraft_doc::GeometryRevision::CanonicalV2;
            })
            .unwrap();
        let before = session.doc().unwrap().model.clone();
        let result = session.execute(command, &json!({"crossing": [[2, -0.2], [1.8, 1.2]], "edges": [edge]}));
        let error = result.unwrap_err();
        assert!(error.to_string().contains("canonical"), "{command}: {error}");
        assert_eq!(session.doc().unwrap().model, before, "{command}");
    }
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

use cadcraft_doc::{Drawing, GeometryRevision, Handle};
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn create(session: &mut Session, command: &str, params: Value) -> Handle {
    session.execute(command, &params).unwrap();
    session.doc().unwrap().model.iter().last().unwrap().handle
}

fn install_canonical_fixture(session: &mut Session, handle: Handle) {
    let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    session.doc_mut().unwrap().model.push(entity);
}

fn check_refusals(drawing: &Drawing, command: &str, params: Value, inputs: &[Handle]) {
    let mut control = Session::new();
    control.open_drawing(drawing.clone(), "legacy control", None);
    control.execute(command, &params).unwrap_or_else(|error| panic!("invalid control {command} {params}: {error}"));
    let mut accepted = Vec::new();
    for &input in inputs {
        let mut session = Session::new();
        session.open_drawing(drawing.clone(), "canonical input", None);
        install_canonical_fixture(&mut session, input);
        let before = session.doc().unwrap().model.clone();
        let seed = session.doc().unwrap().handseed;
        match session.execute(command, &params) {
            Ok(_) => accepted.push(input),
            Err(error) => {
                assert!(error.to_string().contains("canonical"), "{command} {params}: {error}");
                assert_eq!(session.doc().unwrap().model, before, "{command}: {input:?}");
                assert_eq!(session.doc().unwrap().handseed, seed, "{command}: {input:?}");
            }
        }
    }
    assert!(accepted.is_empty(), "{command} {params} accepted canonical inputs: {accepted:?}");
}

#[test]
fn fillet_and_chamfer_check_each_input_and_the_polyline_option() {
    for command in ["fillet", "chamfer"] {
        let mut session = Session::new();
        let a = create(&mut session, "line", json!({"points": [[0, 0], [10, 0]]}));
        let b = create(&mut session, "line", json!({"points": [[0, 0], [0, 10]]}));
        let mut params = json!({"h1": a.hex(), "h2": b.hex(), "p1": [8, 0], "p2": [0, 8]});
        if command == "fillet" {
            params["radius"] = json!(1);
        } else {
            params["d1"] = json!(1);
            params["d2"] = json!(1);
        }
        check_refusals(session.doc().unwrap(), command, params, &[a, b]);

        let mut session = Session::new();
        let a = create(&mut session, "pline", json!({"points": [[0, 0], [10, 0], [10, 10]]}));
        let params = if command == "fillet" {
            json!({"handle": a.hex(), "polyline": true, "radius": 1})
        } else {
            json!({"handle": a.hex(), "polyline": true, "d1": 1, "d2": 1})
        };
        check_refusals(session.doc().unwrap(), command, params, &[a]);
    }
}

#[test]
fn break_and_join_check_all_consumed_geometry() {
    for command in ["break", "breakatpoint", "join", "pedit"] {
        let mut session = Session::new();
        let a = create(&mut session, "line", json!({"points": [[0, 0], [5, 0]]}));
        let b = create(&mut session, "line", json!({"points": [[5, 0], [10, 0]]}));
        let (params, inputs) = match command {
            "break" => (json!({"handle": a.hex(), "p1": [2, 0], "p2": [3, 0]}), vec![a]),
            "breakatpoint" => (json!({"handle": a.hex(), "at": [2, 0]}), vec![a]),
            "join" => (json!({"handles": [a.hex(), b.hex()]}), vec![a, b]),
            _ => (json!({"handle": a.hex(), "option": "join", "handles2": [b.hex()], "convert": true}), vec![a, b]),
        };
        check_refusals(session.doc().unwrap(), command, params, &inputs);
    }
}

#[test]
fn single_curve_edits_refuse_legacy_dispatch_for_canonical_inputs() {
    let cases = [
        ("pline", json!({"points": [[0, 0], [10, 0], [10, 10]]}), "pedit", json!({"option": "reverse"})),
        ("pline", json!({"points": [[0, 0], [10, 0], [10, 10]]}), "pedit", json!({"option": "vertex", "action": "move", "index": 1, "to": [12, 0]})),
        ("arc", json!({"center": [0, 0], "radius": 5, "start": 0, "end": 90}), "join", json!({"close": true})),
        ("spline", json!({"fit": [[0, 0], [2, 2], [4, 0], [6, 2]]}), "splinedit", json!({"option": "reverse"})),
        ("spline", json!({"fit": [[0, 0], [2, 2], [4, 0], [6, 2]]}), "splinedit", json!({"option": "fitmove", "index": 1, "to": [2, 3]})),
        ("line", json!({"points": [[0, 0], [10, 0]]}), "lengthen", json!({"pick": [10, 0], "delta": 2})),
        ("line", json!({"points": [[0, 0], [10, 0]]}), "reverse", json!({})),
        ("line", json!({"points": [[0, 0], [10, 0]]}), "flatten", json!({})),
    ];
    for (create_command, create_params, command, mut params) in cases {
        let mut session = Session::new();
        let a = create(&mut session, create_command, create_params);
        params["handle"] = json!(a.hex());
        check_refusals(session.doc().unwrap(), command, params, &[a]);
    }
}

#[test]
fn divide_and_measure_check_the_curve_before_creating_markers() {
    for command in ["divide", "measure"] {
        let mut session = Session::new();
        let source = create(&mut session, "line", json!({"points": [[0, 0], [10, 0]]}));
        let params = if command == "divide" { json!({"handle": source.hex(), "segments": 4}) } else { json!({"handle": source.hex(), "length": 2}) };
        check_refusals(session.doc().unwrap(), command, params, &[source]);
    }
}

#[test]
fn tangent_circles_check_both_sources_before_curve_extraction() {
    let mut session = Session::new();
    let a = create(&mut session, "line", json!({"points": [[0, 0], [10, 0]]}));
    let b = create(&mut session, "line", json!({"points": [[0, 0], [0, 10]]}));
    check_refusals(session.doc().unwrap(), "circle.ttr", json!({"h1": a.hex(), "h2": b.hex(), "p1": [8, 0], "p2": [0, 8], "radius": 1}), &[a, b]);
}

#[test]
fn blend_checks_both_sources_before_projected_curve_fitting() {
    let mut session = Session::new();
    let a = create(&mut session, "line", json!({"points": [[0, 0], [5, 0]]}));
    let b = create(&mut session, "line", json!({"points": [[8, 2], [12, 2]]}));
    check_refusals(session.doc().unwrap(), "blend", json!({"h1": a.hex(), "h2": b.hex(), "p1": [5, 0], "p2": [8, 2]}), &[a, b]);
}

#[test]
fn a_path_array_checks_its_path_before_creating_copies() {
    let mut session = Session::new();
    let object = create(&mut session, "circle", json!({"center": [0, 2], "radius": 0.5}));
    let path = create(&mut session, "line", json!({"points": [[0, 0], [10, 0]]}));
    check_refusals(session.doc().unwrap(), "arraypath", json!({"handles": [object.hex()], "path": path.hex(), "count": 3}), &[path]);
}

#[test]
fn hatch_and_boundary_trace_only_supported_source_revisions() {
    for (command, by_point) in [("hatch", false), ("gradient", false), ("region", false), ("hatch", true), ("boundary", true)] {
        let mut session = Session::new();
        let source = create(&mut session, "rectang", json!({"p1": [0, 0], "p2": [10, 10]}));
        let params = if by_point { json!({"points": [[5, 5]]}) } else { json!({"handle": source.hex()}) };
        check_refusals(session.doc().unwrap(), command, params, &[source]);
    }
}

#[test]
fn typed_curve_tools_refuse_canonical_inputs_without_a_geometry_change() {
    let cases = [
        ("pline", json!({"points": [[0, 0], [10, 0], [10, 10]]}), "PEDIT 5,0 R\n\n"),
        ("spline", json!({"fit": [[0, 0], [2, 2], [4, 0], [6, 2]]}), "SPLINEDIT 0,0 R\n\n"),
        ("line", json!({"points": [[0, 0], [10, 0]]}), "LENGTHEN DE 2 9,0\n\n"),
        ("line", json!({"points": [[0, 0], [10, 0]]}), "BREAK 2,0 4,0\n\n"),
    ];
    for (command, params, script) in cases {
        let mut session = Session::new();
        let input = create(&mut session, command, params);
        install_canonical_fixture(&mut session, input);
        session.set_selection(Vec::new());
        let before = session.doc().unwrap().model.clone();
        let seed = session.doc().unwrap().handseed;
        let result = session.script(script);
        let reported =
            result.err().is_some_and(|error| error.to_string().contains("canonical")) || session.log.iter().any(|line| line.contains("canonical"));
        assert!(reported, "{script}: {:?}", session.log);
        assert_eq!(session.doc().unwrap().model, before, "{script}");
        assert_eq!(session.doc().unwrap().handseed, seed, "{script}");
    }
}

#[test]
fn spline_join_checks_its_other_input_before_consuming_either_entity() {
    let mut session = Session::new();
    let a = create(&mut session, "spline", json!({"fit": [[0, 0], [1, 0], [2, 0], [3, 0]]}));
    let b = create(&mut session, "line", json!({"points": [[3, 0], [5, 0]]}));
    check_refusals(session.doc().unwrap(), "splinedit", json!({"handle": a.hex(), "option": "join", "handles2": [b.hex()]}), &[a, b]);
}

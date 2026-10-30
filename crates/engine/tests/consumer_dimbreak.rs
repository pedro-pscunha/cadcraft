//! DIMBREAK reads every obstruction before it edits a dimension: a canonical obstruction (a tilted line, a spatial line) is refused, and
//! the legacy dimension is left as it was.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

fn drawing_with_dimension_and_line(points: serde_json::Value) -> (cadcraft_doc::Drawing, cadcraft_doc::Handle, cadcraft_doc::Handle) {
    let mut session = Session::new();
    let dimension = create(&mut session, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]}));
    let line = create(&mut session, "line", json!({"points": points}));
    (session.doc().unwrap().clone(), dimension, line)
}

#[test]
fn an_automatic_break_refuses_a_canonical_obstruction() {
    let (d, dimension, line) = drawing_with_dimension_and_line(json!([[4, -1], [4, 6]]));
    check_refusals(&d, "dimbreak", json!({"handles": [dimension.hex()]}), &[line]);
}

#[test]
fn a_break_by_chosen_objects_refuses_a_canonical_obstruction() {
    let (d, dimension, line) = drawing_with_dimension_and_line(json!([[4, -1], [4, 6]]));
    check_refusals(&d, "dimbreak", json!({"handles": [dimension.hex()], "objects": [line.hex()], "size": 0.5}), &[line]);
}

#[test]
fn a_spatial_canonical_obstruction_is_refused() {
    let mut session = Session::new();
    let dimension = create(&mut session, "dimlinear", json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]}));
    let line = spatial_line(&mut session, [4.0, -1.0, 0.0], [4.0, 6.0, 4.0]);
    check_refusals(session.doc().unwrap(), "dimbreak", json!({"handles": [dimension.hex()]}), &[line]);
}

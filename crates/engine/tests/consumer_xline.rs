//! XLINE from a handle (offset, through, reference) refuses a canonical line or polyline before it makes the construction line.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

#[test]
fn xline_from_a_canonical_source_is_refused_for_each_form() {
    let mut session = Session::new();
    let line = create(&mut session, "line", json!({"points": [[0, 0], [10, 0]]}));
    let d = session.doc().unwrap().clone();
    check_refusals(&d, "xline", json!({"handle": line.hex(), "distance": 2, "side": [5, -1]}), &[line]);
    check_refusals(&d, "xline", json!({"handle": line.hex(), "through": [3, 7]}), &[line]);
}

#[test]
fn xline_from_a_spatial_canonical_line_is_refused() {
    let mut session = Session::new();
    let line = create(&mut session, "line", json!({"points": [[0, 0, 0], [3, 0, 4]]}));
    check_refusals(session.doc().unwrap(), "xline", json!({"handle": line.hex(), "through": [3, 7]}), &[line]);
}

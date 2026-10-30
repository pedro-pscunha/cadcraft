//! CENTERMARK and CENTERLINE refuse a canonical circle, arc or line (a tilted plane, a spatial segment) before they draw the marks.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

#[test]
fn centermark_refuses_a_canonical_circle() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 2}));
    check_refusals(session.doc().unwrap(), "centermark", json!({"handle": circle.hex()}), &[circle]);
}

#[test]
fn centerline_refuses_a_canonical_line_among_two_including_a_spatial_one() {
    let mut session = Session::new();
    let a = spatial_line(&mut session, [10.0, 0.0, 0.0], [13.0, 0.0, 4.0]);
    let b = create(&mut session, "line", json!({"points": [[20, 4], [10, 4]]}));
    check_refusals(session.doc().unwrap(), "centerline", json!({"h1": a.hex(), "h2": b.hex()}), &[a, b]);
}

//! ARC continuation starts from the last line or arc: a canonical last curve is refused before the arc is drawn, not read as a flat 2D end.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

#[test]
fn arc_continue_refuses_a_canonical_last_curve() {
    let mut session = Session::new();
    let line = create(&mut session, "line", json!({"points": [[0, 0], [1, 0]]}));
    check_refusals(session.doc().unwrap(), "arc.continue", json!({"end": [2, 1]}), &[line]);
}

#[test]
fn arc_continue_refuses_a_canonical_open_polyline() {
    let mut session = Session::new();
    let polyline =
        create(&mut session, "pline", json!({"vertices": [{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [1.0, 0.0], "bulge": 0.0}], "closed": false}));
    check_refusals(session.doc().unwrap(), "arc.continue", json!({"end": [2, 1]}), &[polyline]);
}

#[test]
fn a_canonical_curve_before_the_last_one_does_not_block_the_continuation() {
    let mut session = Session::new();
    let earlier = create(&mut session, "line", json!({"points": [[5, 5], [6, 6]]}));
    create(&mut session, "line", json!({"points": [[0, 0], [1, 0]]}));
    let mut canonical = session_of(session.doc().unwrap());
    make_canonical(&mut canonical, earlier);
    canonical.execute("arc.continue", &json!({"end": [2, 1]})).unwrap();
}

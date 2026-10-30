//! DIMJOGGED reads the center and radius of a circle or arc: a canonical one is refused before the dimension is made.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

#[test]
fn dimjogged_refuses_a_canonical_circle_or_arc() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 50}));
    let params = json!({"handle": circle.hex(), "centerOverride": [20, -10], "at": [40, 40], "jog": [30, 15]});
    check_refusals(session.doc().unwrap(), "dimjogged", params, &[circle]);
}

#[test]
fn picking_a_canonical_circle_for_dimjogged_is_refused_with_no_dimension_made() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 50}));
    let mut canonical = session_of(session.doc().unwrap());
    make_canonical(&mut canonical, circle);
    let before = canonical.doc().unwrap().model.clone();
    canonical.start("dimjogged").unwrap();
    // The session reports a refused input on its log and keeps the command at the same prompt.
    let _ = canonical.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(50.0, 0.0)));
    assert!(canonical.log.last().is_some_and(|line| line.contains("canonical")), "{:?}", canonical.log.last());
    assert!(canonical.prompt_text().contains("Select arc or circle"), "{}", canonical.prompt_text());
    assert_eq!(canonical.doc().unwrap().model, before);
}

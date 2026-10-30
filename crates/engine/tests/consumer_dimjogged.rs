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
fn picking_a_canonical_circle_for_dimjogged_makes_no_dimension() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 50}));
    let mut canonical = session_of(session.doc().unwrap());
    make_canonical(&mut canonical, circle);
    let before = canonical.doc().unwrap().model.clone();
    canonical.start("dimjogged").unwrap();
    // The pick does not resolve a canonical circle, and if it ever does, the command refuses it with the typed reason.
    match canonical.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(50.0, 0.0))) {
        Ok(()) => assert!(canonical.prompt_text().contains("Select arc or circle"), "{}", canonical.prompt_text()),
        Err(error) => assert!(error.to_string().contains("canonical"), "{error}"),
    }
    assert_eq!(canonical.doc().unwrap().model, before);
}

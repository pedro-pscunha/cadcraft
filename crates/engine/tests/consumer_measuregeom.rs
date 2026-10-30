//! MEASUREGEOM measures canonical sources in space or refuses them: the angle between two spatial lines is their 3D angle, a canonical
//! polyline gives no arc radius at a plan-view pick, and the interactive picks refuse a canonical circle.

mod consumer_support;

use cadcraft_engine::{Input, Session};
use cadcraft_geom::Vec2;
use consumer_support::*;
use serde_json::json;

fn two_spatial_lines(session: &mut Session, a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> (cadcraft_doc::Handle, cadcraft_doc::Handle) {
    (spatial_line(session, a.0, a.1), spatial_line(session, b.0, b.1))
}

fn angle(session: &mut Session, h1: cadcraft_doc::Handle, h2: cadcraft_doc::Handle) -> cadcraft_engine::Result<f64> {
    session.execute("angle.measure", &json!({"h1": h1.hex(), "h2": h2.hex()})).map(|r| r["angle"].as_f64().unwrap())
}

#[test]
fn the_angle_between_canonical_spatial_lines_is_their_three_dimensional_angle() {
    let mut session = Session::new();
    let (a, b) = two_spatial_lines(&mut session, ([0.0; 3], [1.0, 0.0, 1.0]), ([0.0; 3], [0.0, 1.0, 1.0]));
    // Legacy control: the plan view, the two directions (1, 0) and (0, 1).
    assert!((angle(&mut session, a, b).unwrap() - 90.0).abs() < 1e-9);
    make_canonical(&mut session, a);
    make_canonical(&mut session, b);
    // cos = (1*0 + 0*1 + 1*1) / (sqrt(2) * sqrt(2)) = 1/2.
    assert!((angle(&mut session, a, b).unwrap() - 60.0).abs() < 1e-9);
}

#[test]
fn one_canonical_line_is_enough_for_the_spatial_angle() {
    let mut session = Session::new();
    let (a, b) = two_spatial_lines(&mut session, ([0.0; 3], [1.0, 0.0, 1.0]), ([0.0; 3], [0.0, 1.0, 1.0]));
    make_canonical(&mut session, b);
    assert!((angle(&mut session, a, b).unwrap() - 60.0).abs() < 1e-9);
    assert!((angle(&mut session, b, a).unwrap() - 60.0).abs() < 1e-9);
}

#[test]
fn skew_and_parallel_canonical_lines_have_no_angle() {
    let mut session = Session::new();
    let (a, skew) = two_spatial_lines(&mut session, ([0.0; 3], [1.0, 0.0, 0.0]), ([0.0, 1.0, 1.0], [0.0, 2.0, 1.0]));
    make_canonical(&mut session, a);
    let error = angle(&mut session, a, skew).expect_err("skew lines have no angle");
    assert!(error.to_string().contains("do not meet"), "{error}");
    let parallel = spatial_line(&mut session, [0.0, 1.0, 1.0], [1.0, 1.0, 1.0]);
    let error = angle(&mut session, a, parallel).expect_err("parallel lines have no angle");
    assert!(error.to_string().contains("parallel"), "{error}");
}

#[test]
fn radius_of_a_canonical_polyline_arc_is_refused_and_a_canonical_circle_radius_is_not() {
    let mut session = Session::new();
    let polyline =
        create(&mut session, "pline", json!({"vertices": [{"p": [0.0, 0.0], "bulge": 1.0}, {"p": [4.0, 0.0], "bulge": 0.0}], "closed": false}));
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    let params = json!({"handle": polyline.hex(), "point": [2.0, -2.0]});
    check_refusals(session.doc().unwrap(), "radius.measure", params, &[polyline]);
    make_canonical(&mut session, circle);
    let radius = session.execute("radius.measure", &json!({"handle": circle.hex()})).unwrap()["radius"].as_f64().unwrap();
    assert_eq!(radius, 2.0);
}

#[test]
fn the_interactive_angle_pick_refuses_a_canonical_circle() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 50}));
    let mut canonical = session_of(session.doc().unwrap());
    make_canonical(&mut canonical, circle);
    canonical.start("angle.measure").unwrap();
    let _ = canonical.input(Input::Point(Vec2::new(50.0, 0.0)));
    assert!(canonical.log.last().is_some_and(|line| line.contains("canonical")), "{:?}", canonical.log.last());
    assert!(canonical.prompt_text().contains("Select arc, circle, line"), "{}", canonical.prompt_text());
}

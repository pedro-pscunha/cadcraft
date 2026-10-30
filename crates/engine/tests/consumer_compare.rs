//! COMPARE signs the meaning of the geometry numbers: the same fields in another revision or another plane are a change, the same
//! fields in the same plane are not.

mod consumer_support;

use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::{Value, json};

/// A drawing with one circle, then the same drawing open as "B" with the circle made canonical in `extrusion` (or left alone).
fn session_with_copy(extrusion: Option<Vec3>) -> Session {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    let copy = s.doc().unwrap().clone();
    s.open_drawing(copy, "B", None);
    if let Some(extrusion) = extrusion {
        make_canonical_in(&mut s, circle, extrusion);
    }
    s
}

fn counts(r: &Value) -> (u64, u64) {
    (r["counts"]["changed"].as_u64().unwrap(), r["counts"]["common"].as_u64().unwrap())
}

#[test]
fn a_legacy_pair_with_the_same_fields_is_the_same() {
    let mut s = session_with_copy(None);
    assert_eq!(counts(&s.execute("compare", &json!({"document": "drawing1"})).unwrap()), (0, 1));
}

#[test]
fn the_same_fields_in_another_revision_are_a_change() {
    let mut s = session_with_copy(Some(Vec3::Z));
    let r = s.execute("compare", &json!({"document": "drawing1"})).unwrap();
    assert_eq!(counts(&r), (1, 0), "{r}");
    assert_eq!(r["changed"][0]["what"], "geometry", "{r}");
}

#[test]
fn two_canonical_circles_with_the_same_fields_in_different_planes_are_a_change() {
    let mut s = session_with_copy(Some(Vec3::new(0.0, 0.6, 0.8)));
    // The other drawing's circle is canonical too, in the plane of the normal Z.
    let circle = s.doc().unwrap().model.iter().next().unwrap().handle;
    let mut flat = session_of(s.doc().unwrap());
    make_canonical_in(&mut flat, circle, Vec3::Z);
    let other = flat.doc().unwrap().clone();
    s.open_drawing(other, "C", None);
    let r = s.execute("compare", &json!({"document": "B"})).unwrap();
    assert_eq!(counts(&r), (1, 0), "{r}");
}

#[test]
fn two_canonical_circles_in_the_same_plane_are_the_same() {
    let mut s = session_with_copy(Some(Vec3::new(0.0, 0.6, 0.8)));
    let other = s.doc().unwrap().clone();
    s.open_drawing(other, "C", None);
    assert_eq!(counts(&s.execute("compare", &json!({"document": "B"})).unwrap()), (0, 1));
}

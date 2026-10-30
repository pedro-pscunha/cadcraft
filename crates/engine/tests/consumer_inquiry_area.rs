//! AREA measures a canonical ellipse in its own plane: the major axis (3, 0, 4) is 5 long, not the 3 of its plan view.

mod consumer_support;

use cadcraft_doc::{Common, Ellipse, EntityKind, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::json;

fn ellipse(session: &mut Session) -> cadcraft_doc::Handle {
    let ellipse =
        Ellipse { center: Vec3::ZERO, major: Vec3::new(3.0, 0.0, 4.0), normal: Vec3::Z, ratio: 0.5, start: 0.0, end: std::f64::consts::TAU };
    session.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Ellipse(ellipse)).unwrap()
}

fn area_of(session: &mut Session, handle: cadcraft_doc::Handle) -> f64 {
    session.execute("area", &json!({"handle": handle.hex()})).unwrap()["area"].as_f64().unwrap()
}

#[test]
fn a_canonical_ellipse_has_the_area_of_its_plane() {
    let mut session = Session::new();
    let handle = ellipse(&mut session);
    assert!((area_of(&mut session, handle) - 4.5 * std::f64::consts::PI).abs() < 1e-9, "legacy control keeps the plan view");
    // The plane of the major axis (3, 0, 4) and the normal (0, 1, 0).
    make_canonical_in(&mut session, handle, Vec3::new(0.0, 1.0, 0.0));
    assert!((area_of(&mut session, handle) - 12.5 * std::f64::consts::PI).abs() < 1e-9);
}

#[test]
fn a_canonical_elliptical_arc_has_no_area() {
    let mut session = Session::new();
    let handle = ellipse(&mut session);
    let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    if let EntityKind::Ellipse(e) = &mut entity.kind {
        e.end = 3.0;
    }
    session.doc_mut().unwrap().model.push(entity);
    make_canonical_in(&mut session, handle, Vec3::new(0.0, 1.0, 0.0));
    assert!(session.execute("area", &json!({"handle": handle.hex()})).is_err());
}

//! ID reports the Z of a canonical object at a point from its world geometry: a spatial LINE gives the Z along it, a circle in a tilted
//! plane gives the Z of its WCS points, not its OCS center.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

fn z_of(session: &mut Session, at: [f64; 2]) -> f64 {
    session.execute("id", &json!({"at": at})).unwrap()["z"].as_f64().unwrap()
}

#[test]
fn id_on_a_canonical_spatial_line_gives_the_z_along_it() {
    let mut session = Session::new();
    let line = spatial_line(&mut session, [0.0, 0.0, 0.0], [3.0, 0.0, 4.0]);
    assert!((z_of(&mut session, [1.5, 0.0]) - 2.0).abs() < 1e-9, "legacy control");
    let mut canonical = session_of(session.doc().unwrap());
    let mut entity = canonical.doc().unwrap().entity(line).unwrap().as_ref().clone();
    entity.geometry_revision = cadcraft_doc::GeometryRevision::CanonicalV2;
    canonical.doc_mut().unwrap().model.push(entity);
    assert!((z_of(&mut canonical, [1.5, 0.0]) - 2.0).abs() < 1e-9);
}

#[test]
fn id_on_a_canonical_circle_in_a_tilted_plane_gives_the_world_z() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    // The legacy circle reads its center z (0) as a world value.
    assert_eq!(z_of(&mut session, [12.0, 5.0]), 0.0);
    make_canonical(&mut session, circle);
    // The OCS of the normal (0, 0.6, 0.8): x axis (-1, 0, 0), y axis (0, -0.8, 0.6). The circle point at t = 0 is the OCS point
    // (12, 5, 0), which is the world point (-12, -4, 3).
    assert!((z_of(&mut session, [-12.0, -4.0]) - 3.0).abs() < 1e-6);
}

#[test]
fn id_on_the_reviews_tilted_circle_gives_z_17_8() {
    use cadcraft_doc::{Circle, Common, EntityKind, Space};
    use cadcraft_geom::Vec3;
    let mut session = Session::new();
    let circle = Circle { center: Vec3::new(10.0, 20.0, 3.0), radius: 1.0 };
    let handle = session.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Circle(circle)).unwrap();
    assert_eq!(z_of(&mut session, [11.0, 20.0]), 3.0, "legacy control reads the OCS center z");
    make_canonical_in(&mut session, handle, Vec3::new(0.0, 0.8, 0.6));
    // The OCS of the normal (0, 0.8, 0.6) has x axis (-1, 0, 0) and y axis (0, -0.6, 0.8): the center is (-10, -9.6, 17.8) and the
    // curve point at t = 0 is (-11, -9.6, 17.8).
    assert!((z_of(&mut session, [-11.0, -9.6]) - 17.8).abs() < 1e-9);
}

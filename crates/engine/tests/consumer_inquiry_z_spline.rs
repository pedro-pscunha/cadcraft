//! ID and DIST give the world Z of a picked canonical spatial SPLINE, and refuse (a typed error, never the ELEVATION) a canonical object
//! whose Z at the point cannot be told, such as a text.

mod consumer_support;

use cadcraft_doc::{Common, EntityKind, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::json;

fn spatial_spline(session: &mut Session) -> cadcraft_doc::Handle {
    // Control points on y = 0 with Z equal to X: the curve has Z = X everywhere.
    let mut spline = cadcraft_geom::Spline::from_control(
        vec![cadcraft_geom::Vec2::new(0.0, 0.0), cadcraft_geom::Vec2::new(5.0, 0.0), cadcraft_geom::Vec2::new(10.0, 0.0)],
        2,
    );
    spline.space = Some(cadcraft_geom::SplineSpace { control: vec![0.0, 5.0, 10.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 });
    session.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Spline(spline)).unwrap()
}

#[test]
fn id_on_a_canonical_spatial_spline_gives_its_world_z() {
    let mut session = Session::new();
    let handle = spatial_spline(&mut session);
    make_canonical_in(&mut session, handle, Vec3::Z);
    for x in [2.0, 6.0] {
        let z = session.execute("id", &json!({"at": [x, 0.0]})).unwrap()["z"].as_f64().unwrap();
        assert!((z - x).abs() < 1e-6, "z {z} at x {x}");
    }
    // Empty space still gets the elevation.
    assert_eq!(session.execute("id", &json!({"at": [5.0, 30.0]})).unwrap()["z"], 0.0);
    let dist = session.execute("dist", &json!({"p1": [2.0, 0.0], "p2": [8.0, 0.0]})).unwrap();
    assert!((dist["dz"].as_f64().unwrap() - 6.0).abs() < 1e-6, "{dist}");
}

#[test]
fn id_on_a_canonical_text_is_refused_not_answered_with_the_elevation() {
    let mut session = Session::new();
    let text = create(&mut session, "text", json!({"at": [2, 1], "height": 1, "text": "A"}));
    make_canonical_in(&mut session, text, Vec3::Z);
    let error = session.execute("id", &json!({"at": [2.3, 1.4]})).expect_err("a canonical text answered with a Z");
    assert!(error.to_string().contains("no single Z"), "{error}");
    // Away from the text it is empty space.
    assert_eq!(session.execute("id", &json!({"at": [20.0, 20.0]})).unwrap()["z"], 0.0);
}

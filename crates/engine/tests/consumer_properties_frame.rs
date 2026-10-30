//! PROPERTIES names the coordinate frame of a canonical entity's payload from its family's contract: WCS for an ELLIPSE, a spatial SPLINE
//! or an MTEXT, the OCS for a CIRCLE, and per field for a DIMENSION or a POINT. Each field a mixed contract names exists in the payload.

mod consumer_support;

use cadcraft_doc::{Common, Dimension, Ellipse, EntityKind, MText, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::{Value, json};

fn add(session: &mut Session, kind: EntityKind) -> cadcraft_doc::Handle {
    session.doc_mut().unwrap().add(&Space::Model, Common::default(), kind).unwrap()
}

fn props(session: &mut Session, handle: cadcraft_doc::Handle) -> Value {
    session.execute("properties", &json!({"handles": [handle.hex()]})).unwrap()["objects"][0].clone()
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(1.0, 2.0, 3.0),
        height: 2.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "NOTE".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(1.0, 0.0, 0.0)),
        drawing_direction: None,
    }
}

#[test]
fn wcs_families_say_wcs_and_ocs_families_say_ocs_with_a_nondefault_plane() {
    let mut s = Session::new();
    let ellipse = add(
        &mut s,
        EntityKind::Ellipse(Ellipse {
            center: Vec3::ZERO,
            major: Vec3::new(3.0, 0.0, 4.0),
            normal: Vec3::Z,
            ratio: 0.5,
            start: 0.0,
            end: std::f64::consts::TAU,
        }),
    );
    let mut spline = cadcraft_geom::Spline::from_control(
        vec![cadcraft_geom::Vec2::new(0.0, 0.0), cadcraft_geom::Vec2::new(5.0, 0.0), cadcraft_geom::Vec2::new(10.0, 0.0)],
        2,
    );
    spline.space = Some(cadcraft_geom::SplineSpace { control: vec![0.0, 5.0, 10.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 });
    let spline = add(&mut s, EntityKind::Spline(spline));
    let text = add(&mut s, EntityKind::MText(mtext()));
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    let line = spatial_line(&mut s, [0.0; 3], [3.0, 0.0, 4.0]);
    for (handle, frame) in [(ellipse, "wcs"), (spline, "wcs"), (text, "wcs"), (circle, "ocs"), (line, "wcs")] {
        make_canonical(&mut s, handle);
        let p = props(&mut s, handle);
        assert_eq!(p["geometryFrame"], frame, "{}: {p}", p["type"]);
        assert!(p.get("geometryFieldFrames").is_none(), "{}", p["type"]);
    }
}

#[test]
fn a_dimension_and_a_point_name_the_frame_of_each_field() {
    let mut s = Session::new();
    let dimension = Dimension {
        kind: cadcraft_doc::DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(15.0, 5.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(30.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    };
    let dim = add(&mut s, EntityKind::Dimension(dimension));
    let point = add(&mut s, EntityKind::Point(cadcraft_doc::Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.0 }));
    for handle in [dim, point] {
        make_canonical(&mut s, handle);
        let p = props(&mut s, handle);
        assert_eq!(p["geometryFrame"], "mixed", "{p}");
        let fields = p["geometryFieldFrames"].as_object().expect("per-field frames");
        for (field, frame) in fields {
            assert!(["wcs", "ocs"].contains(&frame.as_str().unwrap()), "{field}: {frame}");
            // `marks` and `angle` are left out of the payload while they hold their defaults.
            let optional = ["marks", "angle"].contains(&field.as_str());
            assert!(
                optional || p["geometry"].get(field).is_some(),
                "the contract names `{field}` but the payload has no such field: {}",
                p["geometry"]
            );
        }
    }
    let p = props(&mut s, dim);
    assert_eq!(p["geometryFieldFrames"]["defpt"], "wcs");
    assert_eq!(p["geometryFieldFrames"]["textMid"], "ocs");
    let p = props(&mut s, point);
    assert_eq!((p["geometryFieldFrames"]["p"].clone(), p["geometryFieldFrames"]["angle"].clone()), (json!("wcs"), json!("ocs")));
}

#[test]
fn a_legacy_entity_names_no_frame() {
    let mut s = Session::new();
    let circle = create(&mut s, "circle", json!({"center": [10, 5], "radius": 2}));
    let p = props(&mut s, circle);
    assert!(p.get("geometryFrame").is_none() && p.get("geometryRevision").is_none(), "{p}");
}

use cadcraft_doc::{Ellipse, EntityKind, Prim};
use cadcraft_geom::{Mat3, Vec2, Vec3};

fn ellipse(normal: Vec3, major: Vec3) -> Ellipse {
    Ellipse { center: Vec3::new(10.0, 20.0, 3.0), major, normal, ratio: 0.4, start: 0.3, end: 2.4 }
}

#[test]
fn projected_ellipse_endpoints_follow_the_source_plane() {
    for (normal, major) in [
        (-Vec3::Z, Vec3::new(5.0, 2.0, 0.0)),
        (Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6)),
        (Vec3::new(0.0, 0.99, 0.1), Vec3::new(1.0, 0.0, 0.0)),
    ] {
        let e = ellipse(normal, major);
        let minor = normal.normalized().cross(major).normalized() * (major.len() * e.ratio);
        let expected = [e.start, e.end].map(|t| (e.center + major * t.cos() + minor * t.sin()).xy());
        let kind = EntityKind::Ellipse(e);
        let Prim::Ellipse(projected) = kind.prims()[0] else { panic!("ellipse primitive") };
        let actual = [projected.at_param(projected.start), projected.at_param(projected.end)];
        for p in expected {
            assert!(actual.iter().any(|q| q.near(p, 1e-10)), "{normal:?}: {actual:?} does not contain {p:?}");
        }
    }
}

#[test]
fn ellipse_serde_defaults_legacy_data_and_retains_new_planes() {
    let source = ellipse(Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6));
    let encoded = serde_json::to_value(&source).unwrap();
    assert_eq!(serde_json::from_value::<Ellipse>(encoded.clone()).unwrap(), source);
    let mut legacy = encoded;
    legacy.as_object_mut().unwrap().remove("normal");
    assert_eq!(serde_json::from_value::<Ellipse>(legacy).unwrap().normal, Vec3::Z);
}

#[test]
fn projected_grips_use_the_same_axes_as_the_display() {
    let kind = EntityKind::Ellipse(ellipse(Vec3::new(0.0, 0.8, 0.6), Vec3::new(1.0, 0.0, 0.0)));
    let Prim::Ellipse(projected) = kind.prims()[0] else { panic!("ellipse primitive") };
    let grips = kind.grips();
    assert!(grips[2].near(projected.center + projected.minor(), 1e-12));
    assert!(grips[2].near(Vec2::new(10.0, 20.24), 1e-12));
}

#[test]
fn identity_transform_keeps_source_ellipse_bits() {
    let mut kind = EntityKind::Ellipse(ellipse(Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6)));
    let before = kind.clone();
    kind.transform(&Mat3::IDENTITY);
    assert_eq!(kind, before);
}

#[test]
fn an_ellipse_edit_never_pairs_a_flat_axis_with_a_tilted_normal() {
    let mut kind = EntityKind::Ellipse(ellipse(Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6)));
    kind.transform(&Mat3::translate(Vec2::new(2.0, 3.0)));
    let EntityKind::Ellipse(e) = kind else { panic!("ellipse") };
    assert!(e.major.dot(e.normal).abs() <= 1e-12, "{e:?}");
}

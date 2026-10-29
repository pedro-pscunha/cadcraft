use cadcraft_geom::{Mat4, OcsFrame, Vec3};

#[test]
fn invalid_normals_do_not_create_a_computational_frame() {
    for normal in [Vec3::ZERO, Vec3::new(f64::NAN, 0.0, 1.0), Vec3::new(0.0, f64::INFINITY, 1.0)] {
        assert!(OcsFrame::new(normal).is_none());
    }
}

#[test]
fn frames_round_trip_on_both_sides_of_the_arbitrary_axis_branch() {
    for normal in [
        Vec3::Z,
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(0.0, 0.8, 0.6),
        Vec3::new(5e-13, 0.0, 1.0),
        Vec3::new(5e-13, 0.0, -1.0),
        Vec3::new(1.0 / 65.0, 0.0, 1.0),
        Vec3::new(1.0 / 63.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, f64::from_bits(1)),
    ] {
        let frame = OcsFrame::new(normal).unwrap();
        let point = Vec3::new(7.0, -11.0, 3.0);
        close(frame.to_ocs(frame.to_world(point)), point);
        close(frame.matrix().apply(point), frame.to_world(point));
        let x = frame.to_world(Vec3::new(1.0, 0.0, 0.0));
        let y = frame.to_world(Vec3::new(0.0, 1.0, 0.0));
        let z = frame.to_world(Vec3::Z);
        close(x.cross(y), z);
        assert!((z.len() - 1.0).abs() < 1e-12);
    }
}

#[test]
fn the_arbitrary_axis_branch_uses_the_normalized_direction() {
    let below = OcsFrame::new(Vec3::new(1.0 / 65.0, 0.0, 1.0)).unwrap();
    let above = OcsFrame::new(Vec3::new(1.0 / 63.0, 0.0, 1.0)).unwrap();
    assert!(below.to_world(Vec3::new(1.0, 0.0, 0.0)).x > 0.99);
    assert_eq!(above.to_world(Vec3::new(1.0, 0.0, 0.0)), Vec3::new(0.0, 1.0, 0.0));
}

fn close(actual: Vec3, expected: Vec3) {
    assert!((actual - expected).len() < 1e-12, "{actual:?} != {expected:?}");
}

#[test]
fn a_small_tilt_is_not_an_identity_frame() {
    let frame = Mat4::ocs(Vec3::new(5e-13, 0.0, 1.0));
    let world = frame.apply(Vec3::new(0.0, 0.0, 1e13));
    assert!((world.x - 5.0).abs() < 1e-12, "{world:?}");
}

#[test]
fn the_frame_depends_on_direction_not_normal_magnitude() {
    let expected = Mat4::ocs(Vec3::new(0.0, 4.0, 3.0));
    for normal in [Vec3::new(0.0, 4e-200, 3e-200), Vec3::new(0.0, 1.6e308, 1.2e308)] {
        let actual = Mat4::ocs(normal);
        for axis in [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::Z] {
            close(actual.apply(axis), expected.apply(axis));
        }
    }
}

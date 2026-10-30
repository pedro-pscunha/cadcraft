//! The curve of a spline-fit polyline: the B-spline of degree 2 or 3 whose control points are the frame.

use cadcraft_geom::{Vec3, frame_curve};

fn frame() -> Vec<Vec3> {
    vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 1.0), Vec3::new(6.0, 4.0, 2.0), Vec3::new(9.0, 0.0, 3.0)]
}

#[test]
fn an_open_curve_runs_from_the_first_frame_point_to_the_last() {
    for degree in [2, 3] {
        let c = frame_curve(&frame(), false, degree, 8, 20_000);
        assert_eq!(c.len(), 8 * 3 + 1, "degree {degree}");
        assert!((c[0] - frame()[0]).len() < 1e-9 && (c[c.len() - 1] - frame()[3]).len() < 1e-9, "degree {degree}");
    }
}

#[test]
fn a_closed_curve_has_segs_points_per_frame_point_and_stays_inside_the_frame() {
    for degree in [2, 3] {
        let c = frame_curve(&frame(), true, degree, 8, 20_000);
        assert_eq!(c.len(), 8 * 4, "degree {degree}");
        // A uniform B-spline is a convex combination of its frame points: the curve stays in their box.
        assert!(c.iter().all(|p| (0.0..=9.0).contains(&p.x) && (0.0..=4.0).contains(&p.y) && (0.0..=3.0).contains(&p.z)), "degree {degree}");
    }
}

#[test]
fn fewer_than_three_frame_points_or_another_degree_give_the_frame() {
    assert_eq!(frame_curve(&frame()[..2], false, 3, 8, 100), frame()[..2].to_vec());
    assert_eq!(frame_curve(&frame(), false, 5, 8, 100), frame());
}

#[test]
fn the_number_of_points_is_capped() {
    assert!(frame_curve(&frame(), false, 3, 64, 40).len() <= 40);
    assert!(frame_curve(&frame(), true, 3, 64, 40).len() <= 40);
}

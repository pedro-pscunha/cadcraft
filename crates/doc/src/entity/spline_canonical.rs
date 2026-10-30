//! Canonical SPLINE geometry.
//!
//! A canonical spline is spatial: its control points, fit points and fit tangents are WCS points and vectors, kept as the
//! record holds them (the X and Y values in the planar fields of the spline and the Z values in `Spline::space`). The
//! knots, weights and degree are kept too, and the spline is never rebuilt from its fit points. `Common.extrusion` is
//! the normal of the plane of a planar spline, kept with its source magnitude.
//!
//! A B-spline is affine-invariant, so any invertible affine map moves the curve exactly by moving its control points: a
//! rotation, a scale (uniform or not), a mirror and a shear are all exact. A collapse (a singular map) loses the curve and
//! refuses, and so does a payload that is not spatial.

use cadcraft_geom::{Mat4, OcsFrame, Spline, Vec3};

use crate::{DocError, Result};

/// This spatial spline in the plane of `normal` after the affine `transform`, and the new normal: the inverse-transpose
/// image of the plane normal with the source magnitude.
pub fn spline_transformed_canonical(spline: &Spline, normal: Vec3, transform: &Mat4) -> Result<(Spline, Vec3)> {
    let moved = spline
        .transformed3(transform)
        .ok_or_else(|| DocError::Invalid("A spatial spline can follow an invertible affine map; this payload or map is not one.".into()))?;
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical spline needs a plane with a finite, non-zero normal.".into()))?;
    let unit =
        transform.apply_normal(normal).ok_or_else(|| DocError::Invalid("The normal of a canonical spline has no image under this map.".into()))?;
    Ok((moved, unit * normal.len()))
}

#[cfg(test)]
mod tests {
    use cadcraft_geom::{FitOptions, SplineSpace, Vec2};

    use super::*;

    fn spline() -> Spline {
        Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, 0.0)],
            weights: Vec::new(),
            fit: Vec::new(),
            closed: false,
            fit_opts: FitOptions::default(),
            space: Some(SplineSpace { control: vec![0.0, 0.0, 0.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 }),
            source: Default::default(),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn a_rotation_turns_the_plane_normal_and_keeps_its_magnitude() {
        let (s, c) = 0.7f64.sin_cos();
        let rx = Mat4 { m: [[1.0, 0.0, 0.0, 4.0], [0.0, c, -s, -2.0], [0.0, s, c, 1.0], [0.0, 0.0, 0.0, 1.0]] };
        let (moved, n) = spline_transformed_canonical(&spline(), Vec3::new(0.0, 0.0, 2.0), &rx).unwrap();
        assert!(near(n, Vec3::new(0.0, -2.0 * s, 2.0 * c)), "{n:?}");
        for (a, b) in spline().control3().unwrap().iter().zip(moved.control3().unwrap()) {
            assert!(near(rx.apply(*a), b));
        }
        // The turned control points still lie in the turned plane.
        let base = moved.control3().unwrap()[0];
        assert!(moved.control3().unwrap().iter().all(|p| (*p - base).dot(n).abs() < 1e-9));
    }

    #[test]
    fn a_non_uniform_scale_moves_the_normal_by_the_inverse_transpose() {
        let squash = Mat4 { m: [[2.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        // A plane with normal (1, 1, 0): its image has normal proportional to (1/2, 1, 0).
        let (_, n) = spline_transformed_canonical(&spline(), Vec3::new(1.0, 1.0, 0.0), &squash).unwrap();
        let unit = Vec3::new(0.5, 1.0, 0.0).normalized() * Vec3::new(1.0, 1.0, 0.0).len();
        assert!(near(n, unit), "{n:?}");
    }

    #[test]
    fn a_collapse_a_planar_payload_or_a_bad_normal_refuses() {
        let flat = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(spline_transformed_canonical(&spline(), Vec3::Z, &flat).is_err());
        assert!(spline_transformed_canonical(&Spline { space: None, ..spline() }, Vec3::Z, &Mat4::IDENTITY).is_err());
        assert!(spline_transformed_canonical(&spline(), Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }
}

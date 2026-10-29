//! Canonical HELIX geometry.
//!
//! A canonical helix keeps what its record declares: the axis base point (`center`) and start point in WCS, the axis
//! vector with its source magnitude, the radii and the heights along the axis. Its frame is the unit axis `a`, the unit
//! direction `e1` from the axis to the start point, and `e2 = a x e1`, so a counter-clockwise helix turns from `e1`
//! towards `e2` seen from the end of the axis. The curve is the legacy helix in that frame, which maps affinely to WCS,
//! so the control points of its spline map exactly.
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact. A mirror turns the handedness
//! and has no measured convention, a non-uniform scale or shear makes the curve a different one: they refuse atomically.

use cadcraft_geom::{Mat4, OcsFrame, Spline as GSpline, Vec3};

use super::helix::Helix;
use super::text_canonical::Similarity;
use crate::{DocError, Result};

fn invalid() -> DocError {
    DocError::Invalid("A canonical helix needs a finite, non-zero axis and a finite start point.".into())
}

impl Helix {
    /// The unit vectors `(e1, e2, a)` of the helix frame.
    fn basis(&self) -> Result<(Vec3, Vec3, Vec3)> {
        let frame = self.frame.ok_or_else(invalid)?;
        let finite = frame.axis.is_finite() && frame.start.is_finite() && self.center.is_finite();
        let axis = OcsFrame::new(frame.axis).filter(|_| finite).ok_or_else(invalid)?;
        let a = axis.to_world(Vec3::Z);
        let radial = frame.start - self.center;
        let perpendicular = radial - a * radial.dot(a);
        let e1 =
            if perpendicular.len() > 1e-12 * radial.len().max(1.0) { perpendicular.normalized() } else { axis.to_world(Vec3::new(1.0, 0.0, 0.0)) };
        Ok((e1, a.cross(e1), a))
    }

    /// The helix in its own frame: round the origin, axis +Z, start angle 0.
    fn local(&self) -> Helix {
        Helix { center: Vec3::ZERO, start_angle: 0.0, frame: None, ..self.clone() }
    }

    fn world(&self, p: Vec3) -> Result<Vec3> {
        let (e1, e2, a) = self.basis()?;
        Ok(self.center + e1 * p.x + e2 * p.y + a * p.z)
    }

    /// The point at `t` (0 = start, 1 = end) along the helix, in WCS.
    pub fn world_point_at(&self, t: f64) -> Result<Vec3> {
        self.world(self.local().point_at(t))
    }

    /// The helix as a cubic spline: its degree and knots, and the control points in WCS.
    pub fn world_spline3(&self) -> Result<(GSpline, Vec<Vec3>)> {
        let (spline, z) = self.local().spline3();
        let points = spline.control.iter().zip(&z).map(|(c, z)| self.world(c.to3(*z))).collect::<Result<Vec<_>>>()?;
        Ok((spline, points))
    }

    /// This helix after the similarity `transform`.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<Helix> {
        self.basis()?;
        let sim = Similarity::from_mat4(transform)?;
        let frame = self.frame.ok_or_else(invalid)?;
        let k = sim.scale;
        Ok(Helix {
            center: sim.point(self.center),
            base_radius: self.base_radius * k,
            top_radius: self.top_radius * k,
            height: self.height * k,
            frame: Some(super::helix::HelixFrame { axis: sim.direction(frame.axis), start: sim.point(frame.start) }),
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HelixFrame;

    fn helix() -> Helix {
        Helix {
            center: Vec3::new(1.0, 2.0, 3.0),
            base_radius: 2.0,
            top_radius: 2.0,
            height: 4.0,
            turns: 2.0,
            start_angle: 0.0,
            ccw: true,
            constrain: Default::default(),
            // An axis along +X, doubled: the start point lies 2 away along +Y.
            frame: Some(HelixFrame { axis: Vec3::new(2.0, 0.0, 0.0), start: Vec3::new(1.0, 4.0, 3.0) }),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn the_curve_starts_at_the_start_point_and_rises_along_the_axis() {
        let h = helix();
        assert!(near(h.world_point_at(0.0).unwrap(), Vec3::new(1.0, 4.0, 3.0)));
        // Two full turns: back to the start direction, 4 along +X.
        assert!(near(h.world_point_at(1.0).unwrap(), Vec3::new(5.0, 4.0, 3.0)), "{:?}", h.world_point_at(1.0));
        // A quarter turn of a counter-clockwise helix about +X goes from +Y towards +Z.
        assert!(h.world_point_at(0.125).unwrap().z > 3.0);
    }

    #[test]
    fn the_spline_control_points_lie_on_the_axis_cylinder_and_keep_the_height() {
        let (_, c) = helix().world_spline3().unwrap();
        assert!(c.len() > 8);
        for p in &c {
            let r = Vec3::new(0.0, p.y - 2.0, p.z - 3.0).len();
            assert!(r > 1.0 && r < 3.2, "{p:?}");
        }
        assert!(near(c[0], Vec3::new(1.0, 4.0, 3.0)));
        assert!((c.last().unwrap().x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn a_rotation_turns_the_frame_and_a_scale_grows_it() {
        let (s, c) = std::f64::consts::FRAC_PI_2.sin_cos();
        let rz = Mat4 { m: [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let moved = helix().transformed_canonical(&rz).unwrap();
        assert!(near(moved.frame.unwrap().axis, Vec3::new(0.0, 2.0, 0.0)), "axis keeps its magnitude");
        assert!(near(moved.world_point_at(0.0).unwrap(), rz.apply(helix().world_point_at(0.0).unwrap())));
        assert!(near(moved.world_point_at(0.7).unwrap(), rz.apply(helix().world_point_at(0.7).unwrap())));
        let k = Mat4 { m: [[3.0, 0.0, 0.0, 0.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 3.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let big = helix().transformed_canonical(&k).unwrap();
        assert_eq!((big.base_radius, big.height), (6.0, 12.0));
        assert!(near(big.world_point_at(0.4).unwrap(), k.apply(helix().world_point_at(0.4).unwrap())));
    }

    #[test]
    fn a_mirror_or_a_zero_axis_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(helix().transformed_canonical(&mirror).is_err());
        let mut h = helix();
        h.frame = Some(HelixFrame { axis: Vec3::ZERO, start: Vec3::new(1.0, 4.0, 3.0) });
        assert!(h.world_point_at(0.0).is_err());
    }
}

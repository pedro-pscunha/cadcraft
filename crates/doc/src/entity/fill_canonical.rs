//! Canonical WIPEOUT geometry (and, in the same module, the other fills as their arcs land).
//!
//! A canonical wipeout keeps the image frame its record declares: the origin and the per-pixel U and V vectors in WCS,
//! the image size and the clip in pixel space. The outline is derived from them, `origin + u * (x + 0.5) + v * (0.5 - y)`
//! for each clip vertex. The frame is an affine frame of its plane, so every finite affine map that does not collapse
//! the plane (any rotation, scale, mirror or shear, in 3D) is exactly representable; a perspective map or a collapse
//! refuses atomically.

use cadcraft_geom::{Mat4, Vec3};

use super::fill::{Wipeout, WipeoutFrame};
use crate::{DocError, Result};

fn invalid(what: &str) -> DocError {
    DocError::Invalid(format!("A canonical wipeout needs {what}."))
}

impl WipeoutFrame {
    fn check(&self) -> Result<()> {
        let finite =
            self.origin.is_finite() && self.u.is_finite() && self.v.is_finite() && self.size.is_finite() && self.clip.iter().all(|p| p.is_finite());
        if !finite {
            return Err(invalid("finite coordinates"));
        }
        let area = self.u.cross(self.v).len();
        if area <= 1e-12 * self.u.len() * self.v.len() || area <= 0.0 {
            return Err(invalid("independent U and V vectors"));
        }
        Ok(())
    }

    /// The outline in WCS: the polygon's vertices, or the four corners of a rectangular clip.
    pub fn world_boundary(&self) -> Result<Vec<Vec3>> {
        self.check()?;
        let place = |x: f64, y: f64| self.origin + self.u * (x + 0.5) + self.v * (0.5 - y);
        if self.clip_type == 1 && self.clip.len() == 2 {
            let (a, b) = (self.clip[0], self.clip[1]);
            return Ok(vec![place(a.x, a.y), place(b.x, a.y), place(b.x, b.y), place(a.x, b.y)]);
        }
        Ok(self.clip.iter().map(|p| place(p.x, p.y)).collect())
    }
}

impl Wipeout {
    /// The outline of a canonical wipeout in WCS; an error for a legacy one (its `boundary` is already XY).
    pub fn world_boundary(&self) -> Result<Vec<Vec3>> {
        self.frame.as_ref().ok_or_else(|| invalid("an image frame"))?.world_boundary()
    }

    /// This wipeout after any affine `transform` that keeps its plane a plane.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<Wipeout> {
        let frame = self.frame.as_ref().ok_or_else(|| invalid("an image frame"))?;
        frame.check()?;
        let m = &transform.m;
        if m[3] != [0.0, 0.0, 0.0, 1.0] || !m.iter().flatten().all(|v| v.is_finite()) {
            return Err(DocError::Invalid(
                "A wipeout can be moved, rotated, scaled, mirrored and sheared; this transform has another effect.".into(),
            ));
        }
        let moved =
            WipeoutFrame { origin: transform.apply(frame.origin), u: transform.apply_vec(frame.u), v: transform.apply_vec(frame.v), ..frame.clone() };
        moved.check()?;
        Ok(Wipeout { frame: Some(moved), ..self.clone() })
    }
}

#[cfg(test)]
mod tests {
    use cadcraft_geom::Vec2;

    use super::*;

    fn wipeout() -> Wipeout {
        Wipeout {
            boundary: Vec::new(),
            frame: Some(WipeoutFrame {
                origin: Vec3::new(10.0, 0.0, 0.0),
                u: Vec3::new(4.0, 0.0, 0.0),
                v: Vec3::new(0.0, 0.0, 2.0),
                size: Vec2::new(1.0, 1.0),
                clip_type: 2,
                clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
            }),
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn the_outline_lies_in_the_plane_of_u_and_v() {
        let b = wipeout().world_boundary().unwrap();
        assert!(near(b[0], Vec3::new(10.0, 0.0, 2.0)) && near(b[2], Vec3::new(14.0, 0.0, 0.0)), "{b:?}");
        assert!(b.iter().all(|p| p.y == 0.0), "a vertical wipeout keeps its Z");
    }

    #[test]
    fn a_rectangular_clip_gives_four_corners() {
        let mut w = wipeout();
        let f = w.frame.as_mut().unwrap();
        (f.clip_type, f.clip) = (1, vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, 0.5)]);
        assert_eq!(w.world_boundary().unwrap().len(), 4);
    }

    #[test]
    fn a_mirror_or_a_shear_is_exact_and_a_collapse_or_perspective_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let moved = wipeout().transformed_canonical(&mirror).unwrap();
        let (a, b) = (wipeout().world_boundary().unwrap(), moved.world_boundary().unwrap());
        assert!(a.iter().zip(&b).all(|(p, q)| near(Vec3::new(-p.x, p.y, p.z), *q)));
        let flat = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(wipeout().transformed_canonical(&flat).is_err());
        let perspective = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.1, 0.0, 0.0, 1.0]] };
        assert!(wipeout().transformed_canonical(&perspective).is_err());
    }
}

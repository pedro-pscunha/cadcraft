//! Canonical MLINE geometry.
//!
//! A canonical multiline keeps its vertices, and at each vertex the direction, the miter and the element parameters
//! its record declares, as WCS vectors and distances (`MLineVertex::frame`); the plane is the normal of the entity.
//! The elements' offsets and the caps stay with the style (`def`).
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact: the vertices move, the
//! directions turn, and every distance (the parameters and the offset scale) grows with the scale. A reflection swaps
//! the sides of the elements and has no measured convention for a tilted plane, so it refuses, as do a shear and a
//! non-uniform scale.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::mline::{MLine, MLineVertexFrame};
use super::text_canonical::Similarity;
use crate::{DocError, Result};

impl MLine {
    /// The frames of all vertices, when each has one, with one parameter list per element and finite numbers.
    pub fn canonical_frames(&self) -> Result<Vec<&MLineVertexFrame>> {
        let invalid = |what: &str| DocError::Invalid(format!("A canonical multiline needs {what}."));
        let frames: Vec<&MLineVertexFrame> = self.vertices.iter().filter_map(|v| v.frame.as_ref()).collect();
        if frames.len() != self.vertices.len() || frames.is_empty() {
            return Err(invalid("a frame at every vertex"));
        }
        let elements = frames[0].params.len();
        let finite = self.vertices.iter().all(|v| v.p.is_finite())
            && frames.iter().all(|f| f.dir.is_finite() && f.miter.is_finite() && f.params.iter().flatten().all(|x| x.is_finite()));
        if !finite || frames.iter().any(|f| f.params.len() != elements) {
            return Err(invalid("finite numbers and the same elements at every vertex"));
        }
        Ok(frames)
    }

    /// This multiline, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(MLine, Vec3)> {
        self.canonical_frames()?;
        OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical multiline needs a plane with a finite, non-zero normal.".into()))?;
        let sim = Similarity::from_mat4(transform)?;
        let k = sim.scale;
        let mut moved = self.clone();
        for v in &mut moved.vertices {
            v.p = sim.point(v.p);
            if let Some(f) = &mut v.frame {
                f.dir = sim.direction(f.dir);
                f.miter = sim.direction(f.miter);
                f.params.iter_mut().flatten().for_each(|x| *x *= k);
            }
        }
        moved.scale *= k;
        Ok((moved, sim.direction(normal)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MLineJust, MLineStyle, MLineVertex};

    fn mline() -> MLine {
        let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 2.0);
        for (p, dir, miter) in [
            (Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
            (Vec3::new(5.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        ] {
            let mut v = MLineVertex::new(p);
            v.frame = Some(MLineVertexFrame { dir, miter, params: vec![vec![1.0, 5.0], vec![-1.0, 5.0]] });
            m.vertices.push(v);
        }
        m
    }

    fn scale(k: f64) -> Mat4 {
        Mat4 { m: [[k, 0.0, 0.0, 1.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn a_scale_grows_every_distance_and_keeps_the_directions() {
        let (m, n) = mline().transformed_canonical(Vec3::Z, &scale(3.0)).unwrap();
        assert_eq!(m.scale, 6.0);
        assert!(near(m.vertices[1].p, Vec3::new(16.0, 0.0, 3.0)) && near(n, Vec3::Z));
        let f = m.vertices[1].frame.as_ref().unwrap();
        assert!(near(f.dir, Vec3::new(1.0, 0.0, 0.0)) && near(f.miter, Vec3::new(0.0, 1.0, 0.0)));
        assert_eq!(f.params, vec![vec![3.0, 15.0], vec![-3.0, 15.0]]);
    }

    #[test]
    fn a_rotation_about_x_turns_the_vectors_and_the_normal() {
        let (s, c) = std::f64::consts::FRAC_PI_2.sin_cos();
        let rx = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (m, n) = mline().transformed_canonical(Vec3::new(0.0, 0.0, 2.0), &rx).unwrap();
        assert!(near(n, Vec3::new(0.0, -2.0, 0.0)));
        assert!(near(m.vertices[0].frame.as_ref().unwrap().miter, Vec3::new(0.0, 0.0, 1.0)));
        assert!(near(m.vertices[0].p, Vec3::new(0.0, -1.0, 0.0)));
    }

    #[test]
    fn a_missing_frame_a_reflection_or_a_bad_normal_refuses() {
        let mut m = mline();
        m.vertices[1].frame = None;
        assert!(m.transformed_canonical(Vec3::Z, &scale(1.0)).is_err());
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(mline().transformed_canonical(Vec3::Z, &mirror).is_err());
        assert!(mline().transformed_canonical(Vec3::ZERO, &scale(1.0)).is_err());
        let mut ragged = mline();
        ragged.vertices[1].frame.as_mut().unwrap().params.pop();
        assert!(ragged.transformed_canonical(Vec3::Z, &scale(1.0)).is_err());
    }
}

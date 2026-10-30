//! Canonical LEADER geometry.
//!
//! A canonical leader keeps its vertices as WCS points, beside the normal of its plane (`Common.extrusion`, kept with its
//! source magnitude), the horizontal direction, the two offsets and the annotation size (`Leader::frame`) as its record
//! holds them. One owner: the vertices are not mirrored into a second list, and the path type (straight or spline) and
//! the arrowhead flag are the leader's own fields.
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact: the vertices move, the
//! direction turns, and the offsets turn and grow with the scale together with the annotation size. A reflection, a
//! shear and a non-uniform scale refuse, because the arrowhead and the annotation text have no sheared form.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::dim::{Leader, LeaderFrame};
use super::text_canonical::Similarity;
use crate::{DocError, Result};

impl Leader {
    /// The frame of a canonical leader, or an error when the payload is not canonical or holds a non-finite number.
    pub fn canonical_frame(&self) -> Result<&LeaderFrame> {
        let frame = self.frame.as_ref().ok_or_else(|| DocError::Invalid("A canonical leader needs its direction and offsets.".into()))?;
        let finite = self.vertices.iter().all(|v| v.is_finite())
            && frame.direction.is_finite()
            && frame.block_offset.is_finite()
            && frame.annotation_offset.is_finite()
            && frame.text_height.is_finite()
            && frame.text_width.is_finite();
        finite.then_some(frame).ok_or_else(|| DocError::Invalid("A canonical leader needs finite numbers.".into()))
    }

    /// This leader in the plane of `normal` after the similarity `transform`, and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Leader, Vec3)> {
        let frame = self.canonical_frame()?;
        OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical leader needs a plane with a finite, non-zero normal.".into()))?;
        let sim = Similarity::from_mat4(transform)?;
        let k = sim.scale;
        let moved = Leader {
            vertices: self.vertices.iter().map(|v| sim.point(*v)).collect(),
            frame: Some(LeaderFrame {
                direction: sim.direction(frame.direction),
                block_offset: sim.direction(frame.block_offset) * k,
                annotation_offset: sim.direction(frame.annotation_offset) * k,
                text_height: frame.text_height * k,
                text_width: frame.text_width * k,
            }),
            ..self.clone()
        };
        Ok((moved, sim.direction(normal)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leader() -> Leader {
        Leader {
            vertices: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 1.0, 2.0), Vec3::new(5.0, 1.0, 2.0)],
            frame: Some(LeaderFrame {
                direction: Vec3::new(1.0, 0.0, 0.0),
                block_offset: Vec3::new(0.0, 0.5, 0.0),
                annotation_offset: Vec3::new(1.0, 0.0, 0.0),
                text_height: 0.2,
                text_width: 3.0,
            }),
            ..Leader::default()
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn a_rotation_and_scale_move_every_field_together() {
        // A quarter turn about X, a scale of 2 and a translation.
        let m = Mat4 { m: [[2.0, 0.0, 0.0, 1.0], [0.0, 0.0, -2.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let (moved, n) = leader().transformed_canonical(Vec3::new(0.0, 0.0, 3.0), &m).unwrap();
        assert!(near(n, Vec3::new(0.0, -3.0, 0.0)));
        for (a, b) in leader().vertices.iter().zip(&moved.vertices) {
            assert!(near(m.apply(*a), *b));
        }
        let f = moved.frame.unwrap();
        assert!(near(f.direction, Vec3::new(1.0, 0.0, 0.0)));
        assert!(near(f.block_offset, Vec3::new(0.0, 0.0, 1.0)) && near(f.annotation_offset, Vec3::new(2.0, 0.0, 0.0)));
        assert_eq!((f.text_height, f.text_width), (0.4, 6.0));
        assert_eq!((moved.arrow, moved.spline, moved.style.as_str()), (true, false, "Standard"));
    }

    #[test]
    fn a_reflection_a_shear_a_legacy_payload_or_a_bad_normal_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let shear = Mat4 { m: [[1.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(leader().transformed_canonical(Vec3::Z, &mirror).is_err());
        assert!(leader().transformed_canonical(Vec3::Z, &shear).is_err());
        assert!(Leader { frame: None, ..leader() }.transformed_canonical(Vec3::Z, &Mat4::IDENTITY).is_err());
        assert!(leader().transformed_canonical(Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }
}

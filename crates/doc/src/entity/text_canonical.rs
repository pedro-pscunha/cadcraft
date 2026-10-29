//! Canonical text geometry (TEXT, ATTDEF, ATTRIB, MTEXT and a multiline attribute's embedded MTEXT).
//!
//! Canonical text keeps what its source record declares, in the coordinates the record declares:
//! TEXT-family points and the baseline angle are OCS (the plane is the normal of the entity, or of the attribute
//! itself), and an MTEXT's insertion and X-axis direction are WCS with the plane as a normal beside them. The
//! source normal keeps its magnitude. Nothing is projected, mirrored or swapped on import.
//!
//! Edits are exact only for what a similarity does to a plane: a translation, a rotation about any axis and a
//! uniform positive scale. A reflection, a shear and a non-uniform scale have no measured text convention, so
//! they refuse atomically (the text is not flipped, sheared or fitted to a nearby one).

use cadcraft_geom::{Mat4, OcsFrame, Vec2, Vec3};

use super::block::{Attrib, MultilineAttrib};
use super::text::{MText, Text};
use crate::{DocError, Result};

/// A world-space affine map that keeps shapes: rotation, uniform positive scale and translation.
#[derive(Clone, Copy, Debug)]
pub struct Similarity {
    transform: Mat4,
    /// The uniform scale factor.
    pub scale: f64,
}

impl Similarity {
    /// Refuse a map that is not a similarity: perspective, shear, non-uniform scale, reflection, collapse.
    pub fn from_mat4(transform: &Mat4) -> Result<Self> {
        let refuse = || DocError::Invalid("Canonical text can be moved, rotated and scaled uniformly; this transform has another effect.".into());
        let m = &transform.m;
        if m[3] != [0.0, 0.0, 0.0, 1.0] || !m.iter().flatten().all(|v| v.is_finite()) {
            return Err(refuse());
        }
        let column = |c: usize| Vec3::new(m[0][c], m[1][c], m[2][c]);
        let (x, y, z) = (column(0), column(1), column(2));
        let scale = x.len();
        let tolerance = 1e-9 * scale;
        let square = tolerance * scale;
        let equal = |v: Vec3| (v.len() - scale).abs() <= tolerance;
        if !scale.is_finite()
            || scale <= 0.0
            || !equal(y)
            || !equal(z)
            || x.dot(y).abs() > square
            || x.dot(z).abs() > square
            || y.dot(z).abs() > square
        {
            return Err(refuse());
        }
        // A reflection makes the basis left-handed.
        if x.cross(y).dot(z) <= 0.0 {
            return Err(refuse());
        }
        Ok(Self { transform: *transform, scale })
    }

    pub fn point(&self, p: Vec3) -> Vec3 {
        self.transform.apply(p)
    }

    /// A direction turned by the rotation part (the scale is removed).
    pub fn direction(&self, v: Vec3) -> Vec3 {
        let w = self.transform.apply_vec(v);
        Vec3::new(w.x / self.scale, w.y / self.scale, w.z / self.scale)
    }
}

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("Canonical text needs a plane with a finite, non-zero normal.".into()))
}

/// The WCS frame of a text: its insertion point, the directions of its baseline and of its height, and its plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFrame {
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub normal: Vec3,
}

impl Text {
    /// The text in WCS, from its OCS fields and the normal of its plane (the entity's, or the attribute's own).
    pub fn world_frame(&self, normal: Vec3) -> Result<TextFrame> {
        let f = frame(normal)?;
        let (s, c) = self.rotation.sin_cos();
        Ok(TextFrame {
            origin: f.to_world(self.insert),
            x_axis: f.to_world(Vec3::new(c, s, 0.0)),
            y_axis: f.to_world(Vec3::new(-s, c, 0.0)),
            normal: f.to_world(Vec3::Z),
        })
    }

    /// This text, whose OCS fields lie in the plane of `normal`, after the similarity `transform`; and the normal of
    /// its new plane (the rotated normal, with the source magnitude).
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Text, Vec3)> {
        let sim = Similarity::from_mat4(transform)?;
        let (from, new_normal) = (frame(normal)?, sim.direction(normal));
        let to = frame(new_normal)?;
        let place = |p: Vec3| to.to_ocs(sim.point(from.to_world(p)));
        let (s, c) = self.rotation.sin_cos();
        let baseline = to.to_ocs(sim.direction(from.to_world(Vec3::new(c, s, 0.0))));
        let mut text = self.clone();
        text.insert = place(self.insert);
        text.align_pt = self.align_pt.map(place);
        text.rotation = Vec2::new(baseline.x, baseline.y).angle();
        text.height *= sim.scale;
        Ok((text, new_normal))
    }
}

impl MText {
    /// The X-axis direction of the text in WCS: the canonical direction, else the rotation in the plane of `normal`.
    pub fn world_direction(&self, normal: Vec3) -> Result<Vec3> {
        match self.direction {
            Some(d) => Ok(d),
            None => {
                let (s, c) = self.rotation.sin_cos();
                Ok(frame(normal)?.to_world(Vec3::new(c, s, 0.0)))
            }
        }
    }

    /// The text in WCS: its insertion, its X axis, the direction of its height in the plane, and the plane.
    pub fn world_frame(&self, normal: Vec3) -> Result<TextFrame> {
        let f = frame(normal)?;
        let x = self.world_direction(normal)?;
        let n = f.to_world(Vec3::Z);
        Ok(TextFrame { origin: self.insert, x_axis: x, y_axis: n.cross(x), normal: n })
    }

    /// This text (WCS insertion and direction, plane of `normal`) after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(MText, Vec3)> {
        let sim = Similarity::from_mat4(transform)?;
        let direction = sim.direction(self.world_direction(normal)?);
        let new_normal = sim.direction(normal);
        let to = frame(new_normal)?;
        let in_plane = to.to_ocs(direction);
        let mut text = self.clone();
        text.insert = sim.point(self.insert);
        text.direction = Some(direction);
        text.rotation = Vec2::new(in_plane.x, in_plane.y).angle();
        text.height *= sim.scale;
        text.width *= sim.scale;
        if let Some(c) = &mut text.columns {
            c.scale(sim.scale);
        }
        Ok((text, new_normal))
    }
}

impl MultilineAttrib {
    /// The embedded text after the similarity `transform`, in its own plane.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<MultilineAttrib> {
        let (mtext, normal) = self.mtext.transformed_canonical(self.normal, transform)?;
        Ok(MultilineAttrib { normal, mtext, ..self.clone() })
    }
}

impl Attrib {
    /// This attribute after the similarity `transform`: its text in its own plane, the plane (normal and thickness)
    /// and the embedded text in the plane of its own.
    pub fn transformed_canonical(&self, transform: &Mat4) -> Result<Attrib> {
        let sim = Similarity::from_mat4(transform)?;
        let (text, normal) = self.text.transformed_canonical(self.plane.normal, transform)?;
        let mut moved = self.clone();
        moved.text = text;
        moved.plane.normal = normal;
        moved.plane.thickness *= sim.scale;
        if let Some(embedded) = &self.multiline {
            moved.multiline = Some(embedded.transformed_canonical(transform)?);
        }
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HAlign, VAlign};

    fn text(insert: Vec3, rotation: f64) -> Text {
        Text {
            insert,
            align_pt: None,
            height: 2.0,
            value: "T".into(),
            rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Left,
            valign: VAlign::Baseline,
            backward: false,
            upside_down: false,
        }
    }

    fn rotation_z(angle: f64) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4 { m: [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn a_similarity_is_told_from_everything_else() {
        let scaled = |sx: f64, sy: f64, sz: f64| Mat4 { m: [[sx, 0.0, 0.0, 0.0], [0.0, sy, 0.0, 0.0], [0.0, 0.0, sz, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(Similarity::from_mat4(&Mat4::IDENTITY).is_ok());
        assert!(Similarity::from_mat4(&rotation_z(0.7)).is_ok());
        assert!(Similarity::from_mat4(&scaled(2.0, 2.0, 2.0)).is_ok());
        for refused in [scaled(2.0, 1.0, 1.0), scaled(-1.0, 1.0, 1.0), scaled(0.0, 0.0, 0.0)] {
            assert!(Similarity::from_mat4(&refused).is_err(), "{refused:?}");
        }
        let mut shear = Mat4::IDENTITY;
        shear.m[0][1] = 0.5;
        assert!(Similarity::from_mat4(&shear).is_err());
        let mut perspective = Mat4::IDENTITY;
        perspective.m[3][0] = 0.001;
        assert!(Similarity::from_mat4(&perspective).is_err());
    }

    #[test]
    fn a_text_frame_follows_its_plane_and_keeps_the_source_normal_magnitude() {
        // Normal (0, 3, 4): OCS x is Z cross n (arbitrary-axis rule), the scaled normal changes nothing.
        let n = Vec3::new(0.0, 3.0, 4.0);
        let t = text(Vec3::new(1.0, 2.0, 5.0), 0.0);
        let f = t.world_frame(n).unwrap();
        let unit = Vec3::new(0.0, 0.6, 0.8);
        assert!(near(f.normal, unit));
        // Independent basis: x = (-4, 0, 0)/4 = (-1, 0, 0), y = n x x.
        assert!(near(f.x_axis, Vec3::new(-1.0, 0.0, 0.0)));
        assert!(near(f.y_axis, unit.cross(Vec3::new(-1.0, 0.0, 0.0))));
        assert!(near(f.origin, Vec3::new(-1.0, 1.4, 5.2)), "{:?}", f.origin);
    }

    #[test]
    fn a_rotation_about_z_turns_the_baseline_and_keeps_the_plane() {
        let t = text(Vec3::new(10.0, 0.0, 3.0), 0.25);
        let (moved, normal) = t.transformed_canonical(Vec3::Z, &rotation_z(std::f64::consts::FRAC_PI_2)).unwrap();
        assert!(near(normal, Vec3::Z));
        assert!(near(moved.insert, Vec3::new(0.0, 10.0, 3.0)));
        assert!((moved.rotation - (0.25 + std::f64::consts::FRAC_PI_2)).abs() < 1e-12);
        assert_eq!((moved.height, moved.width_factor, moved.oblique), (2.0, 1.0, 0.0));
    }

    #[test]
    fn a_tilted_rotation_turns_the_normal_and_the_text_stays_in_its_plane() {
        // Rotate 90 degrees about X: the +Z plane becomes the -Y plane (normal (0, -1, 0)).
        let about_x = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let t = text(Vec3::new(4.0, 6.0, 1.0), 0.0);
        let before = t.world_frame(Vec3::Z).unwrap();
        let (moved, normal) = t.transformed_canonical(Vec3::Z, &about_x).unwrap();
        assert!(near(normal, Vec3::new(0.0, -1.0, 0.0)));
        let after = moved.world_frame(normal).unwrap();
        // World truth: the rotation applied to the world frame.
        assert!(near(after.origin, about_x.apply(before.origin)));
        assert!(near(after.x_axis, about_x.apply_vec(before.x_axis)));
        assert!(near(after.y_axis, about_x.apply_vec(before.y_axis)));
        assert!(near(after.normal, Vec3::new(0.0, -1.0, 0.0)));
    }

    #[test]
    fn a_uniform_scale_scales_the_height_and_the_thickness_and_a_reflection_refuses() {
        let scale = Mat4 { m: [[3.0, 0.0, 0.0, 1.0], [0.0, 3.0, 0.0, 0.0], [0.0, 0.0, 3.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let mut a = Attrib::new("TAG", text(Vec3::new(1.0, 1.0, 0.0), 0.0));
        a.plane.thickness = 0.5;
        let moved = a.transformed_canonical(&scale).unwrap();
        assert_eq!((moved.text.height, moved.plane.thickness), (6.0, 1.5));
        assert!(near(moved.text.insert, Vec3::new(4.0, 3.0, 0.0)));
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(a.transformed_canonical(&mirror).is_err());
        assert!(text(Vec3::ZERO, 0.0).transformed_canonical(Vec3::Z, &mirror).is_err());
    }

    #[test]
    fn mtext_keeps_its_full_direction_and_its_own_plane() {
        let mut m = MText {
            insert: Vec3::new(1.0, 2.0, 3.0),
            height: 0.5,
            width: 4.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "x".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: None,
        };
        // Without a canonical direction the rotation is read in the plane.
        assert!(near(m.world_direction(Vec3::Z).unwrap(), Vec3::new(1.0, 0.0, 0.0)));
        m.direction = Some(Vec3::new(0.0, 0.6, 0.8));
        let about_z = rotation_z(std::f64::consts::FRAC_PI_2);
        let (moved, normal) = m.transformed_canonical(Vec3::new(1.0, 0.0, 0.0), &about_z).unwrap();
        assert!(near(normal, Vec3::new(0.0, 1.0, 0.0)));
        assert!(near(moved.direction.unwrap(), Vec3::new(-0.6, 0.0, 0.8)));
        assert!(near(moved.insert, Vec3::new(-2.0, 1.0, 3.0)));
        // The new rotation is the direction's angle in the new plane (independent computation).
        let f = OcsFrame::new(normal).unwrap();
        let d = f.to_ocs(moved.direction.unwrap());
        assert!((moved.rotation - d.y.atan2(d.x)).abs() < 1e-12);
    }

    #[test]
    fn an_embedded_text_has_a_plane_of_its_own_and_follows_the_attribute() {
        let mut a = Attrib::new("TAG", text(Vec3::ZERO, 0.0));
        let embedded = MText {
            insert: Vec3::new(2.0, 0.0, 0.0),
            height: 1.0,
            width: 0.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "y".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: Some(Vec3::new(1.0, 0.0, 0.0)),
        };
        a.multiline = Some(MultilineAttrib {
            geometry_revision: crate::GeometryRevision::CanonicalV2,
            normal: Vec3::new(0.0, 0.0, 2.0),
            mtext: embedded,
            read_as: String::new(),
        });
        let moved = a.transformed_canonical(&rotation_z(std::f64::consts::FRAC_PI_2)).unwrap();
        let ml = moved.multiline.unwrap();
        assert!(near(ml.mtext.insert, Vec3::new(0.0, 2.0, 0.0)));
        assert!(near(ml.mtext.direction.unwrap(), Vec3::new(0.0, 1.0, 0.0)));
        assert!(near(ml.normal, Vec3::new(0.0, 0.0, 2.0)), "the source magnitude stays: {:?}", ml.normal);
        assert!(near(moved.plane.normal, Vec3::Z), "the attribute's own plane is separate");
    }
}

//! Canonical DIMENSION geometry.
//!
//! A DIMENSION record mixes coordinate systems, and a canonical dimension keeps each as its record declares it. The
//! definition points `defpt` (10), `p13`, `p14` and `p15` (13, 14, 15) are WCS points. The text midpoint `text_mid`
//! (11) and the arc definition point `p16` (16) are OCS points of the dimension plane, with their Z (the elevation of
//! the plane). The angles (the linear rotation, the text rotation and the oblique angle) are measured in the OCS from its
//! X axis. The plane is the normal of the entity (`Common.extrusion`, kept with its source magnitude). The jog,
//! jogged-radius and break points of `marks` are CADCraft's own and are WCS points.
//!
//! The measurement and the graphics are computed in the plane: [`Dimension::localized`] gives the same dimension with every
//! WCS point in the OCS of its plane, which the 2D layout of the renderer then draws.
//!
//! A similarity (translation, rotation about any axis, uniform positive scale) is exact: the WCS points move, the OCS points
//! are placed in the OCS of the turned plane, the angles turn with the OCS X axis, and the measurement and the break sizes
//! grow with the scale (an angular measurement does not). The sizes of the arrowheads and the text come from the dimension
//! style, as for the legacy transform. A reflection, a shear and a non-uniform scale refuse, because the text and
//! the arrowheads have no sheared form.

use cadcraft_geom::{Mat4, OcsFrame, Vec3, norm_angle};

use super::dim::{DimKind, Dimension};
use super::text_canonical::Similarity;
use crate::{DocError, Result};

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical dimension needs a plane with a finite, non-zero normal.".into()))
}

impl Dimension {
    /// The WCS point of the OCS text midpoint, from the normal of the plane.
    pub fn world_text_mid(&self, normal: Vec3) -> Result<Vec3> {
        Ok(frame(normal)?.to_world(self.text_mid))
    }

    /// This dimension with every WCS point in the OCS of the plane of `normal` (the OCS points stay as they are): the
    /// dimension the 2D layout draws. The Z of a point is its elevation above the plane.
    pub fn localized(&self, normal: Vec3) -> Result<Dimension> {
        let f = frame(normal)?;
        let local = |p: Vec3| f.to_ocs(p);
        let mut d = self.clone();
        (d.defpt, d.p13, d.p14, d.p15) = (local(self.defpt), local(self.p13), local(self.p14), local(self.p15));
        if let Some(j) = &mut d.marks.jog_line {
            *j = local(*j);
        }
        if let Some(j) = &mut d.marks.jogged {
            (j.center, j.jog) = (local(j.center), local(j.jog));
        }
        d.marks.breaks.iter_mut().for_each(|b| b.at = local(b.at));
        Ok(d)
    }

    /// This dimension, in the plane of `normal`, after the similarity `transform`; and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Dimension, Vec3)> {
        let from = frame(normal)?;
        let sim = Similarity::from_mat4(transform)?;
        let new_normal = sim.direction(normal);
        let to = frame(new_normal)?;
        let k = sim.scale;
        let wcs = |p: Vec3| sim.point(p);
        let ocs = |p: Vec3| to.to_ocs(sim.point(from.to_world(p)));
        let turn = to.to_ocs(sim.direction(from.to_world(Vec3::new(1.0, 0.0, 0.0)))).xy().angle();
        let mut d = self.clone();
        (d.defpt, d.p13, d.p14, d.p15) = (wcs(self.defpt), wcs(self.p13), wcs(self.p14), wcs(self.p15));
        (d.text_mid, d.p16) = (ocs(self.text_mid), ocs(self.p16));
        if let DimKind::Linear { rotation } = &mut d.kind {
            *rotation = norm_angle(*rotation + turn);
        }
        d.text_rotation = norm_angle(self.text_rotation + turn);
        if let Some(a) = &mut d.marks.oblique {
            *a = norm_angle(*a + turn);
        }
        if let Some(j) = &mut d.marks.jog_line {
            *j = wcs(*j);
        }
        if let Some(j) = &mut d.marks.jogged {
            (j.center, j.jog) = (wcs(j.center), wcs(j.jog));
        }
        for b in &mut d.marks.breaks {
            (b.at, b.size) = (wcs(b.at), b.size * k);
        }
        if !matches!(self.kind, DimKind::Angular | DimKind::Angular3P) {
            d.measurement *= k;
        }
        d.leader_length *= k;
        // The graphics block is regenerated from the moved definition points.
        d.block = None;
        if !crate::has_finite_numbers(&d) {
            return Err(DocError::Invalid("A canonical dimension cannot be placed with finite coordinates.".into()));
        }
        Ok((d, new_normal))
    }
}

#[cfg(test)]
mod tests {
    use super::super::dim::{DimBreak, JoggedRadius};
    use super::*;

    fn dimension() -> Dimension {
        Dimension {
            kind: DimKind::Linear { rotation: 0.4 },
            defpt: Vec3::new(1.0, 2.0, 3.0),
            text_mid: Vec3::new(0.5, 1.5, 3.0),
            p13: Vec3::new(0.0, 0.0, 3.0),
            p14: Vec3::new(4.0, 0.0, 3.0),
            p15: Vec3::new(2.0, 2.0, 3.0),
            p16: Vec3::new(1.0, 1.0, 3.0),
            text: String::new(),
            style: "Standard".into(),
            measurement: 4.0,
            text_rotation: 0.1,
            user_text_pos: false,
            block: Some("*D1".into()),
            overrides: Default::default(),
            assoc: Vec::new(),
            marks: super::super::dim::DimMarks {
                oblique: Some(0.2),
                jog_line: Some(Vec3::new(2.0, 1.0, 3.0)),
                jogged: Some(JoggedRadius { center: Vec3::new(0.0, 0.0, 3.0), jog: Vec3::new(1.0, 1.0, 3.0) }),
                breaks: vec![DimBreak { at: Vec3::new(3.0, 0.0, 3.0), size: 0.5, auto: false }],
                inspect: None,
            },
            leader_length: 0.0,
        }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    /// A turn about X by 0.7, a scale of 2 and a translation.
    fn motion() -> Mat4 {
        let (s, c) = 0.7f64.sin_cos();
        Mat4 { m: [[2.0, 0.0, 0.0, 4.0], [0.0, 2.0 * c, -2.0 * s, -2.0], [0.0, 2.0 * s, 2.0 * c, 1.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    #[test]
    fn localizing_puts_the_wcs_points_in_the_plane_and_leaves_the_ocs_points() {
        let normal = Vec3::new(0.0, 3.0, 4.0);
        let d = dimension();
        let local = d.localized(normal).unwrap();
        let f = OcsFrame::new(normal).unwrap();
        assert!(near(f.to_world(local.defpt), d.defpt) && near(f.to_world(local.p13), d.p13) && near(f.to_world(local.p15), d.p15));
        assert_eq!((local.text_mid, local.p16), (d.text_mid, d.p16));
        assert!(near(f.to_world(local.marks.breaks[0].at), d.marks.breaks[0].at));
        assert!(near(d.world_text_mid(normal).unwrap(), f.to_world(d.text_mid)));
        assert!(d.localized(Vec3::ZERO).is_err());
    }

    #[test]
    fn a_similarity_moves_wcs_points_and_places_ocs_points_in_the_turned_plane() {
        let (m, normal) = (motion(), Vec3::new(0.0, 0.0, 2.0));
        let (moved, n) = dimension().transformed_canonical(normal, &m).unwrap();
        let (old, new) = (OcsFrame::new(normal).unwrap(), OcsFrame::new(n).unwrap());
        assert!(near(moved.defpt, m.apply(dimension().defpt)) && near(moved.p14, m.apply(dimension().p14)));
        // The OCS points are the same world points as before, moved.
        assert!(near(new.to_world(moved.text_mid), m.apply(old.to_world(dimension().text_mid))));
        assert!(near(new.to_world(moved.p16), m.apply(old.to_world(dimension().p16))));
        assert!(near(n, Vec3::new(0.0, -2.0 * 0.7f64.sin(), 2.0 * 0.7f64.cos())), "{n:?}");
        // A direction in the plane at an OCS angle a points the same way after the motion.
        let direction = |f: &OcsFrame, a: f64| f.to_world(Vec3::new(a.cos(), a.sin(), 0.0));
        let DimKind::Linear { rotation } = moved.kind else { panic!("not linear") };
        let want = m.apply_vec(direction(&old, 0.4)).normalized();
        assert!(near(direction(&new, rotation), want), "the linear rotation turns with the plane");
        assert!(near(direction(&new, moved.text_rotation), m.apply_vec(direction(&old, 0.1)).normalized()));
        assert!(near(direction(&new, moved.marks.oblique.unwrap()), m.apply_vec(direction(&old, 0.2)).normalized()));
        assert_eq!((moved.measurement, moved.marks.breaks[0].size, moved.block), (8.0, 1.0, None));
        assert!(near(moved.marks.jog_line.unwrap(), m.apply(Vec3::new(2.0, 1.0, 3.0))));
    }

    #[test]
    fn an_angular_measurement_does_not_scale() {
        let mut d = dimension();
        d.kind = DimKind::Angular;
        d.measurement = 0.5;
        assert_eq!(d.transformed_canonical(Vec3::Z, &motion()).unwrap().0.measurement, 0.5);
    }

    #[test]
    fn a_reflection_a_shear_a_non_uniform_scale_or_a_bad_normal_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let shear = Mat4 { m: [[1.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let squash = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        for bad in [mirror, shear, squash] {
            assert!(dimension().transformed_canonical(Vec3::Z, &bad).is_err());
        }
        assert!(dimension().transformed_canonical(Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }
}

//! Canonical TOLERANCE and TABLE geometry.
//!
//! Both keep their insertion point and horizontal direction in WCS, as their DXF records do, with the plane as a
//! normal beside them (`Common.extrusion`, kept with its source magnitude). The direction must lie in the plane.
//! The vertical axis of the frame is `normal x direction`: a table grows along `-y` from its insertion point.
//!
//! Edits are exact for what a similarity does to a plane: translation, rotation about any axis and uniform positive
//! scale. A reflection, shear or non-uniform scale refuses atomically, as for canonical text. A tolerance frame takes
//! its size from its dimension style, so a scale moves its insertion point and leaves the frame size alone, as the
//! legacy transform does. A table scales its column widths, row heights, text heights and break distances.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::layout::Table;
use super::text_canonical::{Similarity, TextFrame};
use crate::{DocError, Result, Tolerance};

/// The WCS frame of a flat annotation whose horizontal direction is `dir` in the plane of `normal`.
fn frame(origin: Vec3, dir: Vec3, normal: Vec3) -> Result<TextFrame> {
    let invalid = || DocError::Invalid("A canonical tolerance or table needs a finite direction in the plane of a finite, non-zero normal.".into());
    let plane = OcsFrame::new(normal).ok_or_else(invalid)?;
    let n = plane.to_world(Vec3::Z);
    let length = dir.len();
    if !dir.is_finite() || !length.is_finite() || length <= 0.0 || dir.dot(n).abs() > 1e-9 * length {
        return Err(invalid());
    }
    let x = dir.normalized();
    Ok(TextFrame { origin, x_axis: x, y_axis: n.cross(x), normal: n })
}

impl Tolerance {
    /// The frame in WCS, from its insertion point, its direction and the normal of its plane.
    pub fn world_frame(&self, normal: Vec3) -> Result<TextFrame> {
        frame(self.insert, self.dir, normal)
    }

    /// This frame after the similarity `transform`, and the new normal (the rotated one, with the source magnitude).
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Tolerance, Vec3)> {
        self.world_frame(normal)?;
        let sim = Similarity::from_mat4(transform)?;
        let moved = Tolerance { insert: sim.point(self.insert), dir: sim.direction(self.dir), ..self.clone() };
        Ok((moved, sim.direction(normal)))
    }
}

impl Table {
    /// The frame in WCS: the insertion point is its top-left corner and rows grow along the negative vertical axis.
    pub fn world_frame(&self, normal: Vec3) -> Result<TextFrame> {
        frame(self.insert, self.dir, normal)
    }

    /// This table after the similarity `transform`, and the new normal.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Table, Vec3)> {
        self.world_frame(normal)?;
        let sim = Similarity::from_mat4(transform)?;
        let s = sim.scale;
        let mut table = self.clone();
        table.insert = sim.point(self.insert);
        table.dir = sim.direction(self.dir);
        table.col_widths.iter_mut().for_each(|w| *w *= s);
        table.row_heights.iter_mut().for_each(|h| *h *= s);
        table.text_height *= s;
        for cell in table.cells.iter_mut().flatten() {
            if let Some(h) = &mut cell.text_height {
                *h *= s;
            }
        }
        if let Some(b) = &mut table.breaks {
            b.height *= s;
            b.spacing *= s;
            b.heights.iter_mut().for_each(|h| *h *= s);
            // The offsets are along the table's own axes: a similarity scales them and keeps their axes.
            b.offsets.iter_mut().for_each(|o| *o = *o * s);
        }
        Ok((table, sim.direction(normal)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotation_x(angle: f64) -> Mat4 {
        let (s, c) = angle.sin_cos();
        Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn scale(k: f64) -> Mat4 {
        Mat4 { m: [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn tolerance() -> Tolerance {
        Tolerance { insert: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.6, 0.8), style: "Standard".into(), text: "{x}".into() }
    }

    fn table() -> Table {
        Table {
            insert: Vec3::new(1.0, 2.0, 3.0),
            dir: Vec3::new(0.0, 0.6, 0.8),
            col_widths: vec![2.0, 3.0],
            row_heights: vec![1.0],
            text_height: 0.5,
            ..Table::default()
        }
    }

    const TILTED: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };

    #[test]
    fn the_frame_lies_in_the_plane_and_grows_down_the_vertical_axis() {
        let f = tolerance().world_frame(TILTED).unwrap();
        assert!(near(f.x_axis, Vec3::new(0.0, 0.6, 0.8)) && near(f.normal, Vec3::new(1.0, 0.0, 0.0)));
        assert!(near(f.y_axis, f.normal.cross(f.x_axis)));
        // A source magnitude of the normal only scales the stored vector.
        assert!(near(table().world_frame(Vec3::new(4.0, 0.0, 0.0)).unwrap().normal, f.normal));
    }

    #[test]
    fn a_direction_outside_the_plane_or_a_bad_normal_has_no_frame() {
        let mut t = tolerance();
        t.dir = Vec3::new(1.0, 0.0, 0.0);
        assert!(t.world_frame(TILTED).is_err());
        assert!(tolerance().world_frame(Vec3::ZERO).is_err());
        assert!(tolerance().world_frame(Vec3::new(f64::NAN, 0.0, 1.0)).is_err());
    }

    #[test]
    fn a_rotation_turns_the_frame_and_keeps_the_normals_magnitude() {
        let (t, normal) = tolerance().transformed_canonical(Vec3::new(3.0, 0.0, 0.0), &rotation_x(std::f64::consts::FRAC_PI_2)).unwrap();
        assert!(near(normal, Vec3::new(3.0, 0.0, 0.0)));
        assert!(near(t.dir, Vec3::new(0.0, -0.8, 0.6)) && near(t.insert, Vec3::new(1.0, -3.0, 2.0)), "{t:?}");
        assert!(t.world_frame(normal).is_ok());
    }

    #[test]
    fn a_scale_moves_a_tolerance_and_resizes_a_table() {
        let (t, _) = tolerance().transformed_canonical(TILTED, &scale(2.0)).unwrap();
        assert!(near(t.insert, Vec3::new(2.0, 4.0, 6.0)) && near(t.dir, Vec3::new(0.0, 0.6, 0.8)));
        let (tb, _) = table().transformed_canonical(TILTED, &scale(2.0)).unwrap();
        assert_eq!((tb.col_widths, tb.row_heights, tb.text_height), (vec![4.0, 6.0], vec![2.0], 1.0));
        assert!(near(tb.insert, Vec3::new(2.0, 4.0, 6.0)));
    }

    #[test]
    fn a_reflection_or_a_shear_refuses() {
        let mirror = Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let shear = Mat4 { m: [[1.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(tolerance().transformed_canonical(TILTED, &mirror).is_err());
        assert!(table().transformed_canonical(TILTED, &shear).is_err());
    }
}

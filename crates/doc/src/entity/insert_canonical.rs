//! Canonical INSERT placement. Matrices stay spatial until the completed parent reaches a view.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::{Handle, Insert};
use crate::{DocError, Result, has_finite_numbers};

impl Insert {
    /// The complete block-to-WCS matrix for one cell, with a WCS block base and an OCS insertion point.
    ///
    /// Column-vector product: OCS * T(insert) * Rz(rotation) * T(cell spacing) * S(scale) * T(-base).
    /// Cell spacing is outside the signed scale. Zero scales remain valid for display.
    pub fn world_transform(&self, normal: Vec3, base: Vec3, column: u32, row: u32) -> Result<Mat4> {
        if column >= self.cols || row >= self.rows {
            return Err(DocError::Invalid("The INSERT cell is outside its row or column count.".into()));
        }
        if !self.insert.is_finite()
            || !self.scale.is_finite()
            || !base.is_finite()
            || ![self.rotation, self.col_spacing, self.row_spacing].iter().all(|v| v.is_finite())
        {
            return Err(DocError::NonFinite(Handle(0)));
        }
        let frame = OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical INSERT needs a finite, non-zero normal.".into()))?;
        let (s, c) = self.rotation.sin_cos();
        let x_axis = frame.to_world(Vec3::new(c, s, 0.0)) * self.scale.x;
        let y_axis = frame.to_world(Vec3::new(-s, c, 0.0)) * self.scale.y;
        let z_axis = frame.to_world(Vec3::Z) * self.scale.z;
        let dx = self.col_spacing * f64::from(column);
        let dy = self.row_spacing * f64::from(row);
        let offset = Vec3::new(c * dx - s * dy, s * dx + c * dy, 0.0);
        let origin = frame.to_world(self.insert + offset) - x_axis * base.x - y_axis * base.y - z_axis * base.z;
        let matrix = Mat4 {
            m: [
                [x_axis.x, y_axis.x, z_axis.x, origin.x],
                [x_axis.y, y_axis.y, z_axis.y, origin.y],
                [x_axis.z, y_axis.z, z_axis.z, origin.z],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        if !has_finite_numbers(&matrix) {
            return Err(DocError::NonFinite(Handle(0)));
        }
        Ok(matrix)
    }
}

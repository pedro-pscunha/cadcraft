//! Canonical INSERT placement. Matrices stay spatial until the completed parent reaches a view.

use cadcraft_geom::{Mat4, OcsFrame, Vec3};

use super::{GeometryRevision, Handle, Insert};
use crate::{DocError, Result, has_finite_numbers};

fn unrepresentable_axes() -> DocError {
    DocError::Invalid("The transformed INSERT source axes cannot be represented by an orthogonal block reference.".into())
}

impl Insert {
    /// Persist an affine edit when the source axes remain nonzero and perpendicular. Display can retain a shear,
    /// but an INSERT record cannot. Attributes must independently support the same edit in their own planes.
    pub fn transformed_canonical(&self, normal: Vec3, transform: &Mat4) -> Result<(Insert, Vec3)> {
        if *transform == Mat4::IDENTITY {
            return Ok((self.clone(), normal));
        }
        if !has_finite_numbers(self) || !has_finite_numbers(transform) {
            return Err(DocError::NonFinite(Handle(0)));
        }
        if transform.m[3] != [0.0, 0.0, 0.0, 1.0] {
            return Err(DocError::Invalid("A canonical INSERT needs an affine edit.".into()));
        }
        self.world_transform(normal, Vec3::ZERO, 0, 0)?;
        let frame = OcsFrame::new(normal).ok_or_else(unrepresentable_axes)?;
        let mut moved = self.clone();
        moved.attribs = self
            .attribs
            .iter()
            .map(|attribute| {
                if attribute.geometry_revision != GeometryRevision::CanonicalV2
                    || attribute.multiline.as_ref().is_some_and(|text| text.geometry_revision != GeometryRevision::CanonicalV2)
                {
                    return Err(DocError::Invalid("A canonical INSERT edit cannot transform a legacy attribute payload.".into()));
                }
                attribute.transformed_canonical(transform)
            })
            .collect::<Result<Vec<_>>>()?;
        let translation = transform.m[0][..3] == [1.0, 0.0, 0.0] && transform.m[1][..3] == [0.0, 1.0, 0.0] && transform.m[2][..3] == [0.0, 0.0, 1.0];
        let new_normal = if translation {
            moved.insert = moved.insert + frame.to_ocs(transform.apply(Vec3::ZERO));
            normal
        } else {
            let (s, c) = self.rotation.sin_cos();
            let x = transform.apply_vec(frame.to_world(Vec3::new(c, s, 0.0)));
            let y = transform.apply_vec(frame.to_world(Vec3::new(-s, c, 0.0)));
            let z = transform.apply_vec(frame.to_world(Vec3::Z));
            let u = x.try_normalized().ok_or_else(unrepresentable_axes)?;
            let v = y.try_normalized().ok_or_else(unrepresentable_axes)?;
            let w = z.try_normalized().ok_or_else(unrepresentable_axes)?;
            let roundoff = 64.0 * f64::EPSILON;
            if u.dot(v).abs() > roundoff || u.dot(w).abs() > roundoff || v.dot(w).abs() > roundoff {
                return Err(unrepresentable_axes());
            }
            let unit = u.cross(v).try_normalized().ok_or_else(unrepresentable_axes)?;
            let new_normal = unit * normal.len();
            let target = OcsFrame::new(new_normal).ok_or_else(unrepresentable_axes)?;
            moved.insert = target.to_ocs(transform.apply(frame.to_world(self.insert)));
            moved.rotation = target.to_ocs(u).xy().angle();
            moved.scale = Vec3::new(self.scale.x * x.len(), self.scale.y * y.len(), self.scale.z * z.dot(unit));
            moved.col_spacing *= x.len();
            moved.row_spacing *= y.len();
            new_normal
        };
        if !has_finite_numbers(&moved) || !new_normal.is_finite() {
            return Err(DocError::NonFinite(Handle(0)));
        }
        Ok((moved, new_normal))
    }

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

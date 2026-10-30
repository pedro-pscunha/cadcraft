use crate::{Mat4, Vec3};

/// A computational DXF arbitrary-axis frame. The source normal remains drawing data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OcsFrame {
    x: Vec3,
    y: Vec3,
    z: Vec3,
}

impl OcsFrame {
    /// Refuse zero or non-finite normals without a drawing-unit tolerance.
    pub fn new(normal: Vec3) -> Option<Self> {
        let z = normal.try_normalized()?;
        let x = if z.x.abs() < 1.0 / 64.0 && z.y.abs() < 1.0 / 64.0 { Vec3::new(0.0, 1.0, 0.0).cross(z) } else { Vec3::Z.cross(z) }.normalized();
        let y = z.cross(x).normalized();
        Some(Self { x, y, z })
    }

    /// Convert an OCS point or vector to WCS. The frame has no translation.
    pub fn to_world(self, value: Vec3) -> Vec3 {
        self.x * value.x + self.y * value.y + self.z * value.z
    }

    /// Convert a WCS point or vector to OCS with the inverse orthonormal basis.
    pub fn to_ocs(self, value: Vec3) -> Vec3 {
        Vec3::new(value.dot(self.x), value.dot(self.y), value.dot(self.z))
    }

    pub fn matrix(self) -> Mat4 {
        Mat4 {
            m: [[self.x.x, self.y.x, self.z.x, 0.0], [self.x.y, self.y.y, self.z.y, 0.0], [self.x.z, self.y.z, self.z.z, 0.0], [0.0, 0.0, 0.0, 1.0]],
        }
    }
}

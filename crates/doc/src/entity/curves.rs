use super::{Deserialize, PolyVertex, Serialize, Vec3};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub a: Vec3,
    pub b: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub p: Vec3,
    #[serde(default)]
    pub angle: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub center: Vec3,
    pub radius: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    pub center: Vec3,
    pub radius: f64,
    /// Radians, CCW.
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ellipse {
    pub center: Vec3,
    pub major: Vec3,
    pub ratio: f64,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LwPolyline {
    pub vertices: Vec<PolyVertex>,
    pub closed: bool,
    #[serde(default)]
    pub const_width: f64,
    #[serde(default)]
    pub elevation: f64,
    /// Polyline linetype generation (PLINEGEN).
    #[serde(default)]
    pub plinegen: bool,
    /// Keep the POLYLINE/VERTEX representation of a source 2D polyline.
    #[serde(default)]
    pub heavy: bool,
    /// Source fit data, used only while the displayed geometry is unchanged.
    #[serde(default)]
    pub fit: Option<PolylineFit>,
}

impl LwPolyline {
    pub fn saved_fit(&self) -> Option<&PolylineFit> {
        self.fit.as_ref().filter(|fit| {
            self.vertices == fit.display_vertices
                && self.closed == (fit.flags & 1 != 0)
                && self.plinegen == (fit.flags & 128 != 0)
                && self.elevation == fit.elevation
                && self.const_width == 0.0
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolylineFit {
    pub flags: i64,
    pub smooth_type: i64,
    pub m_count: i64,
    pub n_count: i64,
    pub m_density: i64,
    pub n_density: i64,
    pub start_width: f64,
    pub end_width: f64,
    pub elevation: f64,
    pub extrusion: Vec3,
    pub vertices: Vec<PolylineFitVertex>,
    pub display_vertices: Vec<PolyVertex>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolylineFitVertex {
    pub position: Vec3,
    pub flags: i64,
    pub start_width: f64,
    pub end_width: f64,
    pub bulge: f64,
    /// Curve-fit tangent direction in radians.
    pub tangent: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3d {
    pub points: Vec<Vec3>,
    pub closed: bool,
}

/// SOLID / TRACE: up to four corners (DXF order 1,2,4,3 when drawn).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Solid {
    pub corners: [Vec3; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face3d {
    pub corners: [Vec3; 4],
    #[serde(default)]
    pub hidden_edges: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RayLine {
    pub base: Vec3,
    pub dir: Vec3,
}

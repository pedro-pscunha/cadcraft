use super::{Deserialize, Mat3, PolyVertex, Serialize, Vec2, Vec3};

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
    #[serde(default)]
    pub records: PolylineRecords,
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
    /// Fit data and its display snapshot, kept together by whole-polyline edits.
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

    pub fn transform(&mut self, m: &Mat3) {
        if self.saved_fit().is_none() {
            self.fit = None;
        }
        let scale = m.scale_factor();
        let direction = if m.is_mirroring() { -1.0 } else { 1.0 };
        for v in &mut self.vertices {
            v.p = m.apply(v.p);
            v.start_width *= scale;
            v.end_width *= scale;
            v.bulge *= direction;
        }
        self.const_width *= scale;
        if let Some(fit) = &mut self.fit {
            let ocs_x = if fit.extrusion.z < 0.0 { -1.0 } else { 1.0 };
            for v in &mut fit.vertices {
                let p = m.apply(Vec2::new(ocs_x * v.position.x, v.position.y));
                v.position.x = ocs_x * p.x;
                v.position.y = p.y;
                v.start_width *= scale;
                v.end_width *= scale;
                v.bulge *= direction;
                if v.flags & 2 != 0 {
                    let tangent = m.apply_vec(Vec2::new(ocs_x * v.tangent.cos(), v.tangent.sin()));
                    v.tangent = cadcraft_geom::norm_angle(tangent.y.atan2(ocs_x * tangent.x));
                }
            }
            fit.start_width *= scale;
            fit.end_width *= scale;
            fit.display_vertices.clone_from(&self.vertices);
        }
    }

    pub fn set_width(&mut self, width: f64) {
        if self.saved_fit().is_none() {
            self.fit = None;
        }
        self.const_width = if self.fit.is_some() { 0.0 } else { width };
        let vertex_width = if self.fit.is_some() { width } else { 0.0 };
        for v in &mut self.vertices {
            v.start_width = vertex_width;
            v.end_width = vertex_width;
        }
        if let Some(fit) = &mut self.fit {
            fit.start_width = width;
            fit.end_width = width;
            for v in &mut fit.vertices {
                v.start_width = width;
                v.end_width = width;
            }
            fit.display_vertices.clone_from(&self.vertices);
        }
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

impl PolylineFit {
    pub fn reversed(&self, display_vertices: Vec<PolyVertex>) -> Self {
        let mut out = self.clone();
        // Spline control vertices and generated vertices form separate sequences.
        for control in [false, true] {
            let sequence: Vec<_> = self.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).collect();
            let n = sequence.len();
            for (i, v) in out.vertices.iter_mut().filter(|v| (v.flags & 16 != 0) == control).enumerate() {
                let Some(old) = sequence.get(n - 1 - i) else { continue };
                *v = (*old).clone();
                if v.flags & 2 != 0 {
                    v.tangent = cadcraft_geom::norm_angle(v.tangent + std::f64::consts::PI);
                }
                if self.flags & 1 != 0 || i + 1 < n {
                    if let Some(segment) = sequence.get((2 * n - 2 - i) % n) {
                        v.bulge = -segment.bulge;
                        v.start_width = segment.end_width;
                        v.end_width = segment.start_width;
                    }
                } else {
                    v.bulge = 0.0;
                    v.start_width = 0.0;
                    v.end_width = 0.0;
                }
            }
        }
        std::mem::swap(&mut out.start_width, &mut out.end_width);
        out.display_vertices = display_vertices;
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolylineFitVertex {
    #[serde(default)]
    pub record: PolylineRecord,
    pub position: Vec3,
    pub flags: i64,
    pub start_width: f64,
    pub end_width: f64,
    pub bulge: f64,
    /// Curve-fit tangent direction in radians.
    pub tangent: f64,
}

/// The persistent identity and common properties of a POLYLINE child record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolylineRecord {
    pub handle: Option<super::Handle>,
    pub common: super::Common,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolylineRecords {
    pub vertices: Vec<PolylineRecord>,
    pub seqend: Option<PolylineRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3d {
    #[serde(default)]
    pub records: PolylineRecords,
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

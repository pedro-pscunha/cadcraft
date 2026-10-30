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
    #[serde(default = "ellipse_normal")]
    pub normal: Vec3,
    pub ratio: f64,
    pub start: f64,
    pub end: f64,
}

fn ellipse_normal() -> Vec3 {
    Vec3::Z
}

impl Ellipse {
    /// The source plane projected into the drawing's XY display plane.
    pub fn projected(&self) -> cadcraft_geom::Ellipse {
        let minor = self.normal.normalized().cross(self.major).normalized() * (self.major.len() * self.ratio);
        cadcraft_geom::Ellipse::from_conjugate(self.center.xy(), self.major.xy(), minor.xy(), self.start, self.end)
    }

    /// The existing 2D edit representation. A line-shaped projection has no ellipse representation.
    pub fn flatten(&mut self) -> bool {
        if !self.center.is_finite()
            || !self.major.is_finite()
            || !self.normal.is_finite()
            || self.major.len() <= cadcraft_geom::EPS
            || self.normal.len() <= cadcraft_geom::EPS
            || !self.ratio.is_finite()
            || self.ratio <= 0.0
            || self.ratio > 1.0
            || !self.start.is_finite()
            || !self.end.is_finite()
        {
            return false;
        }
        if self.normal == Vec3::Z && self.major.z == 0.0 {
            return true;
        }
        let p = self.projected();
        if !p.major.is_finite() || p.major.len() <= cadcraft_geom::EPS || !p.ratio.is_finite() || p.ratio <= cadcraft_geom::EPS {
            return false;
        }
        self.major = p.major.to3(0.0);
        self.normal = Vec3::Z;
        self.ratio = p.ratio;
        self.start = p.start;
        self.end = p.end;
        true
    }
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

    /// The sign of X between the vertices of `vertices` and the OCS of the fit records: a legacy polyline in a plane facing down is
    /// mirrored (-1), a canonical one is not. It is read off the first generated vertex that is not on the Y axis.
    fn ocs_x(&self) -> f64 {
        let Some(fit) = &self.fit else { return 1.0 };
        let generated = fit.vertices.iter().filter(|v| v.flags & 16 == 0);
        let shown = generated.zip(&self.vertices).find(|(v, q)| v.position.x.abs() > 1e-12 && q.p.x.abs() > 1e-12);
        shown.map_or(1.0, |(v, q)| if v.position.x * q.p.x < 0.0 { -1.0 } else { 1.0 })
    }

    /// The frame control points of a spline-fit polyline whose curve can be rebuilt (curve type quadratic or cubic B-spline), in
    /// the coordinates of `vertices`, in record order. `None`
    /// for any other polyline: those are edited by their drawn vertices.
    pub fn spline_frame(&self) -> Option<Vec<Vec2>> {
        let ocs_x = self.ocs_x();
        let fit = self.saved_fit()?;
        spline_degree(fit.flags, fit.smooth_type)?;
        let frame: Vec<Vec2> = fit.vertices.iter().filter(|v| v.flags & 16 != 0).map(|v| Vec2::new(ocs_x * v.position.x, v.position.y)).collect();
        (frame.len() >= 3).then_some(frame)
    }

    /// Move frame control point `k` to `to` and refit the curve from the edited frame (`segs` segments per span, at most `max`
    /// points): the polyline stays spline-fit, with its flags, curve type and frame count, and new generated vertices. `None`
    /// when this is not a rebuildable spline-fit polyline or `k` is out of range.
    pub fn drag_frame(&mut self, k: usize, to: Vec2, segs: usize, max: usize) -> Option<()> {
        let ocs_x = self.ocs_x();
        let mut frame = self.spline_frame()?;
        *frame.get_mut(k)? = to;
        let fit = self.fit.as_mut()?;
        let degree = spline_degree(fit.flags, fit.smooth_type)?;
        let curve = cadcraft_geom::frame_curve(&frame.iter().map(|p| p.to3(0.0)).collect::<Vec<_>>(), self.closed, degree, segs, max);
        let template = fit.vertices.iter().find(|v| v.flags & 16 == 0).cloned();
        let curve_z = template.as_ref().map_or(0.0, |t| t.position.z);
        let mut frame_vertices: Vec<PolylineFitVertex> = fit.vertices.iter().filter(|v| v.flags & 16 != 0).cloned().collect();
        for (v, p) in frame_vertices.iter_mut().zip(&frame) {
            v.position = Vec3::new(ocs_x * p.x, p.y, v.position.z);
        }
        let old_curve: Vec<PolylineFitVertex> = fit.vertices.iter().filter(|v| v.flags & 16 == 0).cloned().collect();
        let blank = PolylineFitVertex {
            record: PolylineRecord::default(),
            position: Vec3::ZERO,
            flags: 8,
            start_width: fit.start_width,
            end_width: fit.end_width,
            bulge: 0.0,
            tangent: 0.0,
        };
        let curve_vertices: Vec<PolylineFitVertex> = curve
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let base = old_curve.get(i).or(template.as_ref()).cloned().unwrap_or_else(|| blank.clone());
                let record = old_curve.get(i).map(|v| v.record.clone()).unwrap_or_default();
                PolylineFitVertex { record, position: Vec3::new(ocs_x * p.x, p.y, curve_z), bulge: 0.0, tangent: 0.0, ..base }
            })
            .collect();
        self.vertices = curve_vertices
            .iter()
            .map(|v| PolyVertex { p: Vec2::new(ocs_x * v.position.x, v.position.y), bulge: 0.0, start_width: v.start_width, end_width: v.end_width })
            .collect();
        self.records.vertices.resize(self.vertices.len(), PolylineRecord::default());
        fit.vertices = frame_vertices.into_iter().chain(curve_vertices).collect();
        fit.display_vertices.clone_from(&self.vertices);
        Some(())
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
    /// The vertices that are drawn: for a spline-fit polyline, the points of the generated curve (the frame control points
    /// are in `fit`).
    pub points: Vec<Vec3>,
    pub closed: bool,
    /// Curve-fit or spline-fit data and its display snapshot, kept together by whole-polyline edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<Polyline3dFit>,
}

/// The fit data of a 3D polyline as its record holds it: the polyline flags (group 70: 1 closed, 2 curve-fit vertices added,
/// 4 spline-fit vertices added, 8 3D polyline, 128 linetype generation), the curve type (75: 0 none, 5 quadratic B-spline,
/// 6 cubic B-spline, 8 Bézier), and every vertex with its flags (group 70: 1 added by curve fitting, 2 curve-fit tangent
/// defined, 8 vertex created by spline fitting, 16 spline frame control point, 32 3D polyline vertex), so the frame control
/// points and the generated curve both survive a save.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3dFit {
    pub flags: i64,
    pub smooth_type: i64,
    pub vertices: Vec<PolylineFitVertex>,
    /// The polyline's `points` when the fit data was made: the fit data describes the polyline only while they are the same.
    pub display_points: Vec<Vec3>,
}

/// The B-spline degree of a spline-fit polyline whose curve CADCraft rebuilds: header bit 4 (spline-fit vertices added) with
/// curve type 5 (quadratic) or 6 (cubic). Curve-fit polylines and Bézier curves are not rebuilt.
fn spline_degree(flags: i64, smooth_type: i64) -> Option<usize> {
    if flags & 4 == 0 {
        return None;
    }
    match smooth_type {
        5 => Some(2),
        6 => Some(3),
        _ => None,
    }
}

impl Polyline3d {
    /// The grip points: the frame control points of a spline-fit polyline whose curve can be rebuilt, else the drawn points.
    pub fn grip_points(&self) -> Vec<Vec2> {
        match self.spline_frame() {
            Some(frame) => frame.iter().map(|p| p.xy()).collect(),
            None => self.points.iter().map(|p| p.xy()).collect(),
        }
    }

    /// The frame control points of a spline-fit polyline whose curve can be rebuilt (see [`LwPolyline::spline_frame`]).
    pub fn spline_frame(&self) -> Option<Vec<Vec3>> {
        let fit = self.saved_fit()?;
        spline_degree(fit.flags, fit.smooth_type)?;
        let frame: Vec<Vec3> = fit.vertices.iter().filter(|v| v.flags & 16 != 0).map(|v| v.position).collect();
        (frame.len() >= 3).then_some(frame)
    }

    /// Drag grip `index` to `to` in the XY plane. A spline-fit polyline moves a frame control point and refits the curve
    /// (`segs` segments per span, at most `max` points), staying spline-fit with its flags, curve type and frame count; any other
    /// polyline moves a drawn point. `None` when `index` is out of range.
    pub fn drag_grip(&mut self, index: usize, to: Vec2, segs: usize, max: usize) -> Option<()> {
        let Some(mut frame) = self.spline_frame() else {
            let p = self.points.get_mut(index)?;
            (p.x, p.y) = (to.x, to.y);
            return Some(());
        };
        let moved = frame.get_mut(index)?;
        (moved.x, moved.y) = (to.x, to.y);
        let fit = self.fit.as_mut()?;
        let degree = spline_degree(fit.flags, fit.smooth_type)?;
        let curve = cadcraft_geom::frame_curve(&frame, self.closed, degree, segs, max);
        let old_curve: Vec<PolylineFitVertex> = fit.vertices.iter().filter(|v| v.flags & 16 == 0).cloned().collect();
        let blank = PolylineFitVertex {
            record: PolylineRecord::default(),
            position: Vec3::ZERO,
            flags: 8 | 32,
            start_width: 0.0,
            end_width: 0.0,
            bulge: 0.0,
            tangent: 0.0,
        };
        let mut vertices: Vec<PolylineFitVertex> = fit.vertices.iter().filter(|v| v.flags & 16 != 0).cloned().collect();
        for (v, p) in vertices.iter_mut().zip(&frame) {
            v.position = *p;
        }
        vertices.extend(curve.iter().enumerate().map(|(i, p)| {
            let base = old_curve.get(i).or(old_curve.first()).cloned().unwrap_or_else(|| blank.clone());
            PolylineFitVertex { record: old_curve.get(i).map(|v| v.record.clone()).unwrap_or_default(), position: *p, ..base }
        }));
        fit.vertices = vertices;
        self.points = curve;
        self.records.vertices.resize(self.points.len(), PolylineRecord::default());
        fit.display_points.clone_from(&self.points);
        Some(())
    }

    /// The fit data, while it still describes the polyline (its points and closed state are the ones it was made with).
    pub fn saved_fit(&self) -> Option<&Polyline3dFit> {
        self.fit.as_ref().filter(|fit| self.points == fit.display_points && self.closed == (fit.flags & 1 != 0))
    }

    /// This polyline in the other direction. Fit data that still describes it is reversed too: the frame control points and the
    /// generated curve are two sequences, each reversed, so it stays spline-fit; fit data that no longer describes it is dropped.
    pub fn reversed(&self) -> Polyline3d {
        let mut out = self.clone();
        out.points.reverse();
        out.records.vertices.reverse();
        out.fit = self.saved_fit().map(|fit| {
            let mut reversed = fit.clone();
            for control in [false, true] {
                let sequence: Vec<&PolylineFitVertex> = fit.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).collect();
                let n = sequence.len();
                for (i, v) in reversed.vertices.iter_mut().filter(|v| (v.flags & 16 != 0) == control).enumerate() {
                    if let Some(old) = n.checked_sub(i + 1).and_then(|k| sequence.get(k)) {
                        *v = (*old).clone();
                    }
                }
            }
            reversed.display_points.clone_from(&out.points);
            reversed
        });
        out
    }

    /// Move every point (a whole-polyline edit): the fit data moves with it, or is dropped when it no longer described the
    /// polyline.
    pub fn map_points(&mut self, mut f: impl FnMut(&mut Vec3)) {
        if self.saved_fit().is_none() {
            self.fit = None;
        }
        self.points.iter_mut().for_each(&mut f);
        if let Some(fit) = &mut self.fit {
            fit.vertices.iter_mut().for_each(|v| f(&mut v.position));
            fit.display_points.clone_from(&self.points);
        }
    }
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

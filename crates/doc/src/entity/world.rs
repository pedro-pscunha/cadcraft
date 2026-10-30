//! Transient WCS geometry. Projection never becomes persisted drawing truth.

use cadcraft_geom::{Ellipse as GEllipse, Line as GLine, Mat4, OcsFrame, Segment, Spline as GSpline, TAU, Vec2, Vec3};

use super::{Entity, EntityKind, GeometryRevision, Gradient, Handle, MText, Prim, Text, TextFrame};
use crate::library::PatternLine;
use crate::{DocError, Result, has_finite_numbers};

/// A spatial primitive after its complete parent transform.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldPrimitive {
    Line {
        a: Vec3,
        b: Vec3,
    },
    Point(Vec3),
    /// A conic arc: `center + axis_u * cos t + axis_v * sin t` for `t` from `start` to `end` (counter-clockwise by
    /// increasing `t`; a full circle is `0` to `TAU`). The axes are conjugate radii, so every affine parent keeps the arc
    /// exact: a circle has equal orthogonal radii, an ellipse any conjugate pair.
    Conic {
        center: Vec3,
        axis_u: Vec3,
        axis_v: Vec3,
        start: f64,
        end: f64,
    },
    /// A rational B-spline: `knots.len() == control.len() + degree + 1`, and `weights` is empty (not rational) or has one
    /// weight per control point.
    Spline {
        degree: usize,
        knots: Vec<f64>,
        control: Vec<Vec3>,
        weights: Vec<f64>,
    },
    /// Text placed by its complete affine WCS frame. Layout lengths remain in local units.
    Text(WorldText),
    /// A filled region. The fill plane is the plane of its loops.
    Fill(WorldFill),
}

/// Text with its affine WCS frame and unchanged layout payload. Unlike a source [`TextFrame`], the baseline and height
/// axes carry the complete parent scale and shear. The normal follows their orientation; a collapsed frame has a zero
/// normal. Layout points use `origin + x_axis * x + y_axis * y`. Heights and widths remain in local units.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldText {
    pub frame: TextFrame,
    /// The WCS alignment point of a justified single-line text.
    pub align: Option<Vec3>,
    pub content: WorldTextContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldTextContent {
    Text(Text),
    MText(MText),
}

/// A filled region: closed loops of primitives (lines, arcs, splines) in one plane, with the island style (0 normal,
/// 1 outer, 2 ignore) the loops are filled by.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldFill {
    pub loops: Vec<Vec<WorldPrimitive>>,
    pub kind: WorldFillKind,
    pub style: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldFillKind {
    Solid,
    /// A wipeout mask.
    Wipeout,
    Pattern(WorldPattern),
    Gradient(WorldGradient),
}

/// A hatch pattern placed by an affine basis. Pattern lines remain relative to `origin`, `x_axis` and `y_axis`.
/// The axes retain the complete parent scale and shear. The angle and scale remain in local pattern coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPattern {
    pub name: String,
    pub scale: f64,
    pub angle: f64,
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub lines: Vec<PatternLine>,
}

/// A gradient placed by an affine basis. Its angle remains local, measured from `x_axis` towards `y_axis`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGradient {
    pub gradient: Gradient,
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
}

fn content_is_finite(content: &WorldTextContent) -> bool {
    match content {
        WorldTextContent::Text(t) => has_finite_numbers(t),
        WorldTextContent::MText(t) => has_finite_numbers(t),
    }
}

fn invalid(what: &str) -> DocError {
    DocError::Invalid(what.into())
}

impl WorldPrimitive {
    /// Check that the primitive is finite and well formed.
    pub fn validate(&self) -> Result<()> {
        let finite = |vs: &[Vec3]| vs.iter().all(|v| v.is_finite());
        match self {
            WorldPrimitive::Line { a, b } => finite(&[*a, *b]).then_some(()).ok_or_else(|| invalid("A world line needs finite points.")),
            WorldPrimitive::Point(p) => finite(&[*p]).then_some(()).ok_or_else(|| invalid("A world point needs finite coordinates.")),
            WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => {
                (finite(&[*center, *axis_u, *axis_v]) && start.is_finite() && end.is_finite())
                    .then_some(())
                    .ok_or_else(|| invalid("A world conic needs a finite center, finite radii and finite parameters."))
            }
            WorldPrimitive::Spline { degree, knots, control, weights } => {
                let shaped = *degree >= 1
                    && *degree < control.len()
                    && control.len().checked_add(*degree).and_then(|count| count.checked_add(1)) == Some(knots.len())
                    && (weights.is_empty() || weights.len() == control.len())
                    && finite(control)
                    && knots.iter().chain(weights).all(|v| v.is_finite());
                shaped
                    .then_some(())
                    .ok_or_else(|| invalid("A world spline needs knots = controls + degree + 1, matching weights and finite numbers."))
            }
            WorldPrimitive::Text(t) => {
                let f = &t.frame;
                (finite(&[f.origin, f.x_axis, f.y_axis, f.normal]) && t.align.is_none_or(|a| a.is_finite()) && content_is_finite(&t.content))
                    .then_some(())
                    .ok_or_else(|| invalid("A world text needs a finite frame and payload."))
            }
            WorldPrimitive::Fill(fill) => {
                let placement_ok = match &fill.kind {
                    WorldFillKind::Pattern(p) => {
                        finite(&[p.origin, p.x_axis, p.y_axis]) && p.scale.is_finite() && p.angle.is_finite() && has_finite_numbers(&p.lines)
                    }
                    WorldFillKind::Gradient(g) => finite(&[g.origin, g.x_axis, g.y_axis]) && has_finite_numbers(&g.gradient),
                    _ => true,
                };
                if !placement_ok {
                    return Err(invalid("A world fill needs a finite pattern or gradient placement."));
                }
                fill.loops.iter().flatten().try_for_each(WorldPrimitive::validate)
            }
        }
    }

    /// This primitive after the complete affine `parent`. Display bases retain scale, shear and reflection without
    /// imposing the representability limits of persisted entities.
    pub fn transformed(&self, parent: &Mat4) -> Result<WorldPrimitive> {
        self.validate()?;
        if parent.m[3] != [0.0, 0.0, 0.0, 1.0] || !has_finite_numbers(parent) {
            return Err(invalid("A world geometry parent must be a finite affine transform."));
        }
        let moved = match self {
            WorldPrimitive::Line { a, b } => WorldPrimitive::Line { a: parent.apply(*a), b: parent.apply(*b) },
            WorldPrimitive::Point(p) => WorldPrimitive::Point(parent.apply(*p)),
            WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => transformed_conic(parent, *center, *axis_u, *axis_v, *start, *end),
            WorldPrimitive::Spline { degree, knots, control, weights } => WorldPrimitive::Spline {
                degree: *degree,
                knots: knots.clone(),
                control: control.iter().map(|c| parent.apply(*c)).collect(),
                weights: weights.clone(),
            },
            WorldPrimitive::Text(t) => {
                let x_axis = parent.apply_vec(t.frame.x_axis);
                let y_axis = parent.apply_vec(t.frame.y_axis);
                let normal = match (x_axis.try_normalized(), y_axis.try_normalized()) {
                    (Some(x), Some(y)) => x.cross(y).try_normalized().unwrap_or(Vec3::ZERO),
                    _ => Vec3::ZERO,
                };
                let frame = TextFrame { origin: parent.apply(t.frame.origin), x_axis, y_axis, normal };
                WorldPrimitive::Text(WorldText { frame, align: t.align.map(|a| parent.apply(a)), content: t.content.clone() })
            }
            WorldPrimitive::Fill(fill) => {
                let kind = match &fill.kind {
                    WorldFillKind::Solid => WorldFillKind::Solid,
                    WorldFillKind::Wipeout => WorldFillKind::Wipeout,
                    WorldFillKind::Pattern(p) => WorldFillKind::Pattern(WorldPattern {
                        origin: parent.apply(p.origin),
                        x_axis: parent.apply_vec(p.x_axis),
                        y_axis: parent.apply_vec(p.y_axis),
                        ..p.clone()
                    }),
                    WorldFillKind::Gradient(g) => WorldFillKind::Gradient(WorldGradient {
                        origin: parent.apply(g.origin),
                        x_axis: parent.apply_vec(g.x_axis),
                        y_axis: parent.apply_vec(g.y_axis),
                        ..g.clone()
                    }),
                };
                let loops =
                    fill.loops.iter().map(|l| l.iter().map(|p| p.transformed(parent)).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
                WorldPrimitive::Fill(WorldFill { loops, kind, style: fill.style })
            }
        };
        moved.validate()?;
        Ok(moved)
    }

    /// The primitive in the XY view, after the complete parent. A text has no 2D primitive here: its frame stays in
    /// [`WorldGeometry::primitives`] for the consumer. A conic seen edge-on becomes the line between its extreme points.
    fn project_xy(&self, owner: Handle) -> Result<Vec<Prim>> {
        Ok(match self {
            Self::Line { a, b } => vec![Prim::Seg(Segment::Line(GLine::new(a.xy(), b.xy())))],
            Self::Point(point) => vec![Prim::Point(point.xy())],
            Self::Conic { center, axis_u, axis_v, start, end } => vec![project_conic(owner, *center, *axis_u, *axis_v, *start, *end)?],
            Self::Spline { degree, knots, control, weights } => vec![Prim::Spline(GSpline {
                degree: *degree,
                knots: knots.clone(),
                control: control.iter().map(|c| c.xy()).collect(),
                weights: weights.clone(),
                ..GSpline::default()
            })],
            Self::Text(_) => Vec::new(),
            Self::Fill(fill) => fill.loops.iter().map(|l| loop_polygon(l, owner).map(Prim::Fill)).collect::<Result<Vec<_>>>()?,
        })
    }
}

/// The XY view of a conic: an ellipse, a segment when the view is edge-on (rank one), a point when it is a point.
fn project_conic(owner: Handle, center: Vec3, axis_u: Vec3, axis_v: Vec3, start: f64, end: f64) -> Result<Prim> {
    if axis_u.xy() == Vec2::ZERO && axis_v.xy() == Vec2::ZERO {
        return Ok(Prim::Point(center.xy()));
    }
    let ellipse = GEllipse::from_conjugate(center.xy(), axis_u.xy(), axis_v.xy(), start, end);
    if !has_finite_numbers(&ellipse) {
        return Err(DocError::NonFinite(owner));
    }
    if cadcraft_geom::ccw_sweep(start, end) != TAU && ellipse.is_full() {
        return Err(DocError::Invalid(format!("Entity {} has a parameter span too small for this projected conic basis.", owner.hex())));
    }
    if ellipse.ratio == 0.0 {
        let mut lo = ellipse.start.cos().min(ellipse.end.cos());
        let mut hi = ellipse.start.cos().max(ellipse.end.cos());
        if cadcraft_geom::angle_in_sweep(0.0, ellipse.start, ellipse.end) {
            hi = 1.0;
        }
        if cadcraft_geom::angle_in_sweep(cadcraft_geom::PI, ellipse.start, ellipse.end) {
            lo = -1.0;
        }
        let a = ellipse.center + ellipse.major * lo;
        let b = ellipse.center + ellipse.major * hi;
        if !a.is_finite() || !b.is_finite() {
            return Err(DocError::NonFinite(owner));
        }
        return Ok(Prim::Seg(Segment::Line(GLine::new(a, b))));
    }
    Ok(Prim::Ellipse(ellipse))
}

/// A loop of primitives as a polygon in the XY view: its lines by their points, its conics and splines sampled.
fn loop_polygon(loop_: &[WorldPrimitive], owner: Handle) -> Result<Vec<Vec2>> {
    let mut out: Vec<Vec2> = Vec::new();
    for primitive in loop_ {
        match primitive {
            WorldPrimitive::Line { a, b } => out.extend([a.xy(), b.xy()]),
            WorldPrimitive::Conic { .. } | WorldPrimitive::Spline { .. } => {
                for prim in primitive.project_xy(owner)? {
                    match prim {
                        Prim::Ellipse(e) => {
                            let mut pts = Vec::new();
                            e.tessellate(e.major.len() * 1e-3, &mut pts);
                            out.extend(pts);
                        }
                        Prim::Seg(Segment::Line(l)) => out.extend([l.a, l.b]),
                        Prim::Spline(s) => out.extend(s.tessellate(1e-3)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out.dedup();
    Ok(out)
}

/// Finite spatial geometry, kept separate from its view projection and the source payload.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometry {
    owner: Handle,
    primitives: Vec<WorldPrimitive>,
}

impl WorldGeometry {
    /// Geometry owned by `owner` from primitives that are finite and well formed.
    pub fn from_primitives(owner: Handle, primitives: Vec<WorldPrimitive>) -> Result<WorldGeometry> {
        primitives.iter().try_for_each(WorldPrimitive::validate)?;
        Ok(WorldGeometry { owner, primitives })
    }

    pub fn primitives(&self) -> &[WorldPrimitive] {
        &self.primitives
    }

    /// Project the completed WCS result into XY, or refuse an unrepresentable projection.
    pub fn project_xy(&self) -> Result<Vec<Prim>> {
        let mut out = Vec::new();
        for primitive in &self.primitives {
            out.extend(primitive.project_xy(self.owner)?);
        }
        Ok(out)
    }
}

fn transformed_conic(parent: &Mat4, center: Vec3, axis_u: Vec3, axis_v: Vec3, start: f64, end: f64) -> WorldPrimitive {
    WorldPrimitive::Conic { center: parent.apply(center), axis_u: parent.apply_vec(axis_u), axis_v: parent.apply_vec(axis_v), start, end }
}

impl Entity {
    fn world_ocs_frame(&self) -> Result<OcsFrame> {
        OcsFrame::new(self.common.extrusion).ok_or_else(|| DocError::Invalid(format!("Entity {} has no valid source plane.", self.handle.hex())))
    }

    fn ocs_conic(&self, parent: &Mat4, center: Vec3, radius: f64, start: f64, end: f64) -> Result<WorldPrimitive> {
        let frame = self.world_ocs_frame()?;
        Ok(transformed_conic(
            parent,
            frame.to_world(center),
            frame.to_world(Vec3::new(radius, 0.0, 0.0)),
            frame.to_world(Vec3::new(0.0, radius, 0.0)),
            start,
            end,
        ))
    }

    /// Resolve canonical geometry in WCS before any display, pick, or spatial-query projection.
    pub fn world_geometry(&self, parent: &Mat4) -> Result<WorldGeometry> {
        if self.geometry_revision == GeometryRevision::LegacyProjectedV1 {
            return Err(DocError::Invalid("The legacy geometry payload needs recovery before world adaptation. Reopen the original DXF/DWG.".into()));
        }
        if !has_finite_numbers(self) || !has_finite_numbers(parent) {
            return Err(DocError::NonFinite(self.handle));
        }
        if parent.m[3] != [0.0, 0.0, 0.0, 1.0] {
            return Err(DocError::Invalid("A world geometry parent must be an affine transform.".into()));
        }
        // Each family adds its complete field contract here, not a projected legacy fallback.
        let primitives = match &self.kind {
            EntityKind::Line(line) => vec![WorldPrimitive::Line { a: parent.apply(line.a), b: parent.apply(line.b) }],
            EntityKind::Point(point) => vec![WorldPrimitive::Point(parent.apply(point.p))],
            EntityKind::Circle(circle) => vec![self.ocs_conic(parent, circle.center, circle.radius, 0.0, TAU)?],
            EntityKind::Arc(arc) => vec![self.ocs_conic(parent, arc.center, arc.radius, arc.start, arc.end)?],
            EntityKind::Ellipse(ellipse) => {
                let normal = self.world_ocs_frame()?.to_world(Vec3::Z);
                let major_direction = ellipse.major.try_normalized().ok_or_else(|| DocError::Invalid("An ellipse has no major axis.".into()))?;
                let minor_direction = normal
                    .cross(major_direction)
                    .try_normalized()
                    .ok_or_else(|| DocError::Invalid("An ellipse axis is parallel to its source normal.".into()))?;
                let scale = ellipse.major.x.abs().max(ellipse.major.y.abs()).max(ellipse.major.z.abs());
                let scaled_major = Vec3::new(ellipse.major.x / scale, ellipse.major.y / scale, ellipse.major.z / scale);
                let minor = minor_direction * (scaled_major.len() * ellipse.ratio) * scale;
                vec![transformed_conic(parent, ellipse.center, ellipse.major, minor, ellipse.start, ellipse.end)]
            }
            EntityKind::LwPolyline(polyline) => super::lwpolyline_primitives(polyline, self.common.extrusion, parent)?,
            EntityKind::Spline(spline) => super::spline_primitives(spline, parent)?,
            EntityKind::Helix(helix) => super::helix_primitives(helix, parent)?,
            EntityKind::Text(text) => super::text_primitives(text, self.common.extrusion, parent)?,
            EntityKind::MText(text) => super::mtext_primitives(text, self.common.extrusion, parent)?,
            EntityKind::MLine(mline) => super::mline_primitives(mline, self.common.extrusion, parent)?,
            EntityKind::Hatch(hatch) => super::hatch_primitives(hatch, self.common.extrusion, parent)?,
            EntityKind::Solid(solid) | EntityKind::Trace(solid) => super::solid_primitives(solid, self.common.extrusion, parent)?,
            EntityKind::Wipeout(wipeout) => super::wipeout_primitives(wipeout, parent)?,
            EntityKind::Polyline3d(_)
            | EntityKind::PolylineMesh(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::AttDef(_)
            | EntityKind::Insert(_)
            | EntityKind::Dimension(_)
            | EntityKind::Leader(_)
            | EntityKind::MLeader(_)
            | EntityKind::Face3d(_)
            | EntityKind::Region(_)
            | EntityKind::Viewport(_)
            | EntityKind::Image(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Unknown(_) => {
                return Err(DocError::Invalid(format!("The canonical {} geometry adapter is not available.", self.kind.type_name())));
            }
        };
        if primitives.iter().any(|primitive| primitive.validate().is_err()) {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok(WorldGeometry { owner: self.handle, primitives })
    }
}

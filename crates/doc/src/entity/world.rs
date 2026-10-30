//! Transient WCS geometry. Projection never becomes persisted drawing truth.

use cadcraft_geom::{Ellipse as GEllipse, Line as GLine, Mat4, Segment, Spline as GSpline, Vec2, Vec3};

use super::text_canonical::Similarity;
use super::{Entity, EntityKind, GeometryRevision, Gradient, MText, Prim, Text, TextFrame};
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
    /// A conic arc: `center + x_axis * cos t + y_axis * sin t` for `t` from `start` to `end` (counter-clockwise by
    /// increasing `t`; a full circle is `0` to `TAU`). The axes are conjugate radii, so every affine parent keeps the arc
    /// exact: a circle has equal orthogonal radii, an ellipse any conjugate pair.
    Arc {
        center: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
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
    /// Text placed by its WCS frame. A parent must be a similarity (a mirror, a shear and a non-uniform scale refuse).
    Text(WorldText),
    /// A filled region. The fill plane is the plane of its loops.
    Fill(WorldFill),
}

/// A text with its WCS frame (see [`TextFrame`]: the unit baseline and height directions, the unit plane normal) and the
/// payload its layout reads. The point and angle fields of the payload are source data; the frame and `align` place it,
/// and the heights and widths are in world units.
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

/// A hatch pattern placed in the fill plane: the pattern lines are relative to `origin` and the in-plane unit axes
/// (`x_axis` is the plane's X axis, `y_axis` the plane normal crossed into it); `angle` turns the pattern from `x_axis`
/// towards `y_axis`, and `scale` grows its lines.
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

/// A gradient placed in the fill plane; its angle is measured from `x_axis` towards `y_axis`.
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
            WorldPrimitive::Arc { center, x_axis, y_axis, start, end } => {
                (finite(&[*center, *x_axis, *y_axis]) && start.is_finite() && end.is_finite())
                    .then_some(())
                    .ok_or_else(|| invalid("A world arc needs a finite center, finite radii and finite parameters."))
            }
            WorldPrimitive::Spline { degree, knots, control, weights } => {
                let shaped = *degree >= 1
                    && knots.len() == control.len() + degree + 1
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
                let pattern_ok = match &fill.kind {
                    WorldFillKind::Pattern(p) => finite(&[p.origin, p.x_axis, p.y_axis]) && p.scale.is_finite() && p.angle.is_finite(),
                    WorldFillKind::Gradient(g) => finite(&[g.origin, g.x_axis, g.y_axis]),
                    _ => true,
                };
                if !pattern_ok {
                    return Err(invalid("A world fill needs a finite pattern or gradient placement."));
                }
                fill.loops.iter().flatten().try_for_each(WorldPrimitive::validate)
            }
        }
    }

    /// This primitive after the affine `parent`. A text and a pattern or gradient fill need a similarity; every other
    /// primitive (and a solid or wipeout fill) accepts any affine map, because its control geometry maps exactly.
    pub fn transformed(&self, parent: &Mat4) -> Result<WorldPrimitive> {
        if parent.m[3] != [0.0, 0.0, 0.0, 1.0] || !has_finite_numbers(parent) {
            return Err(invalid("A world geometry parent must be a finite affine transform."));
        }
        let moved = match self {
            WorldPrimitive::Line { a, b } => WorldPrimitive::Line { a: parent.apply(*a), b: parent.apply(*b) },
            WorldPrimitive::Point(p) => WorldPrimitive::Point(parent.apply(*p)),
            WorldPrimitive::Arc { center, x_axis, y_axis, start, end } => WorldPrimitive::Arc {
                center: parent.apply(*center),
                x_axis: parent.apply_vec(*x_axis),
                y_axis: parent.apply_vec(*y_axis),
                start: *start,
                end: *end,
            },
            WorldPrimitive::Spline { degree, knots, control, weights } => WorldPrimitive::Spline {
                degree: *degree,
                knots: knots.clone(),
                control: control.iter().map(|c| parent.apply(*c)).collect(),
                weights: weights.clone(),
            },
            WorldPrimitive::Text(t) => {
                let sim = Similarity::from_mat4(parent)?;
                let frame = TextFrame {
                    origin: sim.point(t.frame.origin),
                    x_axis: sim.direction(t.frame.x_axis),
                    y_axis: sim.direction(t.frame.y_axis),
                    normal: sim.direction(t.frame.normal),
                };
                let k = sim.scale;
                let content = match &t.content {
                    WorldTextContent::Text(text) => WorldTextContent::Text(Text { height: text.height * k, ..text.clone() }),
                    WorldTextContent::MText(text) => {
                        let mut text = text.clone();
                        text.height *= k;
                        text.width *= k;
                        if let Some(c) = &mut text.columns {
                            c.scale(k);
                        }
                        WorldTextContent::MText(text)
                    }
                };
                WorldPrimitive::Text(WorldText { frame, align: t.align.map(|a| sim.point(a)), content })
            }
            WorldPrimitive::Fill(fill) => {
                let kind = match &fill.kind {
                    WorldFillKind::Solid => WorldFillKind::Solid,
                    WorldFillKind::Wipeout => WorldFillKind::Wipeout,
                    WorldFillKind::Pattern(p) => {
                        let sim = Similarity::from_mat4(parent)?;
                        WorldFillKind::Pattern(WorldPattern {
                            scale: p.scale * sim.scale,
                            origin: sim.point(p.origin),
                            x_axis: sim.direction(p.x_axis),
                            y_axis: sim.direction(p.y_axis),
                            ..p.clone()
                        })
                    }
                    WorldFillKind::Gradient(g) => {
                        let sim = Similarity::from_mat4(parent)?;
                        WorldFillKind::Gradient(WorldGradient {
                            origin: sim.point(g.origin),
                            x_axis: sim.direction(g.x_axis),
                            y_axis: sim.direction(g.y_axis),
                            ..g.clone()
                        })
                    }
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
    fn project_xy(&self) -> Vec<Prim> {
        match self {
            WorldPrimitive::Line { a, b } => vec![Prim::Seg(Segment::Line(GLine::new(a.xy(), b.xy())))],
            WorldPrimitive::Point(point) => vec![Prim::Point(point.xy())],
            WorldPrimitive::Arc { center, x_axis, y_axis, start, end } => {
                let ellipse = GEllipse::from_conjugate(center.xy(), x_axis.xy(), y_axis.xy(), *start, *end);
                if ellipse.ratio > 1e-9 && ellipse.major.len() > 0.0 {
                    return vec![Prim::Ellipse(ellipse)];
                }
                // Edge-on: the arc lies on a line; keep the segment between its extreme points along it.
                let at = |t: f64| (*center + *x_axis * t.cos() + *y_axis * t.sin()).xy();
                let (dx, dy) = (x_axis.xy(), y_axis.xy());
                let along = if dx.len() >= dy.len() { dx } else { dy };
                let dir = if along.len() > 0.0 { along.normalized() } else { Vec2::X };
                let full = (end - start).abs() >= std::f64::consts::TAU - 1e-9;
                let sweep = if full { std::f64::consts::TAU } else { cadcraft_geom::ccw_sweep(*start, *end) };
                let mut points = vec![at(*start), at(*start + sweep)];
                // The parameters where the position along the line turns back.
                let t_max = dy.dot(dir).atan2(dx.dot(dir));
                for t in [t_max, t_max + std::f64::consts::PI] {
                    if full || cadcraft_geom::ccw_sweep(*start, t) <= sweep {
                        points.push(at(t));
                    }
                }
                let key = |p: &Vec2| p.dot(dir);
                let lo = points.iter().copied().min_by(|a, b| key(a).total_cmp(&key(b))).unwrap_or(Vec2::ZERO);
                let hi = points.iter().copied().max_by(|a, b| key(a).total_cmp(&key(b))).unwrap_or(Vec2::ZERO);
                vec![Prim::Seg(Segment::Line(GLine::new(lo, hi)))]
            }
            WorldPrimitive::Spline { degree, knots, control, weights } => {
                let spline = GSpline {
                    degree: *degree,
                    knots: knots.clone(),
                    control: control.iter().map(|c| c.xy()).collect(),
                    weights: weights.clone(),
                    ..GSpline::default()
                };
                vec![Prim::Spline(spline)]
            }
            WorldPrimitive::Text(_) => Vec::new(),
            WorldPrimitive::Fill(fill) => fill.loops.iter().map(|l| Prim::Fill(loop_polygon(l))).collect(),
        }
    }
}

/// A loop of primitives as a polygon in the XY view: its lines by their points, its arcs and splines sampled.
fn loop_polygon(loop_: &[WorldPrimitive]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for primitive in loop_ {
        match primitive {
            WorldPrimitive::Line { a, b } => out.extend([a.xy(), b.xy()]),
            WorldPrimitive::Arc { .. } => {
                for prim in primitive.project_xy() {
                    match prim {
                        Prim::Ellipse(e) => {
                            let mut pts = Vec::new();
                            e.tessellate(e.major.len() * 1e-3, &mut pts);
                            out.extend(pts);
                        }
                        Prim::Seg(Segment::Line(l)) => out.extend([l.a, l.b]),
                        _ => {}
                    }
                }
            }
            WorldPrimitive::Spline { .. } => {
                for prim in primitive.project_xy() {
                    if let Prim::Spline(s) = prim {
                        out.extend(s.tessellate(1e-3));
                    }
                }
            }
            _ => {}
        }
    }
    out.dedup();
    out
}

/// Finite spatial geometry, kept separate from its view projection and the source payload.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldGeometry {
    primitives: Vec<WorldPrimitive>,
}

impl WorldGeometry {
    /// Geometry from primitives that are finite and well formed.
    pub fn from_primitives(primitives: Vec<WorldPrimitive>) -> Result<WorldGeometry> {
        primitives.iter().try_for_each(WorldPrimitive::validate)?;
        Ok(WorldGeometry { primitives })
    }

    pub fn primitives(&self) -> &[WorldPrimitive] {
        &self.primitives
    }

    /// Project the completed WCS result into the current XY view.
    pub fn project_xy(&self) -> Vec<Prim> {
        self.primitives.iter().flat_map(WorldPrimitive::project_xy).collect()
    }
}

impl Entity {
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
        let primitive = match &self.kind {
            EntityKind::Line(line) => WorldPrimitive::Line { a: parent.apply(line.a), b: parent.apply(line.b) },
            EntityKind::Point(point) => WorldPrimitive::Point(parent.apply(point.p)),
            EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::PolylineMesh(_)
            | EntityKind::Spline(_)
            | EntityKind::Helix(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::AttDef(_)
            | EntityKind::Insert(_)
            | EntityKind::Dimension(_)
            | EntityKind::Leader(_)
            | EntityKind::MLeader(_)
            | EntityKind::MLine(_)
            | EntityKind::Hatch(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::Region(_)
            | EntityKind::Viewport(_)
            | EntityKind::Image(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Unknown(_) => {
                return Err(DocError::Invalid(format!("The canonical {} geometry adapter is not available.", self.kind.type_name())));
            }
        };
        if primitive.validate().is_err() {
            return Err(DocError::NonFinite(self.handle));
        }
        Ok(WorldGeometry { primitives: vec![primitive] })
    }
}

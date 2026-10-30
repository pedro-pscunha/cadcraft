//! Grips of a canonical entity and what dragging each does.
//!
//! The grips are WCS points of the geometry the shared resolver gives. The families that have vertex and end grips (line,
//! circle, arc, 2D polyline, spline) keep the order of their legacy grips; any other family has the generic grips of
//! [`crate::canonical::Flat::grips`], which only move the whole object. A drag works in the object's own plane: the cursor
//! (an XY position) is carried along the view direction (Z) onto the plane of the grip, so the plane and the normal never
//! change; a plane the view direction does not cross refuses.

use cadcraft_doc::{Arc, Circle, Drawing, Entity, EntityKind, LwPolyline};
use cadcraft_geom::{Arc as GArc, Mat3, Mat4, OcsFrame, Vec2, Vec3};

use crate::grips::PolyGrip;
use crate::{EngineError, Result};

/// What dragging a grip does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Role {
    /// Moves the whole object.
    Move,
    /// An end of a line: `true` for the second.
    LineEnd(bool),
    /// A quadrant of a circle: the radius.
    CircleRadius,
    /// The start (0), middle (1) or end (2) of an arc: the arc through the three points.
    ArcPoint(usize),
    PolyVertex(usize),
    PolyMid(usize),
    /// A frame control point of a spline-fit polyline: the curve is refit.
    PolyFrame(usize),
    SplineControl(usize),
    /// A grip with no stretch: dragging it refuses.
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct KindGrip {
    pub world: Vec3,
    pub role: Role,
}

fn frame(normal: Vec3) -> Option<OcsFrame> {
    OcsFrame::new(normal)
}

/// The grips of a canonical entity, in grip-index order; empty when the resolver refuses it.
pub(crate) fn grips(d: &Drawing, e: &Entity) -> Vec<KindGrip> {
    let Some(flat) = crate::canonical::flatten(d, e) else { return Vec::new() };
    if flat.refused {
        return Vec::new();
    }
    kind_grips(e).unwrap_or_else(|| {
        flat.grips().into_iter().map(|g| KindGrip { world: g.world, role: if g.moves { Role::Move } else { Role::Fixed } }).collect()
    })
}

/// The grips of the families with vertex and end grips, in the order of their legacy grips.
fn kind_grips(e: &Entity) -> Option<Vec<KindGrip>> {
    let f = frame(e.common.extrusion);
    let grip = |world, role| KindGrip { world, role };
    match &e.kind {
        EntityKind::Line(l) => Some(vec![grip(l.a, Role::LineEnd(false)), grip((l.a + l.b) * 0.5, Role::Move), grip(l.b, Role::LineEnd(true))]),
        EntityKind::Circle(c) => {
            let f = f?;
            let center = f.to_world(c.center);
            let mut out = vec![grip(center, Role::Move)];
            out.extend((0..4).map(|k| {
                let a = f64::from(k) * std::f64::consts::FRAC_PI_2;
                grip(center + f.to_world(Vec3::new(c.radius * a.cos(), c.radius * a.sin(), 0.0)), Role::CircleRadius)
            }));
            Some(out)
        }
        EntityKind::Arc(a) => {
            let f = f?;
            let g = GArc::new(a.center.xy(), a.radius, a.start, a.end);
            let at = |p: Vec2| f.to_world(p.to3(a.center.z));
            Some(vec![
                grip(at(g.start_point()), Role::ArcPoint(0)),
                grip(at(g.mid_point()), Role::ArcPoint(1)),
                grip(at(g.end_point()), Role::ArcPoint(2)),
                grip(f.to_world(a.center), Role::Move),
            ])
        }
        EntityKind::LwPolyline(p) => {
            let f = f?;
            let points = e.kind.grips();
            Some(
                points
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        let role = match crate::grips::poly_grip(p, i) {
                            Some(PolyGrip::Vertex(v)) => Role::PolyVertex(v),
                            Some(PolyGrip::Mid(m)) => Role::PolyMid(m),
                            Some(PolyGrip::Frame(k)) => Role::PolyFrame(k),
                            None => Role::Fixed,
                        };
                        grip(f.to_world(q.to3(p.elevation)), role)
                    })
                    .collect(),
            )
        }
        EntityKind::Spline(s) => {
            let control = s.control3()?;
            if s.fit.is_empty() {
                Some(control.into_iter().enumerate().map(|(i, c)| grip(c, Role::SplineControl(i))).collect())
            } else {
                // Moving a fit point refits the curve; a canonical spline is never rebuilt from its fit points.
                Some(s.fit3()?.into_iter().map(|c| grip(c, Role::Fixed)).collect())
            }
        }
        _ => None,
    }
}

fn leaves_the_plane() -> EngineError {
    EngineError::Other("A canonical object's grip would leave the object's own plane for this view direction.".into())
}

fn no_stretch() -> EngineError {
    EngineError::Other("A canonical object's grip can only move the whole object here: this grip would stretch the fields of its 2D reading.".into())
}

/// The point of the plane through `through` with the normal `normal` that the view direction (Z) carries to the XY position `to`.
fn onto_plane(to: Vec2, through: Vec3, normal: Vec3) -> Result<Vec3> {
    let nz = normal.z;
    if nz.abs() <= 1e-9 * normal.len() || !nz.is_finite() {
        return Err(leaves_the_plane());
    }
    let z = through.z - (normal.x * (to.x - through.x) + normal.y * (to.y - through.y)) / nz;
    Ok(Vec3::new(to.x, to.y, z))
}

/// The canonical candidate after dragging grip `index` to the XY position `to`: the whole object moved, or the one family-specific
/// stretch in the object's own plane; `Ok(None)` when the grip does not exist or the result would be degenerate.
pub(crate) fn stretch(d: &Drawing, e: &Entity, index: usize, to: Vec2) -> Result<Option<Entity>> {
    let all = grips(d, e);
    let Some(grip) = all.get(index).copied() else { return Ok(None) };
    if !to.is_finite() {
        return Ok(None);
    }
    let with = |kind: EntityKind| Some(Entity { kind, ..e.clone() });
    match (grip.role, &e.kind) {
        (Role::Move, _) => {
            let delta = to - grip.world.xy();
            Ok(Some(e.transformed(&Mat4::from_mat3(&Mat3::translate(delta)))?))
        }
        (Role::Fixed, _) => Err(no_stretch()),
        (Role::LineEnd(second), EntityKind::Line(l)) => {
            // A line has no plane of its own: its end moves in the horizontal plane it is in.
            let mut line = l.clone();
            let end = if second { &mut line.b } else { &mut line.a };
            *end = Vec3::new(to.x, to.y, end.z);
            Ok(with(EntityKind::Line(line)))
        }
        (Role::CircleRadius, EntityKind::Circle(c)) => {
            let Some(f) = frame(e.common.extrusion) else { return Ok(None) };
            let p = onto_plane(to, grip.world, f.to_world(Vec3::Z))?;
            let radius = (p - f.to_world(c.center)).len();
            Ok((radius > 1e-12 && radius.is_finite()).then(|| Entity { kind: EntityKind::Circle(Circle { radius, ..c.clone() }), ..e.clone() }))
        }
        (Role::ArcPoint(k), EntityKind::Arc(a)) => {
            let Some(f) = frame(e.common.extrusion) else { return Ok(None) };
            let p = onto_plane(to, grip.world, f.to_world(Vec3::Z))?;
            let ocs = |g: usize| f.to_ocs(all[g].world).xy();
            let mut three = [ocs(0), ocs(1), ocs(2)];
            three[k] = f.to_ocs(p).xy();
            let Some(arc) = GArc::from_3_points(three[0], three[1], three[2]) else { return Ok(None) };
            let center = Vec3::new(arc.center.x, arc.center.y, a.center.z);
            Ok(with(EntityKind::Arc(Arc { center, radius: arc.radius, start: arc.start, end: arc.end })))
        }
        (Role::PolyVertex(i), EntityKind::LwPolyline(p)) => {
            let Some(f) = frame(e.common.extrusion) else { return Ok(None) };
            let at = f.to_ocs(onto_plane(to, grip.world, f.to_world(Vec3::Z))?).xy();
            let mut polyline = p.clone();
            let Some(v) = polyline.vertices.get_mut(i) else { return Ok(None) };
            v.p = at;
            Ok(with(EntityKind::LwPolyline(polyline)))
        }
        (Role::PolyFrame(k), EntityKind::LwPolyline(p)) => {
            let Some(f) = frame(e.common.extrusion) else { return Ok(None) };
            let at = f.to_ocs(onto_plane(to, grip.world, f.to_world(Vec3::Z))?).xy();
            let mut polyline = p.clone();
            if polyline.drag_frame(k, at, crate::grips::splinesegs(d), crate::cmd::curves::MAX_GEN).is_none() {
                return Ok(None);
            }
            Ok(with(EntityKind::LwPolyline(polyline)))
        }
        (Role::PolyMid(i), EntityKind::LwPolyline(p)) => {
            let Some(f) = frame(e.common.extrusion) else { return Ok(None) };
            let at = f.to_ocs(onto_plane(to, grip.world, f.to_world(Vec3::Z))?).xy();
            let mut polyline: LwPolyline = p.clone();
            let n = polyline.vertices.len().max(1);
            let next = (i + 1) % n;
            let (Some(start), Some(end)) = (polyline.vertices.get(i).map(|v| v.p), polyline.vertices.get(next).map(|v| v.p)) else { return Ok(None) };
            if polyline.vertices.get(i).is_some_and(|v| v.bulge.abs() > 1e-12) {
                // An arc segment is reshaped through the dragged midpoint.
                if let Some(v) = polyline.vertices.get_mut(i) {
                    v.bulge = crate::cmd::curves::bulge_through(start, at, end);
                }
            } else {
                let delta = at - f.to_ocs(grip.world).xy();
                if let Some(v) = polyline.vertices.get_mut(i) {
                    v.p = start + delta;
                }
                if let Some(v) = polyline.vertices.get_mut(next) {
                    v.p = end + delta;
                }
            }
            Ok(with(EntityKind::LwPolyline(polyline)))
        }
        (Role::SplineControl(i), EntityKind::Spline(s)) => {
            // A spatial spline has no plane of its own: its control point moves in the horizontal plane it is in.
            let mut spline = s.clone();
            let Some(c) = spline.control.get_mut(i) else { return Ok(None) };
            *c = to;
            Ok(with(EntityKind::Spline(spline)))
        }
        _ => Err(no_stretch()),
    }
}

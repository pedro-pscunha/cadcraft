//! Paint roles of canonical geometry that the legacy painter draws in the entity's own plane or along the curve: line patterns,
//! wide polylines and point markers. They reuse the legacy painters: the dash laying-out of `linetype::apply` along the true
//! (unfolded) length of the world curve, and `fill::wide_polyline` / `point::figure` in the local 2D coordinates of the entity's
//! plane, placed by one local-to-world matrix and projected for display.

use cadcraft_doc::{DocError, Entity, Linetype, WorldPrimitive};
use cadcraft_geom::{Mat3, Mat4, OcsFrame, Spline, Vec2, Vec3};

use super::{Builder, Ctx, Ink, linetype};

/// A line pattern ready to lay out in WCS: the linetype, its scale in world units and the shortest pattern it draws as dashes.
pub(crate) struct Pattern<'a> {
    pub linetype: &'a Linetype,
    pub scale: f64,
    pub min: f64,
}

fn unsupported(feature: &str) -> DocError {
    DocError::Invalid(format!("Canonical display does not support {feature} yet. The source geometry remains unchanged."))
}

/// The WCS points of a line, conic or spline primitive, sampled so that a chord is within `tolerance` of the curve.
fn sample(primitive: &WorldPrimitive, tolerance: f64) -> Option<Vec<Vec3>> {
    match primitive {
        WorldPrimitive::Line { a, b } => Some(vec![*a, *b]),
        WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => {
            let sweep = cadcraft_geom::ccw_sweep(*start, *end);
            let radius = axis_u.len().max(axis_v.len());
            let step = if radius > 0.0 { 2.0 * (1.0 - (tolerance / radius).min(1.0)).acos() } else { 1.0 };
            let n = if step > 1e-9 { (sweep / step).ceil() as usize } else { 4096 }.clamp(8, 4096);
            Some(
                (0..=n)
                    .map(|i| *center + *axis_u * (start + sweep * i as f64 / n as f64).cos() + *axis_v * (start + sweep * i as f64 / n as f64).sin())
                    .collect(),
            )
        }
        WorldPrimitive::Spline { degree, knots, control, weights } => {
            // The X and Y values and the Z values of the control points are three splines of one parameter (a B-spline is
            // affine-invariant), sampled together.
            let planar = Spline {
                degree: *degree,
                knots: knots.clone(),
                control: control.iter().map(|c| c.xy()).collect(),
                weights: weights.clone(),
                ..Spline::default()
            };
            let height = Spline { control: control.iter().map(|c| Vec2::new(c.z, 0.0)).collect(), ..planar.clone() };
            let (lo, hi) = planar.domain();
            let n = (control.len() * 24).clamp(32, 4096);
            Some(
                (0..=n)
                    .map(|i| {
                        let t = lo + (hi - lo) * i as f64 / n as f64;
                        planar.eval(t).to3(height.eval(t).x)
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// The WCS point at the distance `s` along the polyline `points`, whose cumulative lengths are `lengths`.
fn along(points: &[Vec3], lengths: &[f64], s: f64) -> Vec3 {
    let i = lengths.partition_point(|l| *l < s).clamp(1, lengths.len().saturating_sub(1).max(1));
    let (Some(a), Some(b), Some(&s0), Some(&s1)) = (points.get(i - 1), points.get(i), lengths.get(i - 1), lengths.get(i)) else {
        return points.first().copied().unwrap_or(Vec3::ZERO);
    };
    if s1 - s0 <= 0.0 { *a } else { *a + (*b - *a) * ((s - s0) / (s1 - s0)).clamp(0.0, 1.0) }
}

/// Draw one curve primitive with a line pattern: the pattern runs along the length of the curve in WCS (not its projection), then
/// each dash is projected for display. `None` when the primitive is not a curve.
pub(crate) fn dashed(
    builder: &mut Builder,
    context: &Ctx,
    color: Ink,
    lineweight: f32,
    pattern: &Pattern,
    primitive: &WorldPrimitive,
) -> Option<cadcraft_doc::Result<()>> {
    let points = sample(primitive, builder.opts.tolerance)?;
    if builder.opts.text && pattern.linetype.pattern.iter().any(|e| e.text.as_deref().is_some_and(|t| !t.trim().is_empty())) {
        return Some(Err(unsupported("line patterns with embedded text")));
    }
    let mut lengths = vec![0.0];
    for w in points.windows(2) {
        if let [a, b] = w {
            lengths.push(lengths.last().copied().unwrap_or(0.0) + (*b - *a).len());
        }
    }
    let unfolded: Vec<Vec2> = lengths.iter().map(|s| Vec2::new(*s, 0.0)).collect();
    for dash in linetype::apply(&unfolded, pattern.linetype, pattern.scale, pattern.min) {
        let placed: Vec<Vec2> = dash.iter().map(|q| along(&points, &lengths, q.x).xy()).collect();
        match placed.as_slice() {
            [p] => builder.point(context, color, *p),
            _ => builder.polyline(context, color, lineweight, &placed),
        }
    }
    Some(Ok(()))
}

/// The matrix from the local 2D coordinates of an entity's plane (OCS at the height `z`) to WCS after the parent, and the context
/// that draws local points through it.
fn local_context<'a>(context: &Ctx<'a>, entity: &Entity, z: f64) -> cadcraft_doc::Result<Ctx<'a>> {
    let frame = OcsFrame::new(entity.common.extrusion)
        .ok_or_else(|| DocError::Invalid(format!("Entity {} has no valid source plane.", entity.handle.hex())))?;
    let lift = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, z], [0.0, 0.0, 0.0, 1.0]] };
    let matrix = context.world.mul(&frame.matrix()).mul(&lift);
    if !cadcraft_doc::has_finite_numbers(&matrix) {
        return Err(DocError::NonFinite(entity.handle));
    }
    let xy = Mat3 { a: matrix.m[0][0], b: matrix.m[1][0], c: matrix.m[0][1], d: matrix.m[1][1], e: matrix.m[0][3], f: matrix.m[1][3] };
    Ok(Ctx { world: matrix, xf: xy, ..context.clone() })
}

/// A wide polyline: its band in the plane of the entity, drawn by the legacy painter in the polyline's own 2D coordinates and
/// projected for display.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wide_polyline(
    builder: &mut Builder,
    context: &Ctx,
    entity: &Entity,
    polyline: &cadcraft_doc::LwPolyline,
    color: Ink,
    lineweight: f32,
    linetype: Option<&Linetype>,
    linetype_scale: f64,
) -> cadcraft_doc::Result<()> {
    let local = local_context(context, entity, polyline.elevation)?;
    let tol = builder.opts.tolerance / local.xf.scale_factor().max(1e-12);
    if builder.opts.fill && context.d.header.i64("FILLMODE", 1) != 0 {
        super::draw::wide_fill(builder, &local, polyline, color, lineweight, linetype, linetype_scale, tol);
    } else {
        let line = cadcraft_geom::Polyline { vertices: polyline.vertices.clone(), closed: polyline.closed };
        super::draw::stroke_with(builder, &local, color, lineweight, linetype, linetype_scale, &line.tessellate(tol));
    }
    Ok(())
}

/// A point marker (PDMODE and PDSIZE): its figure in the plane of the point, drawn by the legacy painter in the 2D coordinates of
/// that plane and projected for display.
pub(crate) fn point_marker(
    builder: &mut Builder,
    context: &Ctx,
    entity: &Entity,
    point: &cadcraft_doc::Point,
    color: Ink,
    lineweight: f32,
) -> cadcraft_doc::Result<()> {
    let frame = OcsFrame::new(entity.common.extrusion)
        .ok_or_else(|| DocError::Invalid(format!("Entity {} has no valid source plane.", entity.handle.hex())))?;
    let at = frame.to_ocs(point.p);
    let local = local_context(context, entity, at.z)?;
    // Relative sizes follow the view: undo the block / viewport scale.
    let view = builder.opts.view_height / context.xf.scale_factor().max(1e-12);
    let size = super::point::size(context.d.header.f64("PDSIZE", 0.0), view);
    let tol = builder.opts.tolerance / local.xf.scale_factor().max(1e-12);
    let figure = super::point::figure(context.d.header.i64("PDMODE", 0), size, at.xy(), point.angle, tol);
    if figure.dot {
        builder.point(&local, color, at.xy());
    }
    for line in &figure.lines {
        builder.polyline(&local, color, lineweight, line);
    }
    Ok(())
}

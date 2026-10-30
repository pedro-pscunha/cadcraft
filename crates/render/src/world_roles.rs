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

/// The point at the distance `s` along the polyline `points` and the unit direction of its segment (the last segment extends a
/// little past the end).
fn along_with_direction(points: &[Vec3], lengths: &[f64], s: f64) -> Option<(Vec3, Vec3)> {
    let i = lengths.partition_point(|l| *l < s).clamp(1, lengths.len().saturating_sub(1).max(1));
    // A zero-length segment has no direction: look to the next one that has.
    (1..points.len()).filter(|k| *k >= i).chain((1..i).rev()).find_map(|k| {
        let (a, b) = (*points.get(k - 1)?, *points.get(k)?);
        let t = (b - a).try_normalized()?;
        Some((along(points, lengths, s), t))
    })
}

/// Draw curve primitives as one line with a line pattern: the pattern runs along the length of the chain in WCS (not its
/// projection), then each dash and each text of the linetype is projected for display. The texts lie in the plane of the entity
/// (`plane`: its OCS), turned to the direction of the curve. `false` when a primitive is not a curve.
pub(crate) fn dashed(
    builder: &mut Builder,
    context: &Ctx,
    color: Ink,
    lineweight: f32,
    pattern: &Pattern,
    plane: Option<OcsFrame>,
    chain: &[&WorldPrimitive],
) -> bool {
    let mut points: Vec<Vec3> = Vec::new();
    for primitive in chain {
        let Some(mut piece) = sample(primitive, builder.opts.tolerance) else { return false };
        // A piece runs from the end of the one before it: arcs are laid out counter-clockwise whatever the polyline direction.
        if let (Some(last), Some(first), Some(end)) = (points.last(), piece.first(), piece.last())
            && (*end - *last).len() < (*first - *last).len()
        {
            piece.reverse();
        }
        let joined = points.last().zip(piece.first()).is_some_and(|(a, b)| (*a - *b).len() < 1e-9 * (1.0 + a.len()));
        points.extend(piece.into_iter().skip(usize::from(joined)));
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
    if builder.opts.text
        && let (Some(frame), Some(&total)) = (plane, lengths.last())
    {
        for el in super::linetype_text::placed(context.d, pattern.linetype, pattern.scale, pattern.min, total) {
            let Some((at, tangent)) = along_with_direction(&points, &lengths, el.s) else { break };
            // In the plane of the entity: the direction across the curve is the plane normal crossed into the tangent.
            let normal = frame.to_world(Vec3::Z);
            let Some(across) = normal.cross(tangent).try_normalized() else { continue };
            let direction = if el.absolute {
                frame.to_world(Vec3::new(el.rotation.cos(), el.rotation.sin(), 0.0))
            } else {
                tangent * el.rotation.cos() + across * el.rotation.sin()
            };
            let Some(direction) = direction.try_normalized() else { continue };
            let up = normal.cross(direction);
            let origin = at + tangent * el.offset.x + across * el.offset.y;
            let mut shaped = el.local;
            shaped.map(|q| (origin + direction * q.x + up * q.y).xy());
            builder.shaped(context, color, lineweight, &shaped);
        }
    }
    true
}

/// The matrix from the local 2D coordinates of an entity's plane (OCS at the height `z`) to WCS after the parent, and the context
/// that draws local points through it.
pub(crate) fn local_context<'a>(context: &Ctx<'a>, entity: &Entity, z: f64) -> cadcraft_doc::Result<Ctx<'a>> {
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
    // Relative sizes follow the largest visible scale, even when the parent makes the plane edge-on.
    let view = builder.opts.view_height / super::max_scale(&local.xf).max(1e-12);
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

/// A translation by `v`.
pub(crate) fn translation(v: Vec3) -> Mat4 {
    Mat4 { m: [[1.0, 0.0, 0.0, v.x], [0.0, 1.0, 0.0, v.y], [0.0, 0.0, 1.0, v.z], [0.0, 0.0, 0.0, 1.0]] }
}

/// Whether the entity is swept along its normal by its thickness: the kinds with an extrusion. A text is swept only in a stroke
/// (SHX-like) font; AutoCAD gives TrueType text no thickness, so an outline font draws flat.
pub(crate) fn swept(d: &cadcraft_doc::Drawing, entity: &Entity) -> bool {
    use cadcraft_doc::EntityKind as K;
    match &entity.kind {
        K::Text(t) => stroke_font(d, &t.style),
        K::AttDef(a) => stroke_font(d, &a.text.style),
        K::Line(_) | K::Point(_) | K::Circle(_) | K::Arc(_) | K::Ellipse(_) | K::LwPolyline(_) | K::Solid(_) | K::Trace(_) => true,
        _ => false,
    }
}

/// Whether the text style `style` is set in a stroke (SHX-like) font, the only text AutoCAD extrudes.
pub(crate) fn stroke_font(d: &cadcraft_doc::Drawing, style: &str) -> bool {
    matches!(cadcraft_fonts::TextFont::resolve(&super::text_entity::text_style(d, style).font), cadcraft_fonts::TextFont::Stroke)
}

/// The vector from an outline to its far copy: the thickness along the normal of the entity's plane, carried by the parent.
pub(crate) fn extrusion_offset(context: &Ctx, entity: &Entity) -> cadcraft_doc::Result<Vec3> {
    let frame = OcsFrame::new(entity.common.extrusion)
        .ok_or_else(|| DocError::Invalid(format!("Entity {} has no valid source plane.", entity.handle.hex())))?;
    let offset = context.world.apply_vec(frame.to_world(Vec3::Z) * entity.common.thickness);
    if offset.is_finite() { Ok(offset) } else { Err(DocError::NonFinite(entity.handle)) }
}

/// The side edges of an extrusion: a line from each vertex of the outline to its far copy at `offset`. A line has its two ends, a
/// full circle its four quadrants, an arc its ends, a polyline its vertices, a point itself, a SOLID or TRACE its corners and a text
/// every vertex of its strokes (every vertex the flat painter draws). Edges seen end-on (a normal along the view) have no length and are left out.
pub(crate) fn side_edges(
    builder: &mut Builder,
    context: &Ctx,
    entity: &Entity,
    offset: Vec3,
    color: Ink,
    lineweight: f32,
) -> cadcraft_doc::Result<()> {
    if offset.xy().len() <= 0.0 {
        return Ok(());
    }
    let geometry = super::entity_world_geometry(context.d, entity, &context.world)?;
    side_edges_of(builder, context, geometry.primitives(), offset, color, lineweight)
}

/// The side edges of an extrusion of the WCS `primitives` by `offset` (see [`side_edges`]).
pub(crate) fn side_edges_of(
    builder: &mut Builder,
    context: &Ctx,
    primitives: &[WorldPrimitive],
    offset: Vec3,
    color: Ink,
    lineweight: f32,
) -> cadcraft_doc::Result<()> {
    if offset.xy().len() <= 0.0 {
        return Ok(());
    }
    let mut vertices: Vec<Vec3> = Vec::new();
    for primitive in primitives {
        match primitive {
            WorldPrimitive::Line { a, b } => vertices.extend([*a, *b]),
            WorldPrimitive::Point(p) => vertices.push(*p),
            WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => {
                let at = |t: f64| *center + *axis_u * t.cos() + *axis_v * t.sin();
                if cadcraft_geom::ccw_sweep(*start, *end) >= std::f64::consts::TAU - 1e-12 {
                    vertices.extend((0..4).map(|k| at(f64::from(k) * std::f64::consts::FRAC_PI_2)));
                } else {
                    vertices.extend([at(*start), at(*end)]);
                }
            }
            WorldPrimitive::Spline { .. } | WorldPrimitive::Fill(_) => {}
            WorldPrimitive::Text(text) => {
                if !builder.opts.text {
                    continue;
                }
                let value = match &text.content {
                    cadcraft_doc::WorldTextContent::Text(t) => t.value.as_str(),
                    cadcraft_doc::WorldTextContent::MText(t) => t.contents.as_str(),
                };
                match super::layout_world_text(context.d, text, value)? {
                    super::WorldTextLayout::Text(shaped) => vertices.extend(shaped.strokes.iter().flatten().copied()),
                    super::WorldTextLayout::MText(pieces) => {
                        vertices.extend(pieces.iter().flat_map(|p| p.shaped.strokes.iter().flatten().copied()));
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let flat = Ctx { xf: Mat3::IDENTITY, ..context.clone() };
    for v in vertices.into_iter().filter(|v| seen.insert((v.x.to_bits(), v.y.to_bits(), v.z.to_bits()))) {
        builder.polyline(&flat, color, lineweight, &[v.xy(), (v + offset).xy()]);
    }
    Ok(())
}

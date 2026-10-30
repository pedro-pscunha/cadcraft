//! World-geometry builders for the canonical families: the WCS primitives of a payload, its plane and a parent transform.
//!
//! A builder takes the canonical payload, the normal of its plane (when the payload has one beside it) and the parent
//! transform, and returns the primitives of `WorldGeometry`'s vocabulary after the complete parent. It refuses (an
//! error, no fallback) a zero or non-finite normal, a payload that is not canonical, a perspective parent, and for texts
//! and patterns a parent that is not a similarity. Projection to the view happens only after this, in
//! `WorldGeometry::project_xy`.

use cadcraft_geom::{Mat4, OcsFrame, Spline, Vec2, Vec3, bulge_to_arc};

use super::block::Attrib;
use super::curves::{Arc, Circle, LwPolyline, Solid};
use super::dim::Leader;
use super::fill::{Gradient, Hatch, HatchEdge, HatchLoop, Wipeout};
use super::helix::Helix;
use super::layout::Table;
use super::mline::MLine;
use super::text::{MText, Text};
use super::text_canonical::TextFrame;
use super::world::{WorldFill, WorldFillKind, WorldGradient, WorldPattern, WorldPrimitive, WorldText, WorldTextContent};
use crate::gdt::{GdtLayout, GdtPrim};
use crate::{DocError, Result, Tolerance};

fn frame(normal: Vec3) -> Result<OcsFrame> {
    OcsFrame::new(normal).ok_or_else(|| DocError::Invalid("A canonical curve needs a plane with a finite, non-zero normal.".into()))
}

fn affine(parent: &Mat4) -> Result<()> {
    if parent.m[3] != [0.0, 0.0, 0.0, 1.0] || !parent.m.iter().flatten().all(|v| v.is_finite()) {
        return Err(DocError::Invalid("A world geometry parent must be a finite affine transform.".into()));
    }
    Ok(())
}

/// The edges of a closed loop of points.
fn closed(points: &[Vec3]) -> Vec<WorldPrimitive> {
    points.iter().enumerate().map(|(i, a)| WorldPrimitive::Line { a: *a, b: points[(i + 1) % points.len()] }).collect()
}

/// The outline of a canonical wipeout: its clip polygon (or the rectangle of a rectangular clip), closed.
pub fn wipeout_primitives(wipeout: &Wipeout, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let mut points = wipeout.world_boundary()?;
    // A polygonal clip repeats its first vertex at the end.
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let points: Vec<Vec3> = points.into_iter().map(|p| parent.apply(p)).collect();
    if points.len() < 2 {
        return Err(DocError::Invalid("A wipeout outline needs at least two vertices.".into()));
    }
    Ok(closed(&points))
}

/// The outline of a solid or trace in the DXF drawing order of its corners (1, 2, 4, 3); a triangle (the fourth corner
/// repeats the third) has three edges.
pub fn solid_primitives(solid: &Solid, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let [c0, c1, c2, c3] = solid.world_corners(normal)?.map(|c| parent.apply(c));
    Ok(if c3 == c2 { closed(&[c0, c1, c2]) } else { closed(&[c0, c1, c3, c2]) })
}

/// An arc or a line between two OCS points: a bulge makes a conic arc in the plane of `f`.
fn segment(f: OcsFrame, a: Vec2, b: Vec2, bulge: f64, z: f64) -> WorldPrimitive {
    match bulge_to_arc(a, b, bulge) {
        Some((arc, _ccw)) => WorldPrimitive::Arc {
            center: f.to_world(arc.center.to3(z)),
            x_axis: f.to_world(Vec3::new(arc.radius, 0.0, 0.0)),
            y_axis: f.to_world(Vec3::new(0.0, arc.radius, 0.0)),
            start: arc.start,
            end: arc.end,
        },
        None => WorldPrimitive::Line { a: f.to_world(a.to3(z)), b: f.to_world(b.to3(z)) },
    }
}

fn finish(primitives: Vec<WorldPrimitive>, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    primitives.iter().map(|p| p.transformed(parent)).collect()
}

/// The conic arc of a circle or arc: radii along the OCS axes of the plane.
fn round(f: OcsFrame, center: Vec3, radius: f64, start: f64, end: f64, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    finish(
        vec![WorldPrimitive::Arc {
            center: f.to_world(center),
            x_axis: f.to_world(Vec3::new(radius, 0.0, 0.0)),
            y_axis: f.to_world(Vec3::new(0.0, radius, 0.0)),
            start,
            end,
        }],
        parent,
    )
}

/// A canonical circle: a full conic arc.
pub fn circle_primitives(circle: &Circle, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    round(frame(normal)?, circle.center, circle.radius, 0.0, std::f64::consts::TAU, parent)
}

/// A canonical arc, counter-clockwise from its start angle to its end angle.
pub fn arc_primitives(arc: &Arc, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    // An end angle at or before the start runs on to the next turn; equal angles are a full circle as in the DXF.
    let end = arc.start + cadcraft_geom::ccw_sweep(arc.start, arc.end);
    round(frame(normal)?, arc.center, arc.radius, arc.start, end, parent)
}

/// The wide strip of a polyline segment from `a` to `b` with the full widths `wa` and `wb`, as a filled loop in the OCS
/// plane. A straight strip is exact; an arc strip follows the taper of its width at 5 degree steps.
fn strip(f: OcsFrame, a: Vec2, b: Vec2, bulge: f64, wa: f64, wb: f64, z: f64) -> Option<WorldPrimitive> {
    if wa <= 0.0 && wb <= 0.0 {
        return None;
    }
    let point = |p: Vec2| f.to_world(p.to3(z));
    let mut left = Vec::new();
    let mut right = Vec::new();
    match bulge_to_arc(a, b, bulge) {
        None => {
            let n = (b - a).normalized().perp();
            left = vec![a + n * (wa / 2.0), b + n * (wb / 2.0)];
            right = vec![a - n * (wa / 2.0), b - n * (wb / 2.0)];
        }
        Some((arc, ccw)) => {
            let sweep = arc.sweep();
            let steps = ((sweep.to_degrees() / 5.0).ceil() as usize).max(2);
            // From the polyline vertex `a` towards `b`: along the arc counter-clockwise, or against it.
            let (from, direction) = if ccw { (arc.start, 1.0) } else { (arc.end, -1.0) };
            for i in 0..=steps {
                let t = i as f64 / steps as f64;
                let angle = from + direction * sweep * t;
                let radial = Vec2::new(angle.cos(), angle.sin());
                let half = (wa + (wb - wa) * t) / 2.0;
                let center_point = arc.center + radial * arc.radius;
                left.push(center_point + radial * half);
                right.push(center_point - radial * half);
            }
        }
    }
    let mut ring: Vec<Vec2> = left;
    ring.extend(right.into_iter().rev());
    let loop_: Vec<WorldPrimitive> =
        (0..ring.len()).map(|i| WorldPrimitive::Line { a: point(ring[i]), b: point(ring[(i + 1) % ring.len()]) }).collect();
    Some(WorldPrimitive::Fill(WorldFill { loops: vec![loop_], kind: WorldFillKind::Solid, style: 0 }))
}

/// A canonical 2D polyline: its segments (lines and bulge arcs) with the elevation, closed when the polyline is, and a
/// solid fill for each segment that has a width. Only the widths of a bulge segment are sampled; everything else is exact.
pub fn lwpolyline_primitives(polyline: &LwPolyline, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let f = frame(normal)?;
    let n = polyline.vertices.len();
    let segments = if polyline.closed { n } else { n.saturating_sub(1) };
    let z = polyline.elevation;
    let mut out = Vec::new();
    for i in 0..segments {
        let (v, w) = (&polyline.vertices[i], &polyline.vertices[(i + 1) % n]);
        out.push(segment(f, v.p, w.p, v.bulge, z));
        let (wa, wb) = if polyline.const_width > 0.0 { (polyline.const_width, polyline.const_width) } else { (v.start_width, v.end_width) };
        out.extend(strip(f, v.p, w.p, v.bulge, wa, wb, z));
    }
    finish(out, parent)
}

/// An arrowhead laid out at the origin pointing along +X (the shape the renderer draws for the style's leader arrow
/// block): outline polylines and filled triangles (three points each), in the local coordinates of the leader's first
/// segment, where x runs out of the leader along that segment and y is the plane normal crossed into x.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaderArrowLayout {
    pub lines: Vec<Vec<Vec2>>,
    pub tris: Vec<Vec2>,
}

/// A canonical leader: the segments through its WCS vertices (as the renderer draws them today, whatever its path type)
/// and, when it has an arrow and two distinct first vertices, its arrowhead placed at the first vertex.
pub fn leader_primitives(leader: &Leader, normal: Vec3, arrow: &LeaderArrowLayout, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    leader.canonical_frame()?;
    let unit_normal = frame(normal)?.to_world(Vec3::Z);
    let mut out: Vec<WorldPrimitive> = leader.vertices.windows(2).map(|w| WorldPrimitive::Line { a: w[0], b: w[1] }).collect();
    if let (true, Some(&tip), Some(&next)) = (leader.arrow, leader.vertices.first(), leader.vertices.get(1)) {
        let out_of_leader = tip - next;
        if out_of_leader.len() > 1e-12 {
            let x_axis = out_of_leader.normalized();
            let arrow_frame = TextFrame { origin: tip, x_axis, y_axis: unit_normal.cross(x_axis), normal: unit_normal };
            for line in &arrow.lines {
                out.extend(line.windows(2).map(|w| WorldPrimitive::Line { a: local(&arrow_frame, w[0]), b: local(&arrow_frame, w[1]) }));
            }
            for tri in arrow.tris.as_chunks::<3>().0 {
                out.push(WorldPrimitive::Fill(WorldFill { loops: vec![local_loop(&arrow_frame, tri)], kind: WorldFillKind::Solid, style: 0 }));
            }
        }
    }
    finish(out, parent)
}

/// A canonical spatial spline: its rational B-spline in WCS, from the control points and weights it holds. A planar
/// payload (no Z values) has no spatial control points and is refused.
pub fn spline_primitives(spline: &Spline, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let control = spline.control3().ok_or_else(|| DocError::Invalid("A canonical spline needs its Z values.".into()))?;
    finish(vec![WorldPrimitive::Spline { degree: spline.degree, knots: spline.knots.clone(), control, weights: spline.weights.clone() }], parent)
}

/// A canonical helix: its rational B-spline in WCS.
pub fn helix_primitives(helix: &Helix, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let (spline, control) = helix.world_spline3()?;
    finish(vec![WorldPrimitive::Spline { degree: spline.degree, knots: spline.knots, control, weights: spline.weights }], parent)
}

/// One boundary edge of a canonical hatch loop in WCS, in the plane of `f` at the elevation `z`.
fn hatch_edge(f: OcsFrame, edge: &HatchEdge, z: f64) -> Result<Vec<WorldPrimitive>> {
    let at = |p: Vec2| f.to_world(p.to3(z));
    let vector = |v: Vec2| f.to_world(v.to3(0.0));
    Ok(match edge {
        HatchEdge::Line { a, b } => vec![WorldPrimitive::Line { a: at(*a), b: at(*b) }],
        HatchEdge::Arc { center, radius, start, end, ccw } => {
            // A clockwise edge holds the angles negated: the same arc, counter-clockwise from -end to -start.
            let (start, end) = if *ccw { (*start, *end) } else { (-end, -start) };
            vec![WorldPrimitive::Arc {
                center: at(*center),
                x_axis: vector(Vec2::new(*radius, 0.0)),
                y_axis: vector(Vec2::new(0.0, *radius)),
                start,
                end,
            }]
        }
        HatchEdge::Ellipse { center, major, ratio, start, end, ccw } => {
            let (start, end) = if *ccw { (*start, *end) } else { (-end, -start) };
            vec![WorldPrimitive::Arc { center: at(*center), x_axis: vector(*major), y_axis: vector(major.perp() * *ratio), start, end }]
        }
        HatchEdge::Spline(sp) => {
            if sp.control.is_empty() {
                return Err(DocError::Invalid("A hatch spline edge with fit points only has no control polygon in the world geometry.".into()));
            }
            let weights = if sp.rational { sp.weights.clone() } else { Vec::new() };
            vec![WorldPrimitive::Spline {
                degree: sp.degree as usize,
                knots: sp.knots.clone(),
                control: sp.control.iter().map(|c| at(*c)).collect(),
                weights,
            }]
        }
        HatchEdge::Polyline { vertices, closed } => {
            let n = vertices.len();
            let count = if *closed { n } else { n.saturating_sub(1) };
            (0..count).map(|i| segment(f, vertices[i].p, vertices[(i + 1) % n].p, vertices[i].bulge, z)).collect()
        }
    })
}

/// A canonical hatch: a fill of its loops (solid, pattern or gradient) in the plane of `normal` at the hatch elevation.
pub fn hatch_primitives(hatch: &Hatch, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let f = frame(normal)?;
    let z = hatch.elevation;
    if hatch.loops.is_empty() || hatch.loops.iter().any(|l: &HatchLoop| l.edges.is_empty()) {
        return Err(DocError::Invalid("A canonical hatch needs exact edges in every loop.".into()));
    }
    let loops = hatch
        .loops
        .iter()
        .map(|l| l.edges.iter().map(|e| hatch_edge(f, e, z)).collect::<Result<Vec<_>>>().map(|v| v.into_iter().flatten().collect::<Vec<_>>()))
        .collect::<Result<Vec<_>>>()?;
    let (origin, x_axis, y_axis) = (f.to_world(hatch.origin.to3(z)), f.to_world(Vec3::new(1.0, 0.0, 0.0)), f.to_world(Vec3::new(0.0, 1.0, 0.0)));
    let kind = if let Some(g) = &hatch.gradient {
        WorldFillKind::Gradient(WorldGradient { gradient: Gradient { angle: g.angle, ..g.clone() }, origin, x_axis, y_axis })
    } else if hatch.solid {
        WorldFillKind::Solid
    } else {
        WorldFillKind::Pattern(WorldPattern {
            name: hatch.pattern.clone(),
            scale: hatch.scale,
            angle: hatch.angle,
            origin,
            x_axis,
            y_axis,
            lines: hatch.pattern_lines.clone(),
        })
    };
    finish(vec![WorldPrimitive::Fill(WorldFill { loops, kind, style: hatch.style })], parent)
}

/// A canonical TEXT (or ATTDEF) in the plane of `normal`; an attribute instance uses [`attrib_primitives`].
pub fn text_primitives(text: &Text, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let primitive = WorldPrimitive::Text(WorldText {
        frame: text.world_frame(normal)?,
        align: text.world_align(normal)?,
        content: WorldTextContent::Text(text.clone()),
    });
    finish(vec![primitive], parent)
}

/// A canonical MTEXT in the plane of `normal`.
pub fn mtext_primitives(text: &MText, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let primitive = WorldPrimitive::Text(WorldText { frame: text.world_frame(normal)?, align: None, content: WorldTextContent::MText(text.clone()) });
    finish(vec![primitive], parent)
}

/// A canonical attribute instance: its text in its own plane, and its multiline text in the plane of its own.
pub fn attrib_primitives(attrib: &Attrib, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let mut out = text_primitives(&attrib.text, attrib.plane.normal, parent)?;
    if let Some(m) = &attrib.multiline {
        out.extend(mtext_primitives(&m.mtext, m.normal, parent)?);
    }
    Ok(out)
}

/// The WCS point of the local 2D point `p` of a flat annotation frame (x along the frame, y up the plane).
fn local(frame: &TextFrame, p: Vec2) -> Vec3 {
    frame.origin + frame.x_axis * p.x + frame.y_axis * p.y
}

/// A text of a flat annotation at the local point `at`, attached like MTEXT (1 top left to 9 bottom right).
fn local_text(frame: &TextFrame, at: Vec2, attach: u8, height: f64, style: &str, value: &str) -> WorldPrimitive {
    let origin = local(frame, at);
    let content = MText {
        insert: origin,
        height,
        width: 0.0,
        attach,
        rotation: 0.0,
        style: style.into(),
        contents: value.into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(frame.x_axis),
    };
    WorldPrimitive::Text(WorldText { frame: TextFrame { origin, ..*frame }, align: None, content: WorldTextContent::MText(content) })
}

fn local_loop(frame: &TextFrame, points: &[Vec2]) -> Vec<WorldPrimitive> {
    (0..points.len()).map(|i| WorldPrimitive::Line { a: local(frame, points[i]), b: local(frame, points[(i + 1) % points.len()]) }).collect()
}

/// A canonical tolerance: the frame boxes and symbols of `layout` (laid out at the origin with no rotation, in the local
/// coordinates of the frame: the layout of `Tolerance::layout` for a tolerance at (0, 0) turned by 0) placed on the
/// tolerance's WCS frame, and its texts set in `text_style`.
pub fn tolerance_primitives(tolerance: &Tolerance, normal: Vec3, layout: &GdtLayout, text_style: &str, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let frame = tolerance.world_frame(normal)?;
    let mut out = Vec::new();
    for prim in layout.frame.iter().chain(&layout.symbols) {
        match prim {
            GdtPrim::Line(points) => out.extend(points.windows(2).map(|w| WorldPrimitive::Line { a: local(&frame, w[0]), b: local(&frame, w[1]) })),
            GdtPrim::Circle { center, radius } => out.push(WorldPrimitive::Arc {
                center: local(&frame, *center),
                x_axis: frame.x_axis * *radius,
                y_axis: frame.y_axis * *radius,
                start: 0.0,
                end: std::f64::consts::TAU,
            }),
            GdtPrim::Arc { center, radius, start, end } => out.push(WorldPrimitive::Arc {
                center: local(&frame, *center),
                x_axis: frame.x_axis * *radius,
                y_axis: frame.y_axis * *radius,
                start: *start,
                end: *start + cadcraft_geom::ccw_sweep(*start, *end),
            }),
            GdtPrim::Fill(points) => {
                out.push(WorldPrimitive::Fill(WorldFill { loops: vec![local_loop(&frame, points)], kind: WorldFillKind::Solid, style: 0 }));
            }
        }
    }
    // A layout text is a left baseline point, so attach at the bottom left (7).
    out.extend(layout.texts.iter().map(|t| local_text(&frame, t.pos, 7, t.height, text_style, &t.value)));
    finish(out, parent)
}

/// A table's layout in its local coordinates: the origin is the insertion point (the top left corner), x runs along the
/// table and y up the plane, so rows have negative y. The renderer lays it out for a table at the origin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableLayout {
    /// Cell backgrounds: corners in order round the cell.
    pub fills: Vec<[Vec2; 4]>,
    pub lines: Vec<[Vec2; 2]>,
    pub texts: Vec<TableLayoutText>,
}

/// A cell text at its attachment point.
#[derive(Clone, Debug, PartialEq)]
pub struct TableLayoutText {
    pub at: Vec2,
    /// MTEXT attachment (1 top left to 9 bottom right).
    pub attach: u8,
    pub height: f64,
    pub style: String,
    pub text: String,
}

/// A canonical table: its cell backgrounds, grid lines and cell texts placed on the WCS frame of the table.
pub fn table_primitives(table: &Table, normal: Vec3, layout: &TableLayout, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    let frame = table.world_frame(normal)?;
    let mut out: Vec<WorldPrimitive> = layout
        .fills
        .iter()
        .map(|corners| WorldPrimitive::Fill(WorldFill { loops: vec![local_loop(&frame, corners)], kind: WorldFillKind::Solid, style: 0 }))
        .collect();
    out.extend(layout.lines.iter().map(|[a, b]| WorldPrimitive::Line { a: local(&frame, *a), b: local(&frame, *b) }));
    out.extend(layout.texts.iter().map(|t| local_text(&frame, t.at, t.attach, t.height, &t.style, &t.text)));
    finish(out, parent)
}

/// The pieces of a multiline element on one segment: `list` is the record's parameters for it (its distance along the
/// miter, then where it starts and the break pairs), `len` the length of the element line.
fn element_pieces(list: &[f64], len: f64) -> Vec<(f64, f64)> {
    let Some(&start) = list.get(1) else { return Vec::new() };
    let rest = list.get(2..).unwrap_or(&[]);
    let (mut out, mut from) = (Vec::new(), start);
    let mut push = |a: f64, b: f64| {
        let (a, b) = (a.max(0.0), b.min(len));
        if b > a + 1e-12 {
            out.push((a, b));
        }
    };
    for pair in rest.chunks(2) {
        match pair {
            [stop, restart] => {
                push(from, start + stop);
                from = start + restart;
            }
            [stop] => {
                push(from, start + stop);
                return out;
            }
            _ => {}
        }
    }
    push(from, len);
    out
}

/// A canonical multiline: its element pieces (breaks taken out), caps, joints and fill, from the vertices, miters and
/// element parameters it holds. The style's caps, joints and fill switches come from `MLine::def`.
pub fn mline_primitives(mline: &MLine, normal: Vec3, parent: &Mat4) -> Result<Vec<WorldPrimitive>> {
    affine(parent)?;
    let frames = mline.canonical_frames()?;
    let unit_normal = frame(normal)?.to_world(Vec3::Z);
    let n = mline.vertices.len();
    let nseg = mline.segment_count();
    let elements = frames[0].params.len();
    let unit = |v: Vec3| if v.len() > 0.0 { v.normalized() } else { Vec3::ZERO };
    let at = |k: usize, e: usize| {
        let (v, f) = (&mline.vertices[k % n], frames[k % n]);
        v.p + unit(f.miter) * f.params[e].first().copied().unwrap_or(0.0)
    };
    let mut out = Vec::new();
    for (k, vertex_frame) in frames.iter().enumerate().take(nseg) {
        for e in 0..elements {
            let (a, b) = (at(k, e), at(k + 1, e));
            let len = (b - a).len();
            if len <= 1e-12 {
                continue;
            }
            let dir = (b - a) * (1.0 / len);
            for (s, t) in element_pieces(&vertex_frame.params[e], len) {
                out.push(WorldPrimitive::Line { a: a + dir * s, b: a + dir * t });
            }
        }
    }
    // The elements by offset, the leftmost (largest) first, as the 2D geometry orders them.
    let offsets = mline.offsets();
    let mut order: Vec<usize> = (0..elements).collect();
    order.sort_by(|a, b| offsets.get(*b).copied().unwrap_or(0.0).total_cmp(&offsets.get(*a).copied().unwrap_or(0.0)));
    if let (Some(&top), Some(&bottom)) = (order.first(), order.last()) {
        let across = |k: usize| WorldPrimitive::Line { a: at(k, top), b: at(k, bottom) };
        if !mline.closed {
            for (k, caps, skip, start) in
                [(0, mline.def.start_caps, mline.no_start_caps, true), (n - 1, mline.def.end_caps, mline.no_end_caps, false)]
            {
                if skip {
                    continue;
                }
                if caps.line && order.len() > 1 {
                    out.push(across(k));
                }
                for i in 0..order.len() / 2 {
                    if (i == 0 && !caps.outer_arc) || (i > 0 && !caps.inner_arcs) {
                        continue;
                    }
                    let (a, b) = (at(k, order[i]), at(k, order[order.len() - 1 - i]));
                    let r = (b - a).len() / 2.0;
                    if r <= 1e-12 {
                        continue;
                    }
                    // A half circle from one outer element to the other, behind the start or ahead of the end.
                    let (c, from) = ((a + b) * 0.5, if start { a } else { b });
                    let x_axis = from - c;
                    out.push(WorldPrimitive::Arc { center: c, x_axis, y_axis: unit_normal.cross(x_axis), start: 0.0, end: std::f64::consts::PI });
                }
            }
        }
        if mline.def.joints && order.len() > 1 {
            let inner = if mline.closed { 0..n } else { 1..n.saturating_sub(1) };
            out.extend(inner.map(across));
        }
        if mline.def.fill.is_some() {
            let side = |e: usize| (0..n).map(|k| at(k, e)).collect::<Vec<Vec3>>();
            let ring = |points: Vec<Vec3>| -> Vec<WorldPrimitive> {
                (0..points.len()).map(|i| WorldPrimitive::Line { a: points[i], b: points[(i + 1) % points.len()] }).collect()
            };
            let loops = if mline.closed {
                vec![ring(side(top)), ring(side(bottom))]
            } else {
                let mut polygon = side(top);
                polygon.extend(side(bottom).into_iter().rev());
                vec![ring(polygon)]
            };
            out.push(WorldPrimitive::Fill(WorldFill { loops, kind: WorldFillKind::Solid, style: 0 }));
        }
    }
    finish(out, parent)
}

#[cfg(test)]
mod tests {
    use cadcraft_geom::{PolyVertex, Vec2};

    use super::super::fill::WipeoutFrame;
    use super::*;

    const EX: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };
    const EY: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    fn lines(p: &[WorldPrimitive]) -> Vec<(Vec3, Vec3)> {
        p.iter()
            .filter_map(|p| match p {
                WorldPrimitive::Line { a, b } => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    /// A parent that moves and turns about Z by a quarter turn: (x, y, z) -> (-y + 10, x - 5, z + 1).
    fn parent() -> Mat4 {
        Mat4 { m: [[0.0, -1.0, 0.0, 10.0], [1.0, 0.0, 0.0, -5.0], [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]] }
    }

    /// The arbitrary axis algorithm written out by hand for the three planes: +Z, -Z and (1, 0, 0) (tilted).
    fn expected_world(plane: usize, p: Vec3) -> Vec3 {
        match plane {
            0 => p,
            1 => Vec3::new(-p.x, p.y, -p.z),
            _ => Vec3::new(p.z, p.x, p.y),
        }
    }

    const NORMALS: [Vec3; 3] = [Vec3 { x: 0.0, y: 0.0, z: 1.0 }, Vec3 { x: 0.0, y: 0.0, z: -2.0 }, Vec3 { x: 3.0, y: 0.0, z: 0.0 }];

    #[test]
    fn a_solid_outline_follows_the_dxf_corner_order_in_every_plane() {
        let corners = [Vec3::new(0.0, 0.0, 2.0), Vec3::new(4.0, 0.0, 2.0), Vec3::new(0.0, 3.0, 2.0), Vec3::new(4.0, 3.0, 2.0)];
        for (plane, normal) in NORMALS.iter().enumerate() {
            let got = lines(&solid_primitives(&Solid { corners }, *normal, &parent()).unwrap());
            let want: Vec<Vec3> = [0, 1, 3, 2].iter().map(|i| parent().apply(expected_world(plane, corners[*i]))).collect();
            assert_eq!(got.len(), 4);
            for (i, (a, b)) in got.iter().enumerate() {
                assert!(near(*a, want[i]) && near(*b, want[(i + 1) % 4]), "plane {plane} edge {i}: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_triangle_has_three_edges() {
        let t = Solid { corners: [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)] };
        assert_eq!(solid_primitives(&t, Vec3::Z, &Mat4::IDENTITY).unwrap().len(), 3);
    }

    fn polyline(closed: bool, bulge: f64) -> LwPolyline {
        LwPolyline {
            records: Default::default(),
            vertices: vec![
                PolyVertex::with_bulge(Vec2::new(0.0, 0.0), bulge),
                PolyVertex::new(Vec2::new(3.0, 0.0)),
                PolyVertex::new(Vec2::new(3.0, 2.0)),
            ],
            closed,
            const_width: 0.0,
            elevation: 1.5,
            plinegen: false,
            heavy: false,
            fit: None,
        }
    }

    #[test]
    fn a_polyline_has_its_segments_in_every_plane_with_the_elevation() {
        for (plane, normal) in NORMALS.iter().enumerate() {
            for (closed, count) in [(false, 2), (true, 3)] {
                let got = lines(&lwpolyline_primitives(&polyline(closed, 0.0), *normal, &parent()).unwrap());
                assert_eq!(got.len(), count);
                let pts = [Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, 2.0)];
                let at = |i: usize| parent().apply(expected_world(plane, pts[i % 3].to3(1.5)));
                for (i, (a, b)) in got.iter().enumerate() {
                    assert!(near(*a, at(i)) && near(*b, at(i + 1)), "plane {plane} closed {closed} segment {i}");
                }
            }
        }
    }

    #[test]
    fn a_bad_normal_or_a_perspective_parent_refuses() {
        assert!(lwpolyline_primitives(&polyline(false, 0.0), Vec3::ZERO, &Mat4::IDENTITY).is_err());
        let mut perspective = Mat4::IDENTITY;
        perspective.m[3][0] = 0.1;
        assert!(lwpolyline_primitives(&polyline(false, 0.0), Vec3::Z, &perspective).is_err());
        let circle = Circle { center: Vec3::ZERO, radius: 1.0 };
        assert!(circle_primitives(&circle, Vec3::new(f64::NAN, 0.0, 1.0), &Mat4::IDENTITY).is_err());
        assert!(circle_primitives(&circle, Vec3::Z, &perspective).is_err());
    }

    /// The point of a conic arc primitive at the parameter `t`.
    fn arc_at(p: &WorldPrimitive, t: f64) -> Vec3 {
        match p {
            WorldPrimitive::Arc { center, x_axis, y_axis, .. } => *center + *x_axis * t.cos() + *y_axis * t.sin(),
            other => panic!("not an arc: {other:?}"),
        }
    }

    fn arc_range(p: &WorldPrimitive) -> (f64, f64) {
        match p {
            WorldPrimitive::Arc { start, end, .. } => (*start, *end),
            other => panic!("not an arc: {other:?}"),
        }
    }

    #[test]
    fn a_circle_and_an_arc_lie_where_the_hand_written_plane_puts_them() {
        let (center, r) = (Vec3::new(1.0, 2.0, 3.0), 2.0);
        for (plane, normal) in NORMALS.iter().enumerate() {
            let circle = circle_primitives(&Circle { center, radius: r }, *normal, &parent()).unwrap();
            for t in [0.0, 1.0, 2.5, 4.0] {
                let local = center + Vec3::new(r * f64::cos(t), r * f64::sin(t), 0.0);
                assert!(near(arc_at(&circle[0], t), parent().apply(expected_world(plane, local))), "plane {plane} circle t = {t}");
            }
            assert_eq!(arc_range(&circle[0]), (0.0, std::f64::consts::TAU));
            let arc = arc_primitives(&Arc { center, radius: r, start: 0.3, end: 2.0 }, *normal, &parent()).unwrap();
            for t in [0.3, 1.1, 2.0] {
                let local = center + Vec3::new(r * f64::cos(t), r * f64::sin(t), 0.0);
                assert!(near(arc_at(&arc[0], t), parent().apply(expected_world(plane, local))), "plane {plane} arc t = {t}");
            }
            assert_eq!(arc_range(&arc[0]), (0.3, 2.0));
        }
        // Equal angles are a full circle.
        let full = arc_primitives(&Arc { center, radius: r, start: 0.5, end: 0.5 }, Vec3::Z, &Mat4::IDENTITY).unwrap();
        assert!((arc_range(&full[0]).1 - arc_range(&full[0]).0 - std::f64::consts::TAU).abs() < 1e-12);
    }

    #[test]
    fn a_polyline_bulge_is_an_arc_through_the_chord_ends_and_the_sagitta() {
        // A semicircle from (0,0) to (2,0), counter-clockwise: it passes through (1,-1).
        let pl = LwPolyline {
            vertices: vec![PolyVertex::with_bulge(Vec2::new(0.0, 0.0), 1.0), PolyVertex::new(Vec2::new(2.0, 0.0))],
            elevation: 1.5,
            ..polyline(false, 0.0)
        };
        for (plane, normal) in NORMALS.iter().enumerate() {
            let got = lwpolyline_primitives(&pl, *normal, &parent()).unwrap();
            assert_eq!(got.len(), 1);
            let (start, end) = arc_range(&got[0]);
            let sweep = cadcraft_geom::ccw_sweep(start, end);
            let at = |p: Vec3| parent().apply(expected_world(plane, p));
            assert!(near(arc_at(&got[0], start), at(Vec3::new(0.0, 0.0, 1.5))), "plane {plane} start");
            assert!(near(arc_at(&got[0], start + sweep), at(Vec3::new(2.0, 0.0, 1.5))), "plane {plane} end");
            assert!(near(arc_at(&got[0], start + sweep / 2.0), at(Vec3::new(1.0, -1.0, 1.5))), "plane {plane} middle");
        }
    }

    #[test]
    fn a_wide_segment_adds_a_solid_strip_and_a_constant_width_wins() {
        let mut pl = LwPolyline {
            vertices: vec![PolyVertex { p: Vec2::new(0.0, 0.0), bulge: 0.0, start_width: 1.0, end_width: 3.0 }, PolyVertex::new(Vec2::new(4.0, 0.0))],
            elevation: 0.0,
            ..polyline(false, 0.0)
        };
        let strip_corners = |pl: &LwPolyline, plane: usize, normal: Vec3| -> Vec<Vec3> {
            let got = lwpolyline_primitives(pl, normal, &parent()).unwrap();
            let WorldPrimitive::Fill(fill) = &got[1] else { panic!("no strip") };
            assert_eq!(fill.kind, WorldFillKind::Solid);
            let _ = plane;
            fill.loops[0].iter().map(|l| if let WorldPrimitive::Line { a, .. } = l { *a } else { panic!("line") }).collect()
        };
        for (plane, normal) in NORMALS.iter().enumerate() {
            let at = |x: f64, y: f64| parent().apply(expected_world(plane, Vec3::new(x, y, 0.0)));
            let corners = strip_corners(&pl, plane, *normal);
            for want in [at(0.0, 0.5), at(4.0, 1.5), at(4.0, -1.5), at(0.0, -0.5)] {
                assert!(corners.iter().any(|c| near(*c, want)), "plane {plane}: {want:?} in {corners:?}");
            }
        }
        pl.const_width = 2.0;
        let corners = strip_corners(&pl, 0, Vec3::Z);
        assert!(corners.iter().any(|c| near(*c, parent().apply(Vec3::new(4.0, 1.0, 0.0)))));
    }

    #[test]
    fn a_helix_is_a_spline_that_starts_at_the_start_point() {
        use super::super::helix::{Helix, HelixConstrain};
        let h = Helix {
            center: Vec3::new(1.0, 2.0, 3.0),
            base_radius: 2.0,
            top_radius: 2.0,
            height: 4.0,
            turns: 2.0,
            start_angle: 0.0,
            ccw: true,
            constrain: HelixConstrain::Height,
            frame: Some(super::super::helix::HelixFrame { axis: Vec3::new(2.0, 0.0, 0.0), start: Vec3::new(1.0, 4.0, 3.0) }),
        };
        let got = helix_primitives(&h, &parent()).unwrap();
        let WorldPrimitive::Spline { degree, knots, control, .. } = &got[0] else { panic!("not a spline") };
        assert_eq!(knots.len(), control.len() + degree + 1);
        assert!(near(control[0], parent().apply(Vec3::new(1.0, 4.0, 3.0))));
        assert!(near(*control.last().unwrap(), parent().apply(Vec3::new(5.0, 4.0, 3.0))));
        let mut no_axis = h.clone();
        no_axis.frame = None;
        assert!(helix_primitives(&no_axis, &parent()).is_err());
    }

    fn hatch(solid: bool, gradient: Option<Gradient>) -> Hatch {
        use super::super::fill::HatchSpline;
        let edges = vec![
            HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
            // Clockwise: the angles are negated, so it runs counter-clockwise from -1.5 to -0.5.
            HatchEdge::Arc { center: Vec2::new(4.0, 1.0), radius: 1.0, start: 0.5, end: 1.5, ccw: false },
            HatchEdge::Ellipse { center: Vec2::new(2.0, 3.0), major: Vec2::new(2.0, 0.0), ratio: 0.5, start: 0.0, end: 3.0, ccw: true },
            HatchEdge::Spline(HatchSpline {
                degree: 2,
                rational: true,
                periodic: false,
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                control: vec![Vec2::new(0.0, 3.0), Vec2::new(-1.0, 1.0), Vec2::new(0.0, 0.0)],
                weights: vec![1.0, 0.5, 1.0],
                fit: Vec::new(),
                start_tangent: Vec2::ZERO,
                end_tangent: Vec2::ZERO,
            }),
        ];
        Hatch {
            pattern: "ANSI31".into(),
            solid,
            loops: vec![HatchLoop { flags: 1, edges, outer: true, ..HatchLoop::default() }],
            scale: 1.5,
            angle: 0.25,
            associative: false,
            style: 1,
            elevation: 2.0,
            gradient,
            origin: Vec2::new(1.0, 1.0),
            background: None,
            pattern_lines: Vec::new(),
        }
    }

    #[test]
    fn a_hatch_is_a_fill_of_exact_edges_in_its_plane() {
        for (plane, normal) in NORMALS.iter().enumerate() {
            let at = |x: f64, y: f64| parent().apply(expected_world(plane, Vec3::new(x, y, 2.0)));
            let got = hatch_primitives(&hatch(false, None), *normal, &parent()).unwrap();
            let WorldPrimitive::Fill(fill) = &got[0] else { panic!("not a fill") };
            assert_eq!(fill.style, 1);
            let l = &fill.loops[0];
            let WorldPrimitive::Line { a, b } = &l[0] else { panic!("line") };
            assert!(near(*a, at(0.0, 0.0)) && near(*b, at(4.0, 0.0)), "plane {plane}");
            // The clockwise arc runs counter-clockwise from -1.5 to -0.5 round (4, 1).
            let (s, e) = arc_range(&l[1]);
            assert!(
                near(arc_at(&l[1], s), at(4.0 + f64::cos(-1.5), 1.0 + f64::sin(-1.5)))
                    && near(arc_at(&l[1], e), at(4.0 + f64::cos(-0.5), 1.0 + f64::sin(-0.5)))
            );
            // The ellipse: center (2, 3), semi-axes 2 along X and 1 along Y.
            assert!(near(arc_at(&l[2], 0.0), at(4.0, 3.0)) && near(arc_at(&l[2], std::f64::consts::FRAC_PI_2), at(2.0, 4.0)));
            let WorldPrimitive::Spline { control, weights, .. } = &l[3] else { panic!("spline") };
            assert!(near(control[1], at(-1.0, 1.0)) && weights == &vec![1.0, 0.5, 1.0]);
            let WorldFillKind::Pattern(p) = &fill.kind else { panic!("pattern") };
            assert!(near(p.origin, at(1.0, 1.0)) && p.name == "ANSI31" && p.angle == 0.25 && p.scale == 1.5);
            assert!(near(p.x_axis, parent().apply_vec(expected_world(plane, Vec3::new(1.0, 0.0, 0.0)))));
            assert!(near(p.y_axis, parent().apply_vec(expected_world(plane, Vec3::new(0.0, 1.0, 0.0)))));
        }
        let solid = hatch_primitives(&hatch(true, None), Vec3::Z, &Mat4::IDENTITY).unwrap();
        assert!(matches!(&solid[0], WorldPrimitive::Fill(f) if f.kind == WorldFillKind::Solid));
        let g = Gradient {
            name: "LINEAR".into(),
            color1: cadcraft_color::Color::Index(1),
            color2: cadcraft_color::Color::Index(5),
            angle: 0.5,
            centered: false,
        };
        let gradient = hatch_primitives(&hatch(true, Some(g)), Vec3::Z, &Mat4::IDENTITY).unwrap();
        assert!(matches!(&gradient[0], WorldPrimitive::Fill(f) if matches!(f.kind, WorldFillKind::Gradient(_))));
        // A pattern needs a similarity; a solid fill does not.
        let shear = Mat4 { m: [[2.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(hatch_primitives(&hatch(false, None), Vec3::Z, &shear).is_err());
        assert!(hatch_primitives(&hatch(true, None), Vec3::Z, &shear).is_ok());
        let mut legacy = hatch(true, None);
        legacy.loops[0].edges.clear();
        assert!(hatch_primitives(&legacy, Vec3::Z, &Mat4::IDENTITY).is_err());
    }

    fn text_at(insert: Vec3, rotation: f64) -> Text {
        Text {
            insert,
            align_pt: None,
            height: 2.0,
            value: "T".into(),
            rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
            backward: false,
            upside_down: false,
        }
    }

    #[test]
    fn a_text_frame_follows_the_plane_and_the_similarity_parent() {
        // OCS axes written out by hand: +Z (X, Y, Z), -Z (-X, Y, -Z) and the (1, 0, 0) plane (Y, Z, X).
        let axes = [(EX, EY, Vec3::Z), (Vec3::new(-1.0, 0.0, 0.0), EY, Vec3::new(0.0, 0.0, -1.0)), (EY, Vec3::Z, EX)];
        for (plane, normal) in NORMALS.iter().enumerate() {
            let got = text_primitives(&text_at(Vec3::new(1.0, 2.0, 3.0), 0.0), *normal, &parent()).unwrap();
            let WorldPrimitive::Text(t) = &got[0] else { panic!("not a text") };
            assert!(near(t.frame.origin, parent().apply(expected_world(plane, Vec3::new(1.0, 2.0, 3.0)))), "plane {plane}");
            let (x, y, n) = axes[plane];
            assert!(near(t.frame.x_axis, parent().apply_vec(x)), "plane {plane} x axis {:?}", t.frame.x_axis);
            assert!(near(t.frame.y_axis, parent().apply_vec(y)) && near(t.frame.normal, parent().apply_vec(n)), "plane {plane}");
            let WorldTextContent::Text(content) = &t.content else { panic!("content") };
            assert_eq!(content.height, 2.0, "this parent does not scale");
        }
        let shear = Mat4 { m: [[2.0, 0.5, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(text_primitives(&text_at(Vec3::ZERO, 0.0), Vec3::Z, &shear).is_err());
        assert!(text_primitives(&text_at(Vec3::ZERO, 0.0), Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }

    #[test]
    fn an_attribute_uses_its_own_plane_and_its_embedded_text_the_plane_of_its_own() {
        use super::super::block::MultilineAttrib;
        let mut a = Attrib::new("TAG", text_at(Vec3::new(1.0, 2.0, 3.0), 0.0));
        a.plane.normal = Vec3::new(3.0, 0.0, 0.0);
        let mtext = crate::MText {
            insert: Vec3::new(1.0, 1.0, 1.0),
            height: 1.0,
            width: 5.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "x".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: Some(Vec3::new(0.0, 0.0, 1.0)),
        };
        a.multiline = Some(MultilineAttrib { geometry_revision: Default::default(), normal: Vec3::new(0.0, 1.0, 0.0), mtext, read_as: "x".into() });
        let got = attrib_primitives(&a, &Mat4::IDENTITY).unwrap();
        assert_eq!(got.len(), 2);
        let (WorldPrimitive::Text(t), WorldPrimitive::Text(m)) = (&got[0], &got[1]) else { panic!("texts") };
        assert!(near(t.frame.origin, Vec3::new(3.0, 1.0, 2.0)) && near(t.frame.normal, EX), "(x, y, z) -> (z, x, y) in the (1, 0, 0) plane");
        // The embedded text: WCS insertion and direction as held, and its own normal.
        assert!(near(m.frame.origin, Vec3::new(1.0, 1.0, 1.0)) && near(m.frame.x_axis, Vec3::new(0.0, 0.0, 1.0)) && near(m.frame.normal, EY));
    }

    #[test]
    fn a_wipeout_outline_is_its_closed_clip_in_the_parent_frame() {
        let w = Wipeout {
            boundary: Vec::new(),
            frame: Some(WipeoutFrame {
                origin: Vec3::new(10.0, 0.0, 0.0),
                u: Vec3::new(4.0, 0.0, 0.0),
                v: Vec3::new(0.0, 0.0, 2.0),
                size: Vec2::new(1.0, 1.0),
                clip_type: 2,
                clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
            }),
        };
        let got = lines(&wipeout_primitives(&w, &parent()).unwrap());
        // Pixel (x, y) is at origin + u (x + 0.5) + v (0.5 - y): the four corners of a 4 by 2 vertical rectangle.
        let corner = |x: f64, z: f64| parent().apply(Vec3::new(10.0 + x, 0.0, z));
        let want = [corner(0.0, 2.0), corner(4.0, 2.0), corner(4.0, 0.0), corner(0.0, 0.0)];
        assert_eq!(got.len(), 4, "the repeated closing vertex adds no edge");
        for (i, (a, b)) in got.iter().enumerate() {
            assert!(near(*a, want[i]) && near(*b, want[(i + 1) % 4]), "edge {i}");
        }
        assert!(wipeout_primitives(&Wipeout { boundary: Vec::new(), frame: None }, &parent()).is_err());
    }

    /// The in-plane directions (x along the frame, y up the plane) and a direction for each of the three planes.
    fn frames() -> [(Vec3, Vec3, Vec3); 3] {
        // +Z: dir X, up Y. -Z: dir X, up (0, -1, 0). (1, 0, 0): dir Y, up Z.
        [(EX, EY, EX), (EX, Vec3::new(0.0, -1.0, 0.0), EX), (EY, Vec3::new(0.0, 0.0, 1.0), EY)]
    }

    #[test]
    fn a_tolerance_layout_is_placed_on_the_frame_of_its_plane() {
        use crate::gdt::GdtText;
        let layout = GdtLayout {
            frame: vec![GdtPrim::Line(vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, -2.0)])],
            symbols: vec![
                GdtPrim::Circle { center: Vec2::new(1.0, -1.0), radius: 0.5 },
                GdtPrim::Arc { center: Vec2::new(2.0, -1.0), radius: 0.25, start: 0.5, end: 1.5 },
                GdtPrim::Fill([Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)]),
            ],
            texts: vec![GdtText { pos: Vec2::new(0.5, -1.5), height: 0.18, rotation: 0.0, value: "0.1".into() }],
            bounds: Default::default(),
        };
        let insert = Vec3::new(1.0, 2.0, 3.0);
        for (plane, normal) in NORMALS.iter().enumerate() {
            let (xa, ya, dir) = frames()[plane];
            let t = Tolerance { insert, dir, style: "Standard".into(), text: "x".into() };
            let got = tolerance_primitives(&t, *normal, &layout, "Standard", &Mat4::IDENTITY).unwrap();
            let at = |x: f64, y: f64| insert + xa * x + ya * y;
            let WorldPrimitive::Line { a, b } = &got[1] else { panic!("line") };
            assert!(near(*a, at(4.0, 0.0)) && near(*b, at(4.0, -2.0)), "plane {plane}");
            let WorldPrimitive::Arc { center, x_axis, y_axis, .. } = &got[2] else { panic!("circle") };
            assert!(near(*center, at(1.0, -1.0)) && near(*x_axis, xa * 0.5) && near(*y_axis, ya * 0.5), "plane {plane}");
            assert!(near(arc_at(&got[3], 0.5), at(2.0 + 0.25 * f64::cos(0.5), -1.0 + 0.25 * f64::sin(0.5))), "plane {plane}");
            let WorldPrimitive::Fill(f) = &got[4] else { panic!("fill") };
            assert!(matches!(&f.loops[0][1], WorldPrimitive::Line { a, .. } if near(*a, at(1.0, 0.0))), "plane {plane}");
            let WorldPrimitive::Text(text) = &got[5] else { panic!("text") };
            assert!(near(text.frame.origin, at(0.5, -1.5)) && near(text.frame.x_axis, xa) && near(text.frame.y_axis, ya), "plane {plane}");
            let WorldTextContent::MText(m) = &text.content else { panic!("mtext") };
            assert_eq!((m.attach, m.height, m.contents.as_str()), (7, 0.18, "0.1"));
        }
        let off_plane = Tolerance { insert, dir: EX, style: "Standard".into(), text: "x".into() };
        assert!(tolerance_primitives(&off_plane, NORMALS[2], &layout, "Standard", &Mat4::IDENTITY).is_err(), "the direction must lie in the plane");
    }

    #[test]
    fn a_table_layout_is_placed_on_the_frame_of_its_plane() {
        let layout = TableLayout {
            fills: vec![[Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(3.0, -1.0), Vec2::new(0.0, -1.0)]],
            lines: vec![[Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)], [Vec2::new(0.0, -1.0), Vec2::new(3.0, -1.0)]],
            texts: vec![TableLayoutText { at: Vec2::new(0.1, -0.1), attach: 1, height: 0.2, style: "Standard".into(), text: "cell".into() }],
        };
        let insert = Vec3::new(1.0, 2.0, 3.0);
        for (plane, normal) in NORMALS.iter().enumerate() {
            let (xa, ya, dir) = frames()[plane];
            let table = Table { insert, dir, col_widths: vec![3.0], row_heights: vec![1.0], ..Table::default() };
            let got = table_primitives(&table, *normal, &layout, &parent()).unwrap();
            let at = |x: f64, y: f64| parent().apply(insert + xa * x + ya * y);
            let WorldPrimitive::Fill(f) = &got[0] else { panic!("fill") };
            assert!(matches!(&f.loops[0][2], WorldPrimitive::Line { a, .. } if near(*a, at(3.0, -1.0))), "plane {plane}");
            let WorldPrimitive::Line { a, b } = &got[2] else { panic!("line") };
            assert!(near(*a, at(0.0, -1.0)) && near(*b, at(3.0, -1.0)), "plane {plane}");
            let WorldPrimitive::Text(t) = &got[3] else { panic!("text") };
            assert!(near(t.frame.origin, at(0.1, -0.1)), "plane {plane}");
            assert!(near(t.frame.x_axis, parent().apply_vec(xa)) && near(t.frame.y_axis, parent().apply_vec(ya)), "plane {plane}");
        }
    }

    use crate::{MLineJust, MLineStyle, MLineVertex, MLineVertexFrame};

    /// A two-element multiline from (0,0,0) to (10,0,0) turned into the plane by `turn`, with `breaks` in the first element.
    fn mline(turn: impl Fn(Vec3) -> Vec3, breaks: &[f64]) -> MLine {
        let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 1.0);
        m.def.elements = vec![crate::MLineElement { offset: 1.0, ..Default::default() }, crate::MLineElement { offset: -1.0, ..Default::default() }];
        for x in [0.0, 10.0] {
            let mut first = vec![1.0, 0.0];
            first.extend_from_slice(breaks);
            let frame = MLineVertexFrame { dir: turn(EX), miter: turn(EY), params: vec![first, vec![-1.0, 0.0]] };
            let mut v = MLineVertex::new(turn(Vec3::new(x, 0.0, 0.0)));
            v.frame = Some(frame);
            m.vertices.push(v);
        }
        m
    }

    #[test]
    fn a_multiline_has_its_element_pieces_caps_joints_and_fill() {
        // A turn about X by 0.6 radians takes the XY plane to a tilted one; the plane normal turns with it.
        let (s, c) = 0.6f64.sin_cos();
        let turn = |v: Vec3| Vec3::new(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
        let normal = turn(Vec3::Z);
        let mut m = mline(turn, &[3.0, 5.0]);
        let got = mline_primitives(&m, normal, &parent()).unwrap();
        let p = |x: f64, y: f64| parent().apply(turn(Vec3::new(x, y, 0.0)));
        // Element 0 (y = +1) has a break from 3 to 5; element 1 (y = -1) is whole.
        let drawn = lines(&got);
        let want = [(p(0.0, 1.0), p(3.0, 1.0)), (p(5.0, 1.0), p(10.0, 1.0)), (p(0.0, -1.0), p(10.0, -1.0))];
        assert_eq!(drawn.len(), 3, "{drawn:?}");
        for (i, (a, b)) in want.iter().enumerate() {
            assert!(near(drawn[i].0, *a) && near(drawn[i].1, *b), "piece {i}: {:?}", drawn[i]);
        }
        // A cap line at the start, an outer arc at the end, and a fill.
        m.def.start_caps.line = true;
        m.def.end_caps.outer_arc = true;
        m.def.fill = Some(cadcraft_color::Color::Index(1));
        let got = mline_primitives(&m, normal, &parent()).unwrap();
        assert!(lines(&got).iter().any(|(a, b)| near(*a, p(0.0, 1.0)) && near(*b, p(0.0, -1.0))), "the start cap line");
        let arc = got.iter().find(|g| matches!(g, WorldPrimitive::Arc { .. })).expect("the end cap arc");
        // A half circle from the lower end point round the end: ahead of the multiline, through (11, 0).
        assert!(near(arc_at(arc, 0.0), p(10.0, -1.0)) && near(arc_at(arc, std::f64::consts::PI), p(10.0, 1.0)));
        assert!(near(arc_at(arc, std::f64::consts::FRAC_PI_2), p(11.0, 0.0)), "{:?}", arc_at(arc, std::f64::consts::FRAC_PI_2));
        let fill = got.iter().find_map(|g| if let WorldPrimitive::Fill(f) = g { Some(f) } else { None }).expect("a fill");
        assert_eq!(fill.loops.len(), 1);
        assert_eq!(fill.loops[0].len(), 4);
    }

    #[test]
    fn a_legacy_multiline_or_a_bad_normal_refuses() {
        let mut legacy = mline(|v| v, &[]);
        legacy.vertices[0].frame = None;
        assert!(mline_primitives(&legacy, Vec3::Z, &Mat4::IDENTITY).is_err());
        assert!(mline_primitives(&mline(|v| v, &[]), Vec3::ZERO, &Mat4::IDENTITY).is_err());
    }

    #[test]
    fn a_spatial_spline_is_a_world_spline_through_its_xyz_controls() {
        use cadcraft_geom::{FitOptions, SplineSpace};
        let sp = Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, 0.0)],
            weights: vec![1.0, 0.5, 1.0],
            fit: Vec::new(),
            closed: false,
            fit_opts: FitOptions::default(),
            space: Some(SplineSpace { control: vec![0.0, 4.0, 1.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 }),
        };
        let got = spline_primitives(&sp, &parent()).unwrap();
        let WorldPrimitive::Spline { degree, knots, control, weights } = &got[0] else { panic!("not a spline") };
        assert_eq!((*degree, knots.len(), weights.clone()), (2, 6, vec![1.0, 0.5, 1.0]));
        assert!(near(control[1], parent().apply(Vec3::new(1.0, 2.0, 4.0))) && near(control[2], parent().apply(Vec3::new(3.0, 0.0, 1.0))));
        assert!(spline_primitives(&Spline { space: None, ..sp.clone() }, &parent()).is_err());
        let mut perspective = Mat4::IDENTITY;
        perspective.m[3][0] = 0.1;
        assert!(spline_primitives(&sp, &perspective).is_err());
    }

    #[test]
    fn a_leader_has_its_segments_and_an_arrowhead_out_of_its_first_segment() {
        use super::super::dim::LeaderFrame;
        let leader = Leader {
            vertices: vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 2.0, 7.0), Vec3::new(4.0, 2.0, 7.0)],
            frame: Some(LeaderFrame { direction: EX, block_offset: Vec3::ZERO, annotation_offset: Vec3::ZERO, text_height: 0.0, text_width: 0.0 }),
            ..Leader::default()
        };
        // An arrow of length 1 and half width 0.2 pointing along +X from the origin tip, back along -X.
        let arrow = LeaderArrowLayout {
            lines: vec![vec![Vec2::new(-1.0, 0.2), Vec2::new(0.0, 0.0)]],
            tris: vec![Vec2::new(0.0, 0.0), Vec2::new(-1.0, 0.2), Vec2::new(-1.0, -0.2)],
        };
        let normal = Vec3::new(0.0, 3.0, 0.0);
        let got = leader_primitives(&leader, normal, &arrow, &parent()).unwrap();
        let drawn = lines(&got);
        // Two leader segments and the arrow outline.
        assert_eq!(drawn.len(), 3);
        assert!(near(drawn[0].0, parent().apply(Vec3::new(1.0, 2.0, 3.0))) && near(drawn[0].1, parent().apply(Vec3::new(1.0, 2.0, 7.0))));
        // The tip points out of the leader: from (1,2,7) to (1,2,3) is -Z, so x = -Z; the plane normal is +Y and y = Y x x = (-1, 0, 0) x ... written out by hand.
        let x = Vec3::new(0.0, 0.0, -1.0);
        let y = Vec3::new(0.0, 1.0, 0.0).cross(x);
        assert!(near(y, Vec3::new(-1.0, 0.0, 0.0)));
        let wing = Vec3::new(1.0, 2.0, 3.0) + x * -1.0 + y * 0.2;
        assert!(near(drawn[2].0, parent().apply(wing)) && near(drawn[2].1, parent().apply(Vec3::new(1.0, 2.0, 3.0))), "{:?}", drawn[2]);
        assert!(got.iter().any(|p| matches!(p, WorldPrimitive::Fill(_))));
        let no_arrow = Leader { arrow: false, ..leader.clone() };
        assert_eq!(leader_primitives(&no_arrow, normal, &arrow, &parent()).unwrap().len(), 2);
        assert!(leader_primitives(&Leader { frame: None, ..leader }, normal, &arrow, &parent()).is_err());
    }
}

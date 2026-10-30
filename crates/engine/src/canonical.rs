//! Canonical (`CanonicalV2`) entities for pick, selection and snap.
//!
//! Their geometry comes from the shared WCS resolver (`cadcraft_render::entity_world_geometry`), seen along Z: the XY
//! primitives the 2D machinery tests, the words of their texts, and the filled areas. The Z the 2D view loses is kept in
//! the WCS primitives, so a snap on a canonical object can say which WCS point it found. A legacy entity
//! (`LegacyProjectedV1`) never comes here: its explicit 2D path stays as it was.

use cadcraft_doc::{Drawing, Entity, GeometryRevision, Prim, WorldFillKind, WorldPrimitive, WorldTextContent};
use cadcraft_geom::{Mat4, Vec2, Vec3};

/// The words of one text in the XY view: the points of its strokes and the angle of its baseline.
pub(crate) struct TextWords {
    pub points: Vec<Vec2>,
    pub angle: f64,
}

/// A canonical entity in the XY view.
#[derive(Default)]
pub(crate) struct Flat {
    /// The 2D primitives of its lines, curves and fills.
    pub prims: Vec<Prim>,
    /// Its filled areas that a pick inside them hits (solid and gradient fills).
    pub fills: Vec<Vec<Vec2>>,
    pub texts: Vec<TextWords>,
    /// Its insertion points (Insertion snap): where each text is placed.
    pub inserts: Vec<Vec2>,
    /// Its WCS primitives, for the Z of a 2D hit.
    pub world: Vec<WorldPrimitive>,
    /// The resolver refused its geometry.
    pub refused: bool,
}

/// The XY view of `e` when it is canonical (`None` for a legacy payload). A canonical entity whose geometry the resolver
/// refuses has no geometry here: it is neither picked nor snapped, never read as its 2D fields.
pub(crate) fn flatten(d: &Drawing, e: &Entity) -> Option<Flat> {
    flatten_in(d, e, &Mat4::IDENTITY)
}

pub(crate) fn flatten_in(d: &Drawing, e: &Entity, parent: &Mat4) -> Option<Flat> {
    if e.geometry_revision != GeometryRevision::CanonicalV2 {
        return None;
    }
    let refused = || Some(Flat { refused: true, ..Flat::default() });
    let Ok(geometry) = cadcraft_render::entity_world_geometry(d, e, parent) else { return refused() };
    let Ok(prims) = geometry.project_xy() else { return refused() };
    let mut flat = Flat { prims, world: geometry.primitives().to_vec(), ..Flat::default() };
    // A canonical SOLID or TRACE is a closed loop of lines: its polygon is the filled area.
    if matches!(e.kind, cadcraft_doc::EntityKind::Solid(_) | cadcraft_doc::EntityKind::Trace(_)) {
        let corners: Vec<Vec2> =
            geometry.primitives().iter().filter_map(|p| if let WorldPrimitive::Line { a, .. } = p { Some(a.xy()) } else { None }).collect();
        flat.fills.push(corners);
        flat.prims.push(Prim::Fill(flat.fills[0].clone()));
    }
    for primitive in geometry.primitives() {
        match primitive {
            WorldPrimitive::Fill(fill) if matches!(fill.kind, WorldFillKind::Solid | WorldFillKind::Gradient(_)) => {
                flat.fills.extend(fill.loops.iter().map(|l| loop_points(l)));
            }
            WorldPrimitive::Text(text) => {
                let value = match &text.content {
                    WorldTextContent::Text(t) => t.value.clone(),
                    WorldTextContent::MText(t) => t.contents.clone(),
                };
                let angle = text.frame.x_axis.xy().angle();
                let origin = match &text.content {
                    WorldTextContent::Text(t) => match text.align {
                        Some(a) if !matches!((t.halign, t.valign), (cadcraft_doc::HAlign::Left, cadcraft_doc::VAlign::Baseline)) => a.xy(),
                        _ => text.frame.origin.xy(),
                    },
                    WorldTextContent::MText(_) => text.frame.origin.xy(),
                };
                flat.inserts.push(origin);
                if let Ok(layout) = cadcraft_render::layout_world_text(d, text, &value) {
                    let points = match layout {
                        cadcraft_render::WorldTextLayout::Text(shaped) => shaped.project_xy().strokes.into_iter().flatten().collect(),
                        cadcraft_render::WorldTextLayout::MText(pieces) => {
                            pieces.into_iter().flat_map(|p| p.shaped.project_xy().strokes.into_iter().flatten()).collect()
                        }
                    };
                    flat.texts.push(TextWords { points, angle });
                }
            }
            _ => {}
        }
    }
    Some(flat)
}

/// The points of a fill loop in the XY view.
fn loop_points(loop_: &[WorldPrimitive]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for primitive in loop_ {
        match primitive {
            WorldPrimitive::Line { a, b } => out.extend([a.xy(), b.xy()]),
            other => {
                if let Ok(owned) = cadcraft_doc::WorldGeometry::from_primitives(cadcraft_doc::Handle(0), vec![other.clone()])
                    && let Ok(prims) = owned.project_xy()
                {
                    for prim in prims {
                        match prim {
                            Prim::Ellipse(e) => {
                                let mut pts = Vec::new();
                                e.tessellate(e.major.len() * 1e-3, &mut pts);
                                out.extend(pts);
                            }
                            Prim::Seg(cadcraft_geom::Segment::Line(l)) => out.extend([l.a, l.b]),
                            Prim::Spline(s) => out.extend(s.tessellate(1e-3)),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    out.dedup();
    out
}

/// The WCS center of a canonical circle, arc or ellipse (the center of its conic), when the resolver gives it.
pub(crate) fn world_center(d: &Drawing, e: &Entity) -> Option<Vec3> {
    if e.geometry_revision != GeometryRevision::CanonicalV2 {
        return None;
    }
    let geometry = cadcraft_render::entity_world_geometry(d, e, &Mat4::IDENTITY).ok()?;
    match geometry.primitives() {
        [WorldPrimitive::Conic { center, .. }] => Some(*center),
        _ => None,
    }
}

/// The read-only geometry rows of a canonical LINE, CIRCLE or ARC for a property palette: WCS coordinates with their frame named, and
/// the lengths and angles that the world geometry gives. `None` for a legacy entity and for kinds whose rows hold no coordinates.
pub fn canonical_geometry_rows(d: &Drawing, e: &Entity) -> Option<Vec<(&'static str, String)>> {
    use cadcraft_doc::EntityKind;
    if e.geometry_revision != GeometryRevision::CanonicalV2 || !matches!(e.kind, EntityKind::Line(_) | EntityKind::Circle(_) | EntityKind::Arc(_)) {
        return None;
    }
    let real = |v: f64| format!("{v:.4}");
    let xyz = |v: Vec3, labels: [&'static str; 3]| [(labels[0], real(v.x)), (labels[1], real(v.y)), (labels[2], real(v.z))];
    let geometry = match cadcraft_render::entity_world_geometry(d, e, &Mat4::IDENTITY) {
        Ok(geometry) => geometry,
        Err(error) => return Some(vec![("Geometry", error.to_string())]),
    };
    let mut rows = Vec::new();
    match (&e.kind, geometry.primitives()) {
        (EntityKind::Line(_), [WorldPrimitive::Line { a, b }]) => {
            let delta = *b - *a;
            rows.extend(xyz(*a, ["Start X (WCS)", "Start Y (WCS)", "Start Z (WCS)"]));
            rows.extend(xyz(*b, ["End X (WCS)", "End Y (WCS)", "End Z (WCS)"]));
            rows.extend(xyz(delta, ["Delta X", "Delta Y", "Delta Z"]));
            rows.push(("Length", real(delta.len())));
            rows.push(("Angle", real(delta.xy().angle().to_degrees())));
        }
        (EntityKind::Circle(_) | EntityKind::Arc(_), [WorldPrimitive::Conic { center, axis_u, axis_v, .. }]) => {
            rows.extend(xyz(*center, ["Center X (WCS)", "Center Y (WCS)", "Center Z (WCS)"]));
            if let Some(normal) = axis_u.cross(*axis_v).try_normalized() {
                rows.extend(xyz(normal, ["Normal X", "Normal Y", "Normal Z"]));
            }
            let radius = axis_u.len();
            rows.push(("Radius", real(radius)));
            match &e.kind {
                EntityKind::Circle(_) => {
                    rows.push(("Diameter", real(radius * 2.0)));
                    rows.push(("Circumference", real(radius * cadcraft_geom::TAU)));
                    rows.push(("Area", real(radius * radius * cadcraft_geom::PI)));
                }
                EntityKind::Arc(a) => {
                    rows.push(("Start angle (in plane)", real(a.start.to_degrees())));
                    rows.push(("End angle (in plane)", real(a.end.to_degrees())));
                }
                _ => {}
            }
        }
        _ => return Some(vec![("Geometry", "no world geometry".into())]),
    }
    Some(rows)
}

impl Flat {
    /// The tessellated outlines of the primitives (and the words' boxes) for hit tests at `tol`.
    pub fn polylines(&self, tol: f64) -> Vec<Vec<Vec2>> {
        let mut out = Vec::new();
        for p in &self.prims {
            let mut pts = Vec::new();
            match p {
                Prim::Seg(s) => s.tessellate(tol, &mut pts),
                Prim::Circle(c) => c.tessellate(tol, &mut pts),
                Prim::Ellipse(el) => el.tessellate(tol, &mut pts),
                Prim::Spline(s) => pts = s.tessellate(tol),
                Prim::Point(q) => pts = vec![*q, *q],
                Prim::Infinite { base, dir, ray } => {
                    let far = 1e9;
                    pts = vec![if *ray { *base } else { *base - *dir * far }, *base + *dir * far];
                }
                Prim::Fill(f) => {
                    pts = f.clone();
                    if let Some(first) = f.first() {
                        pts.push(*first);
                    }
                }
            }
            out.push(pts);
        }
        out
    }

    /// Whether `p` is inside a filled area (odd parity over the loops of a fill) or on the words of a text.
    pub fn covers(&self, p: Vec2) -> bool {
        self.fills.iter().filter(|f| cadcraft_geom::point_in_polygon(f, p)).count() % 2 == 1
            || self.texts.iter().any(|t| {
                let turn = |q: Vec2| q.rotate(-t.angle);
                let b = cadcraft_geom::Bounds2::from_points(t.points.iter().filter(|q| q.is_finite()).map(|q| turn(*q)));
                !b.is_empty() && b.contains(turn(p))
            })
    }

    /// The WCS points of the geometry whose XY view is `p` (within `tol`): one per primitive that passes through it, and more than one
    /// where a primitive passes through it at different heights. When they do not agree on a Z the point is ambiguous in a 2D view.
    /// The reason comes with them when a primitive could not be told there: its answer is then unknown, which is not a miss.
    fn collect(&self, p: Vec2, tol: f64) -> Collected {
        let mut out = Collected::default();
        for w in &self.world {
            match w {
                WorldPrimitive::Spline { degree, knots, control, weights } => match spline_hits(*degree, knots, control, weights, p, tol) {
                    SplineProbe::Points(points) => out.points.extend(points),
                    SplineProbe::Unsupported(why) => out.unsupported = out.unsupported.or(Some(why)),
                },
                WorldPrimitive::Conic { center, .. } if center.xy().dist(p) <= tol => {
                    out.points.extend(world_point(w, p, tol));
                    out.points.push(*center);
                }
                _ => out.points.extend(world_point(w, p, tol)),
            }
        }
        out
    }
}

/// What `Flat::collect` found at a plan-view point.
#[derive(Default)]
struct Collected {
    points: Vec<Vec3>,
    unsupported: Option<&'static str>,
}

/// The WCS point of `w` whose XY is `p`, if `w` passes through it.
fn world_point(w: &WorldPrimitive, p: Vec2, tol: f64) -> Option<Vec3> {
    match w {
        WorldPrimitive::Point(q) => (q.xy().dist(p) <= tol).then_some(*q),
        WorldPrimitive::Line { a, b } => {
            let d = b.xy() - a.xy();
            let len2 = d.len2();
            if len2 <= f64::EPSILON {
                return (a.xy().dist(p) <= tol).then_some(Vec3::new(a.x, a.y, a.z.min(b.z)));
            }
            let t = ((p - a.xy()).dot(d) / len2).clamp(0.0, 1.0);
            let at = a.xy() + d * t;
            (at.dist(p) <= tol).then(|| *a + (*b - *a) * t)
        }
        WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => {
            // The parameter whose XY point is nearest `p`: coarse samples over the sweep, then a local refinement.
            let sweep = cadcraft_geom::ccw_sweep(*start, *end);
            let at = |t: f64| *center + *axis_u * t.cos() + *axis_v * t.sin();
            let n = 720;
            let mut best = (f64::INFINITY, *start);
            for i in 0..=n {
                let t = start + sweep * f64::from(i) / f64::from(n);
                let dist = at(t).xy().dist(p);
                if dist < best.0 {
                    best = (dist, t);
                }
            }
            let (mut lo, mut hi) = (best.1 - sweep / f64::from(n), best.1 + sweep / f64::from(n));
            for _ in 0..60 {
                let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                if at(m1).xy().dist(p) < at(m2).xy().dist(p) {
                    hi = m2;
                } else {
                    lo = m1;
                }
            }
            let t = (lo + hi) / 2.0;
            let t = if cadcraft_geom::angle_in_sweep(t, *start, *end) { t } else { best.1 };
            let point = at(t);
            (point.xy().dist(p) <= tol).then_some(point)
        }
        // A spline can pass through `p` more than once: `Flat::collect` asks `spline_hits`.
        WorldPrimitive::Spline { .. } => None,
        // A text and a fill have no single WCS point at a plan-view point.
        WorldPrimitive::Text(_) | WorldPrimitive::Fill(_) => None,
    }
}

/// What the plan-view point finds on one spline.
enum SplineProbe {
    /// The WCS points of the curve whose XY is the point: none for a miss, two or more where it passes through at different heights.
    Points(Vec<Vec3>),
    /// The curve cannot be told at the point.
    Unsupported(&'static str),
}

/// Samples per knot span in the search for the passes of a spline.
const SPAN_SAMPLES: usize = 32;
/// Most plan-view samples one probe of a spline takes; a spline with more candidate spans than this allows is Unsupported at the point.
const MAX_SAMPLES: usize = 16_384;
/// Most separate passes (local minima) one probe refines.
const MAX_PASSES: usize = 64;
/// The evaluations one probe makes at most: the plan-view samples, the height of the samples of a stretch, and a refinement per pass
/// (two evaluations for each of 60 steps, then the end point's two). The tests check the bound; nothing needs it at run time.
#[cfg(test)]
const MAX_EVALS: usize = 2 * MAX_SAMPLES + MAX_PASSES * 125;

/// The WCS points of a (rational) B-spline whose XY is within `tol` of `p`. Z is evaluated as a second spline over the same knots and
/// weights (the Z of the control points on the first axis), which is the Z of the same rational curve.
///
/// A curve can pass a plan-view point more than once (a self-crossing at different heights) or stay on it over a stretch (a vertical
/// segment), so the answer is every place where the distance to `p` has a local minimum within `tol`, and a stretch that stays on `p`
/// (within a thousandth of `tol`) gives the points at its lowest and highest Z. Sampling cannot prove that two passes are not closer
/// than one sample step: the ceiling is one sample per 1/32 of a knot span.
///
/// The work is bounded whatever the spline: the knot spans are tested by the box of their control points (a span lies in the convex
/// hull of its `degree + 1` control points when the weights are positive), so only the spans that can reach `p` are sampled, and a
/// probe that would take more than [`MAX_SAMPLES`] samples or find more than [`MAX_PASSES`] passes is Unsupported. The knot vector is
/// validated once.
fn spline_hits(degree: usize, knots: &[f64], control: &[Vec3], weights: &[f64], p: Vec2, tol: f64) -> SplineProbe {
    spline_search(degree, knots, control, weights, p, tol).0
}

/// `spline_hits` and the number of curve evaluations it made.
fn spline_search(degree: usize, knots: &[f64], control: &[Vec3], weights: &[f64], p: Vec2, tol: f64) -> (SplineProbe, usize) {
    let plan = cadcraft_geom::Spline {
        degree,
        knots: knots.to_vec(),
        control: control.iter().map(|c| c.xy()).collect(),
        weights: weights.to_vec(),
        ..Default::default()
    };
    let Some(plan_curve) = plan.validated() else { return (SplineProbe::Points(Vec::new()), 0) };
    if !(control.iter().all(|c| c.x.is_finite() && c.y.is_finite() && c.z.is_finite()) && knots.iter().all(|k| k.is_finite())) {
        return (SplineProbe::Unsupported("its spline has a non-finite control point or knot"), 0);
    }
    if weights.iter().any(|w| !(*w > 0.0 && w.is_finite())) {
        return (SplineProbe::Unsupported("its spline has a weight that is not positive"), 0);
    }
    let (lo, hi) = plan_curve.domain();
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
        return (SplineProbe::Points(Vec::new()), 0);
    }
    let height = cadcraft_geom::Spline { control: control.iter().map(|c| Vec2::new(c.z, 0.0)).collect(), ..plan.clone() };
    let Some(height_curve) = height.validated() else { return (SplineProbe::Points(Vec::new()), 0) };

    // The knot spans whose control points' box reaches `p`, in parameter order, grouped into runs of neighbouring spans.
    let reach = |span: usize| {
        let first = span.saturating_sub(degree);
        let points = plan.control.get(first..=span).unwrap_or(&[]);
        let b = cadcraft_geom::Bounds2::from_points(points.iter().copied());
        !b.is_empty() && p.x >= b.min.x - tol && p.x <= b.max.x + tol && p.y >= b.min.y - tol && p.y <= b.max.y + tol
    };
    let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut open = false;
    let mut candidates = 0usize;
    for span in degree..control.len() {
        let (a, b) = (knots.get(span).copied().unwrap_or(0.0), knots.get(span + 1).copied().unwrap_or(0.0));
        if b <= a {
            continue;
        }
        if reach(span) {
            if !open {
                runs.push(Vec::new());
                open = true;
            }
            if let Some(run) = runs.last_mut() {
                run.push((a, b));
            }
            candidates += 1;
        } else {
            open = false;
        }
    }
    if candidates == 0 {
        return (SplineProbe::Points(Vec::new()), 0);
    }
    if candidates.saturating_mul(SPAN_SAMPLES) + runs.len() > MAX_SAMPLES {
        return (SplineProbe::Unsupported("too many parts of its spline pass near that point: zoom in, because the pick radius follows the view"), 0);
    }

    let evals = std::cell::Cell::new(0usize);
    let plan_at = |t: f64| {
        evals.set(evals.get() + 1);
        plan_curve.eval(t)
    };
    let height_at = |t: f64| {
        evals.set(evals.get() + 1);
        height_curve.eval(t).x
    };
    let stay = (tol * 1e-3).max(1e-9);
    let mut hits: Vec<Vec3> = Vec::new();
    let mut passes = 0usize;
    for run in &runs {
        let mut ts: Vec<f64> = Vec::new();
        for (a, b) in run {
            let from = usize::from(!ts.is_empty());
            ts.extend((from..=SPAN_SAMPLES).map(|k| a + (b - a) * k as f64 / SPAN_SAMPLES as f64));
        }
        let at: Vec<Vec2> = ts.iter().map(|t| plan_at(*t)).collect();
        let dist: Vec<f64> = at.iter().map(|q| q.dist(p)).collect();
        let n = ts.len() - 1;
        let mut i = 0;
        while i <= n {
            if dist[i] <= stay {
                // A stretch of samples on the point: the curve stays there over it, whatever its Z does.
                let start = i;
                while i < n && dist[i + 1] <= stay {
                    i += 1;
                }
                let on: Vec<Vec3> = (start..=i).map(|k| Vec3::new(at[k].x, at[k].y, height_at(ts[k]))).collect();
                hits.extend(on.iter().copied().min_by(|a, b| a.z.total_cmp(&b.z)));
                hits.extend(on.iter().copied().max_by(|a, b| a.z.total_cmp(&b.z)));
                i += 1;
                continue;
            }
            // A local minimum of the distance: refine between its neighbours (the curve is smooth between samples).
            let left = if i == 0 { f64::INFINITY } else { dist[i - 1] };
            let right = if i == n { f64::INFINITY } else { dist[i + 1] };
            if dist[i] <= left && dist[i] < right {
                passes += 1;
                if passes > MAX_PASSES {
                    return (
                        SplineProbe::Unsupported(
                            "too many parts of its spline pass near that point: zoom in, because the pick radius follows the view",
                        ),
                        evals.get(),
                    );
                }
                let (mut a, mut b) = (ts[i.saturating_sub(1)], ts[(i + 1).min(n)]);
                for _ in 0..60 {
                    let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
                    if plan_at(m1).dist(p) < plan_at(m2).dist(p) {
                        b = m2;
                    } else {
                        a = m1;
                    }
                }
                let t = (a + b) / 2.0;
                let found = plan_at(t);
                if found.dist(p) <= tol {
                    hits.push(Vec3::new(found.x, found.y, height_at(t)));
                }
            }
            i += 1;
        }
    }
    // The same place found twice (the end of a refined minimum and a sample on it, or the two ends of a span) is one point.
    let mut points: Vec<Vec3> = Vec::new();
    for hit in hits {
        if !points.iter().any(|q| (q.x - hit.x).abs() <= tol && (q.y - hit.y).abs() <= tol && (q.z - hit.z).abs() <= tol) {
            points.push(hit);
        }
    }
    (SplineProbe::Points(points), evals.get())
}

/// What a plan-view sample of a canonical entity finds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorldSample {
    /// The WCS point of the geometry under the sample.
    Hit(Vec3),
    /// Nothing of the entity is under the sample.
    Miss,
    /// Parts of the entity at the same plan-view point have different Z values.
    Ambiguous,
    /// The geometry has no single WCS point there (text, a filled area), or no world form at all.
    Unsupported(&'static str),
}

impl Flat {
    /// The WCS point under the plan-view point `p` within `tol`, or why there is none.
    pub fn sample(&self, p: Vec2, tol: f64) -> WorldSample {
        if self.refused {
            return WorldSample::Unsupported("its geometry has no world form");
        }
        let Collected { mut points, unsupported } = self.collect(p, tol);
        if let Some(why) = unsupported {
            return WorldSample::Unsupported(why);
        }
        if points.is_empty() {
            return if self.covers(p) { WorldSample::Unsupported("text and filled areas have no single world point") } else { WorldSample::Miss };
        }
        points.sort_by(|a, b| a.xy().dist(p).total_cmp(&b.xy().dist(p)));
        let nearest = points[0].xy().dist(p);
        let same: Vec<f64> = points.iter().filter(|q| q.xy().dist(p) <= nearest + tol).map(|q| q.z).collect();
        if same.iter().any(|z| (z - points[0].z).abs() > tol) {
            return WorldSample::Ambiguous;
        }
        WorldSample::Hit(points[0])
    }
}

/// `hit` with the WCS point of the canonical geometry under it: the single point the nearby canonical objects pass through
/// at its 2D position, or none when there is not one (legacy geometry, or several Z values).
pub(crate) fn with_world(world: Vec<WorldPrimitive>, mut hit: crate::snap::SnapHit) -> crate::snap::SnapHit {
    if hit.deferred.is_some() {
        return hit;
    }
    let tol = 1e-6 * hit.point.x.abs().max(hit.point.y.abs()).max(1.0);
    let flat = Flat { world, ..Flat::default() };
    let Collected { points, unsupported } = flat.collect(hit.point, tol);
    if let Some(first) = points.first()
        && unsupported.is_none()
        && points.iter().all(|p| (p.z - first.z).abs() <= tol)
    {
        hit.world = Some(*first);
    }
    hit
}

/// A grip of a canonical entity: its XY position, the WCS point it is at, and whether dragging it moves the whole entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanonicalGrip {
    pub point: Vec2,
    pub world: Vec3,
    /// A grip that stands for the entity as a whole (a line's middle, a circle's center, a point, a text's insertion point).
    pub moves: bool,
}

impl Flat {
    /// The grips of the WCS geometry, in the order of its primitives: a line's ends and middle, a conic's center and its
    /// quadrants (or the ends of an arc), a point, a text's insertion point, a spline's control points and the corners of a
    /// fill. A WCS point that two primitives share is one grip.
    pub fn grips(&self) -> Vec<CanonicalGrip> {
        let mut out: Vec<CanonicalGrip> = Vec::new();
        let mut add = |world: Vec3, moves: bool| {
            if !out.iter().any(|g| g.world == world) {
                out.push(CanonicalGrip { point: world.xy(), world, moves });
            }
        };
        for primitive in &self.world {
            match primitive {
                WorldPrimitive::Line { a, b } => {
                    add(*a, false);
                    add((*a + *b) * 0.5, true);
                    add(*b, false);
                }
                WorldPrimitive::Point(p) => add(*p, true),
                WorldPrimitive::Conic { center, axis_u, axis_v, start, end } => {
                    let at = |t: f64| *center + *axis_u * t.cos() + *axis_v * t.sin();
                    add(*center, true);
                    if cadcraft_geom::ccw_sweep(*start, *end) >= std::f64::consts::TAU - 1e-12 {
                        (0..4).for_each(|k| add(at(f64::from(k) * std::f64::consts::FRAC_PI_2), false));
                    } else {
                        add(at(*start), false);
                        add(at(start + cadcraft_geom::ccw_sweep(*start, *end) / 2.0), false);
                        add(at(*end), false);
                    }
                }
                WorldPrimitive::Spline { control, .. } => control.iter().for_each(|c| add(*c, false)),
                WorldPrimitive::Text(text) => add(text.frame.origin, true),
                WorldPrimitive::Fill(fill) => {
                    for edge in fill.loops.iter().flatten() {
                        if let WorldPrimitive::Line { a, .. } = edge {
                            add(*a, false);
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64, f64), b: (f64, f64, f64)) -> WorldPrimitive {
        WorldPrimitive::Line { a: Vec3::new(a.0, a.1, a.2), b: Vec3::new(b.0, b.1, b.2) }
    }

    #[test]
    fn a_sample_says_hit_miss_ambiguous_or_unsupported() {
        let flat = Flat { world: vec![line((0.0, 0.0, 0.0), (10.0, 0.0, 4.0))], ..Flat::default() };
        assert_eq!(flat.sample(Vec2::new(5.0, 0.0), 1e-6), WorldSample::Hit(Vec3::new(5.0, 0.0, 2.0)));
        assert_eq!(flat.sample(Vec2::new(5.0, 3.0), 1e-6), WorldSample::Miss);
        // Two parts that cross in the plan view at different heights have no single Z there.
        let crossing = Flat { world: vec![line((0.0, 0.0, 0.0), (10.0, 0.0, 0.0)), line((5.0, -5.0, 3.0), (5.0, 5.0, 3.0))], ..Flat::default() };
        assert_eq!(crossing.sample(Vec2::new(5.0, 0.0), 1e-6), WorldSample::Ambiguous);
        // Where they agree, it is a hit.
        let touching = Flat { world: vec![line((0.0, 0.0, 3.0), (10.0, 0.0, 3.0)), line((5.0, -5.0, 3.0), (5.0, 5.0, 3.0))], ..Flat::default() };
        assert_eq!(touching.sample(Vec2::new(5.0, 0.0), 1e-6), WorldSample::Hit(Vec3::new(5.0, 0.0, 3.0)));
        // A refused geometry has no world form.
        assert!(matches!(Flat { refused: true, ..Flat::default() }.sample(Vec2::ZERO, 1.0), WorldSample::Unsupported(_)));
    }

    #[test]
    fn a_spatial_spline_gives_its_z_at_a_plan_view_point() {
        // Three control points on the line y = 0 with Z equal to X: the rational curve keeps Z = X at every point.
        let spline = WorldPrimitive::Spline {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            control: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 5.0), Vec3::new(10.0, 0.0, 10.0)],
            weights: Vec::new(),
        };
        let flat = Flat { world: vec![spline], ..Flat::default() };
        for x in [1.0, 4.0, 7.5] {
            let WorldSample::Hit(w) = flat.sample(Vec2::new(x, 0.0), 1e-6) else { panic!("no hit at {x}") };
            assert!((w.z - x).abs() < 1e-6, "z {} at x {x}", w.z);
        }
        assert_eq!(flat.sample(Vec2::new(4.0, 2.0), 1e-6), WorldSample::Miss);
    }

    fn polyline_spline(points: &[(f64, f64, f64)]) -> WorldPrimitive {
        // Degree 1 with uniform interior knots: the curve is the polyline through the control points.
        let n = points.len();
        let mut knots = vec![0.0];
        knots.extend((0..n).map(|i| i as f64 / (n - 1) as f64));
        knots.push(1.0);
        WorldPrimitive::Spline { degree: 1, knots, control: points.iter().map(|p| Vec3::new(p.0, p.1, p.2)).collect(), weights: Vec::new() }
    }

    #[test]
    fn a_vertical_spline_has_no_single_z_at_its_plan_view_point() {
        // Both controls project to (0, 0): the whole curve is one plan-view point with every Z from 0 to 10.
        let flat = Flat { world: vec![polyline_spline(&[(0.0, 0.0, 0.0), (0.0, 0.0, 10.0)])], ..Flat::default() };
        assert_eq!(flat.sample(Vec2::new(0.0, 0.0), 1e-6), WorldSample::Ambiguous);
        assert_eq!(flat.sample(Vec2::new(1.0, 0.0), 1e-6), WorldSample::Miss);
    }

    #[test]
    fn a_spline_that_crosses_itself_in_plan_view_at_different_heights_is_ambiguous_there() {
        // AB runs along y = x at Z 0 and CD along y = 10 - x at Z 6: they cross at (5, 5), and BC joins them.
        let curve = [(0.0, 0.0, 0.0), (10.0, 10.0, 0.0), (10.0, 0.0, 6.0), (0.0, 10.0, 6.0)];
        let flat = Flat { world: vec![polyline_spline(&curve)], ..Flat::default() };
        assert_eq!(flat.sample(Vec2::new(5.0, 5.0), 1e-6), WorldSample::Ambiguous);
        // Away from the crossing each pass is single: AB at (2, 2) is at Z 0, and CD at (2, 8) is at Z 6.
        assert_hit(&flat, (2.0, 2.0), 0.0);
        assert_hit(&flat, (2.0, 8.0), 6.0);
        // The same curve crossing itself at one height has a single Z there.
        let level = [(0.0, 0.0, 3.0), (10.0, 10.0, 3.0), (10.0, 0.0, 3.0), (0.0, 10.0, 3.0)];
        let flat = Flat { world: vec![polyline_spline(&level)], ..Flat::default() };
        assert_hit(&flat, (5.0, 5.0), 3.0);
    }

    #[test]
    fn a_spline_with_a_non_finite_control_point_is_unsupported_not_a_miss() {
        let flat = Flat { world: vec![polyline_spline(&[(0.0, 0.0, 0.0), (f64::NAN, 1.0, 2.0)])], ..Flat::default() };
        assert!(matches!(flat.sample(Vec2::new(0.0, 0.0), 1e-6), WorldSample::Unsupported(_)));
    }

    /// A cubic B-spline with `n` clamped, uniformly spaced knots through the controls `control(i)`.
    fn cubic(n: usize, control: impl Fn(usize) -> Vec3) -> (Vec<f64>, Vec<Vec3>) {
        let mut knots = vec![0.0; 4];
        knots.extend((1..=n - 4).map(|k| k as f64));
        knots.extend([(n - 3) as f64; 4]);
        (knots, (0..n).map(control).collect())
    }

    #[test]
    fn a_spline_of_ten_thousand_controls_is_probed_within_the_work_bound() {
        // The controls (i, 0, i / 1000) reproduce the line z = x / 1000: the curve is x = t + 1.5.
        let (knots, control) = cubic(10_000, |i| Vec3::new(i as f64, 0.0, i as f64 / 1000.0));
        let (probe, evals) = spline_search(3, &knots, &control, &[], Vec2::new(5000.5, 0.0), 1e-6);
        let SplineProbe::Points(points) = probe else { panic!("unsupported") };
        assert_eq!(points.len(), 1);
        assert!((points[0].x - 5000.5).abs() < 1e-6 && (points[0].z - 5.0005).abs() < 1e-6, "{:?}", points[0]);
        // Only the few spans whose controls' box reaches the point are sampled, not every span of the curve (the full-domain search took
        // 320,001 evaluations here).
        assert!(evals <= 1000, "{evals} evaluations");
        // A point far from every span costs none.
        let (probe, evals) = spline_search(3, &knots, &control, &[], Vec2::new(-100.0, 0.0), 1e-6);
        assert!(matches!(probe, SplineProbe::Points(p) if p.is_empty()));
        assert_eq!(evals, 0);
    }

    #[test]
    fn a_probe_that_would_exceed_the_budget_is_unsupported_before_it_evaluates() {
        // Every span of the spline lies on the point: 9,997 spans of 32 samples are over the budget.
        let (knots, control) = cubic(10_000, |i| Vec3::new(0.0, 0.0, i as f64));
        let (probe, evals) = spline_search(3, &knots, &control, &[], Vec2::ZERO, 1e-6);
        assert!(matches!(probe, SplineProbe::Unsupported(_)));
        assert_eq!(evals, 0);
        // Under the budget the same curve is answered, and the answer stays within the bound on the evaluations.
        let (knots, control) = cubic(400, |i| Vec3::new(0.0, 0.0, i as f64));
        let (probe, evals) = spline_search(3, &knots, &control, &[], Vec2::ZERO, 1e-6);
        let SplineProbe::Points(points) = probe else { panic!("unsupported") };
        assert!(points.len() >= 2 && points.iter().any(|q| q.z > 300.0) && points.iter().any(|q| q.z < 100.0), "{points:?}");
        assert!(evals <= MAX_EVALS, "{evals} evaluations");
    }

    #[test]
    fn more_passes_than_the_budget_allows_are_unsupported_within_the_bound() {
        // A zigzag that crosses the point in every span at a different height, off the sample grid: every crossing is a pass, and only 64 are
        // refined.
        let (knots, control) = cubic(400, |i| Vec3::new(if i % 2 == 0 { -1.0 } else { 1.37 }, 0.0, i as f64));
        let (probe, evals) = spline_search(3, &knots, &control, &[], Vec2::ZERO, 1e-6);
        assert!(matches!(probe, SplineProbe::Unsupported(_)), "the passes were not limited");
        assert!(evals <= MAX_EVALS, "{evals} evaluations");
    }

    #[test]
    fn a_spline_with_a_weight_that_is_not_positive_is_unsupported() {
        let (knots, control) = cubic(8, |i| Vec3::new(i as f64, 0.0, 0.0));
        let mut weights = vec![1.0; 8];
        weights[3] = 0.0;
        assert!(matches!(spline_search(3, &knots, &control, &weights, Vec2::new(2.0, 0.0), 1e-6).0, SplineProbe::Unsupported(_)));
    }

    /// The sample is a hit at `at` with the height `z` (the refinement finds the point to about 1e-9).
    fn assert_hit(flat: &Flat, at: (f64, f64), z: f64) {
        let WorldSample::Hit(w) = flat.sample(Vec2::new(at.0, at.1), 1e-6) else { panic!("no hit at {at:?}") };
        assert!((w.x - at.0).abs() < 1e-6 && (w.y - at.1).abs() < 1e-6 && (w.z - z).abs() < 1e-6, "{w:?} at {at:?}");
    }
}

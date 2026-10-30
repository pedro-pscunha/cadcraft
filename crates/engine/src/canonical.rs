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

    /// The WCS points of the geometry whose XY view is `p` (within `tol`): one per primitive that passes through it.
    /// When they do not agree on a Z the point is ambiguous in a 2D view.
    pub fn world_points(&self, p: Vec2, tol: f64) -> Vec<Vec3> {
        let centers = self.world.iter().filter_map(|w| match w {
            WorldPrimitive::Conic { center, .. } if center.xy().dist(p) <= tol => Some(*center),
            _ => None,
        });
        self.world.iter().filter_map(|w| world_point(w, p, tol)).chain(centers).collect()
    }
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
        WorldPrimitive::Spline { degree, knots, control, weights } => spline_point(*degree, knots, control, weights, p, tol),
        // A text and a fill have no single WCS point at a plan-view point.
        WorldPrimitive::Text(_) | WorldPrimitive::Fill(_) => None,
    }
}

/// The WCS point of a (rational) B-spline whose XY is nearest `p`, when it is within `tol`. Z is evaluated as a second spline over the
/// same knots and weights (the Z of the control points on the first axis), which is the Z of the same rational curve.
fn spline_point(degree: usize, knots: &[f64], control: &[Vec3], weights: &[f64], p: Vec2, tol: f64) -> Option<Vec3> {
    let plan = cadcraft_geom::Spline {
        degree,
        knots: knots.to_vec(),
        control: control.iter().map(|c| c.xy()).collect(),
        weights: weights.to_vec(),
        ..Default::default()
    };
    if !plan.is_valid() {
        return None;
    }
    let height = cadcraft_geom::Spline { control: control.iter().map(|c| Vec2::new(c.z, 0.0)).collect(), ..plan.clone() };
    let (lo, hi) = plan.domain();
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo {
        return None;
    }
    let n = (control.len() * 32).max(256);
    let (mut best_t, mut best) = (lo, f64::INFINITY);
    for i in 0..=n {
        let t = lo + (hi - lo) * i as f64 / n as f64;
        let d = plan.eval(t).dist(p);
        if d < best {
            (best_t, best) = (t, d);
        }
    }
    // Refine around the best sample (the curve is smooth between samples; a ternary search finds the local minimum).
    let step = (hi - lo) / n as f64;
    let (mut a, mut b) = ((best_t - step).max(lo), (best_t + step).min(hi));
    for _ in 0..60 {
        let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
        if plan.eval(m1).dist(p) < plan.eval(m2).dist(p) {
            b = m2;
        } else {
            a = m1;
        }
    }
    let t = (a + b) / 2.0;
    let at = plan.eval(t);
    (at.dist(p) <= tol).then(|| Vec3::new(at.x, at.y, height.eval(t).x))
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
        let mut points = self.world_points(p, tol);
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
    let points = flat.world_points(hit.point, tol);
    if let Some(first) = points.first()
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
}

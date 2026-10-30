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
        // A spline and a text have no WCS point to give back here.
        WorldPrimitive::Spline { .. } | WorldPrimitive::Text(_) | WorldPrimitive::Fill(_) => None,
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

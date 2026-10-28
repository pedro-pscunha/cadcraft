//! Associative dimensions: definition points attached to objects (`Dimension::assoc`) follow
//! them. [`update`] runs after every command that changed the drawing (from
//! `Session::execute` and when an interactive command finishes).

use std::sync::Arc;

use cadcraft_doc::{AssocSnap, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, Space};
use cadcraft_geom::{Vec2, Vec3};

/// Definition point names in the order they are tried.
const POINTS: [&str; 5] = ["defpt", "p13", "p14", "p15", "p16"];

fn def_point_mut<'a>(d: &'a mut Dimension, name: &str) -> Option<&'a mut Vec3> {
    Some(match name {
        "defpt" => &mut d.defpt,
        "p13" => &mut d.p13,
        "p14" => &mut d.p14,
        "p15" => &mut d.p15,
        "p16" => &mut d.p16,
        _ => return None,
    })
}

fn arc_geom(a: &cadcraft_doc::Arc) -> cadcraft_geom::Arc {
    cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end)
}

/// Resolve supported snap semantics in the engine's model/layout scope, not block-local space.
pub fn snap_point(d: &Drawing, a: &DimAssoc) -> Option<Vec2> {
    cadcraft_doc::association_point(a, |h| d.entity(h).map(|e| e.as_ref()))
}

fn close(a: Vec2, b: Vec2) -> bool {
    a.dist(b) <= 1e-6 * (1.0 + a.x.abs().max(a.y.abs()))
}

/// Candidate snaps of an object (endpoints first, then centres, then midpoints).
fn candidates(kind: &EntityKind) -> Vec<(AssocSnap, Vec2)> {
    match kind {
        EntityKind::Line(l) => vec![(AssocSnap::Start, l.a.xy()), (AssocSnap::End, l.b.xy()), (AssocSnap::Mid, l.a.xy().mid(l.b.xy()))],
        EntityKind::Arc(a) => {
            let g = arc_geom(a);
            vec![
                (AssocSnap::Start, g.start_point()),
                (AssocSnap::End, g.end_point()),
                (AssocSnap::Center, a.center.xy()),
                (AssocSnap::Mid, g.mid_point()),
            ]
        }
        EntityKind::Circle(c) => vec![(AssocSnap::Center, c.center.xy())],
        EntityKind::LwPolyline(p) => p.vertices.iter().take(10_000).enumerate().map(|(i, v)| (AssocSnap::Vertex { index: i }, v.p)).collect(),
        EntityKind::Point(p) => vec![(AssocSnap::Start, p.p.xy())],
        _ => Vec::new(),
    }
}

/// An object point exactly at `p` (an endpoint, vertex, centre or midpoint) to attach to.
pub fn find_snap(d: &Drawing, space: &Space, p: Vec2) -> Option<(Handle, AssocSnap)> {
    if !p.is_finite() {
        return None;
    }
    let store = d.space(space)?;
    let mut best: Option<(usize, Handle, AssocSnap)> = None;
    for e in store.iter() {
        for (i, (snap, q)) in candidates(&e.kind).into_iter().enumerate() {
            let rank = snap_rank(&snap, i);
            if close(p, q) && best.as_ref().is_none_or(|b| rank < b.0) {
                best = Some((rank, e.handle, snap));
            }
        }
    }
    best.map(|(_, h, s)| (h, s))
}

/// Preference of the `i`-th candidate snap of an object (lower wins): endpoints and vertices,
/// then centres, then midpoints.
fn snap_rank(snap: &AssocSnap, i: usize) -> usize {
    match snap {
        AssocSnap::Mid => 100 + i,
        AssocSnap::Center => 50 + i,
        _ => i.min(1),
    }
}

/// Associations for named definition points found at their positions.
pub fn auto_assoc(d: &Drawing, space: &Space, pts: &[(&str, Vec2)]) -> Vec<DimAssoc> {
    pts.iter().filter_map(|(name, p)| find_snap(d, space, *p).map(|(handle, snap)| DimAssoc { point: (*name).into(), handle, snap })).collect()
}

/// The snap candidates of one space, sorted by x, for many [`find_snap`] lookups in a row: a
/// lookup is a binary search instead of a scan of the whole space. Dimensions add no candidates,
/// so the index stays valid while a command adds dimensions (DIMCONTINUE/DIMBASELINE chains).
pub struct SnapIndex {
    /// (point, rank, position in space order, object, snap).
    pts: Vec<(Vec2, usize, usize, Handle, AssocSnap)>,
}

impl SnapIndex {
    pub fn new(d: &Drawing, space: &Space) -> Self {
        let mut pts = Vec::new();
        for e in d.space(space).into_iter().flat_map(|st| st.iter()) {
            for (i, (snap, q)) in candidates(&e.kind).into_iter().enumerate() {
                if q.is_finite() {
                    pts.push((q, snap_rank(&snap, i), pts.len(), e.handle, snap));
                }
            }
        }
        pts.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        SnapIndex { pts }
    }

    /// The same snap as [`find_snap`] (lowest rank, then the first in space order).
    pub fn find(&self, p: Vec2) -> Option<(Handle, AssocSnap)> {
        if !p.is_finite() {
            return None;
        }
        let tol = 1e-6 * (1.0 + p.x.abs().max(p.y.abs()));
        let lo = self.pts.partition_point(|c| c.0.x < p.x - tol);
        let hi = self.pts.partition_point(|c| c.0.x <= p.x + tol);
        let hit = self.pts.get(lo..hi)?.iter().filter(|c| close(p, c.0)).min_by_key(|c| (c.1, c.2))?;
        Some((hit.3, hit.4.clone()))
    }

    /// [`auto_assoc`] through the index.
    pub fn auto_assoc(&self, pts: &[(&str, Vec2)]) -> Vec<DimAssoc> {
        pts.iter().filter_map(|(name, p)| self.find(*p).map(|(handle, snap)| DimAssoc { point: (*name).into(), handle, snap })).collect()
    }
}

/// A circle or arc (centre, radius) by handle.
fn circle_of(d: &Drawing, h: Handle) -> Option<(Vec2, f64)> {
    match &d.entity(h)?.kind {
        EntityKind::Circle(c) => Some((c.center.xy(), c.radius)),
        EntityKind::Arc(a) => Some((a.center.xy(), a.radius)),
        _ => None,
    }
}

/// A circle or arc whose centre is at `c` (and radius `r`, when given).
fn circle_at(d: &Drawing, space: &Space, c: Vec2, r: Option<f64>) -> Option<Handle> {
    d.space(space)?.iter().find_map(|e| {
        let (cc, rr) = match &e.kind {
            EntityKind::Circle(x) => (x.center.xy(), x.radius),
            EntityKind::Arc(x) => (x.center.xy(), x.radius),
            _ => return None,
        };
        (close(cc, c) && r.is_none_or(|r| (r - rr).abs() <= 1e-6 * (1.0 + r.abs()))).then_some(e.handle)
    })
}

/// Work out the associations of a dimension from the geometry under its definition points
/// (DIMREASSOCIATE).
pub fn reassociate(d: &Drawing, space: &Space, dm: &Dimension) -> Vec<DimAssoc> {
    let mut links = match dm.kind {
        DimKind::Radius => {
            let (c, p) = (dm.defpt.xy(), dm.p15.xy());
            match circle_at(d, space, c, Some(c.dist(p))) {
                Some(h) => vec![
                    DimAssoc { point: "defpt".into(), handle: h, snap: AssocSnap::Center },
                    DimAssoc { point: "p15".into(), handle: h, snap: AssocSnap::OnCircle { angle: c.angle_to(p) } },
                ],
                None => Vec::new(),
            }
        }
        DimKind::Diameter => {
            let (a, b) = (dm.defpt.xy(), dm.p15.xy());
            let c = a.mid(b);
            match circle_at(d, space, c, Some(a.dist(b) / 2.0)) {
                Some(h) => vec![
                    DimAssoc { point: "defpt".into(), handle: h, snap: AssocSnap::OnCircle { angle: c.angle_to(a) } },
                    DimAssoc { point: "p15".into(), handle: h, snap: AssocSnap::OnCircle { angle: c.angle_to(b) } },
                ],
                None => Vec::new(),
            }
        }
        _ => auto_assoc(
            d,
            space,
            &dm.kind.association_points().iter().filter_map(|name| dm.definition_point(name).map(|point| (*name, point.xy()))).collect::<Vec<_>>(),
        ),
    };
    // Coincident endpoints are ambiguous. Do not switch a still-valid existing attachment
    // (notably the second angular leg's origin) to an earlier object at the same point.
    for previous in &dm.assoc {
        if snap_point(d, previous).zip(dm.definition_point(&previous.point)).is_some_and(|(snap, point)| close(snap, point.xy()))
            && let Some(link) = links.iter_mut().find(|link| link.point == previous.point)
        {
            *link = previous.clone();
        }
    }
    links
}

/// Associations for a radius/diameter dimension made by picking a circle or arc.
pub fn radial_assoc(d: &Drawing, h: Handle, diameter: bool, defpt: Vec2, p15: Vec2) -> Vec<DimAssoc> {
    let Some((c, _)) = circle_of(d, h) else { return Vec::new() };
    let on = |name: &str, p: Vec2| DimAssoc { point: name.into(), handle: h, snap: AssocSnap::OnCircle { angle: c.angle_to(p) } };
    if diameter {
        vec![on("defpt", defpt), on("p15", p15)]
    } else {
        vec![DimAssoc { point: "defpt".into(), handle: h, snap: AssocSnap::Center }, on("p15", p15)]
    }
}

fn src_changed(before: &Drawing, after: &Drawing, h: Handle) -> bool {
    match (before.entity_including_blocks(h), after.entity_including_blocks(h)) {
        (Some(x), Some(y)) => !Arc::ptr_eq(x, y) && x.kind != y.kind,
        (Some(_), None) | (None, Some(_)) => true,
        (None, None) => match (before.attribute_instance(h), after.attribute_instance(h)) {
            (Some((x, a)), Some((y, b))) => {
                let base_changed = |i: &cadcraft_doc::Insert| before.block(&i.block).map(|b| b.base) != after.block(&i.block).map(|b| b.base);
                x.handle != y.handle
                    || a != b
                    || x.kind != y.kind
                    || match &x.kind {
                        EntityKind::Insert(i) => base_changed(i),
                        EntityKind::MLeader(m) => m.block.as_ref().is_some_and(base_changed),
                        _ => false,
                    }
            }
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        },
    }
}

/// Recompute an associative dimension's definition points from its objects. Attachments that
/// no longer hold (the object was erased, or the point had been moved off the object before
/// this change) are dropped.
/// `prev` is the dimension as it was before the change (when it existed).
pub fn recompute(dm: &Dimension, prev: Option<&Dimension>, before: &Drawing, after: &Drawing) -> Dimension {
    let mut nd = dm.clone();
    let mut deltas: Vec<Vec2> = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    let was = prev.unwrap_or(dm);
    nd.assoc.retain(|a| {
        let Some(old) = was.definition_point(&a.point) else { return false };
        // Only follow if the point sat on the object before the change.
        if let Some(prev) = snap_point(before, a)
            && !close(prev, old.xy())
        {
            return false;
        }
        snap_point(after, a).is_some()
    });
    for a in &nd.assoc {
        if let (Some(p), Some(old)) = (snap_point(after, a), dm.definition_point(&a.point)) {
            deltas.push(p - old.xy());
            moved.push(a.point.clone());
        }
    }
    for a in nd.assoc.clone() {
        if let (Some(p), Some(slot)) = (snap_point(after, &a), def_point_mut(&mut nd, &a.point)) {
            slot.x = p.x;
            slot.y = p.y;
        }
    }
    // A rigid move of every attached point carries the rest of the dimension along. The measured
    // origins of a linear dimension are the exception: a free one stays where it was placed.
    let linear = matches!(nd.kind, DimKind::Linear { .. } | DimKind::Aligned);
    if let Some(first) = deltas.first().copied()
        && deltas.iter().all(|d| d.dist(first) <= 1e-9 * (1.0 + first.len()))
        && first.len() > 0.0
    {
        // An ordinate's `defpt` is its fixed datum (the drawing origin); it must not follow the feature.
        let ordinate = matches!(nd.kind, DimKind::Ordinate { .. });
        for name in POINTS {
            if !(moved.iter().any(|m| m == name) || ordinate && name == "defpt")
                && !(linear && (name == "p13" || name == "p14"))
                && let Some(slot) = def_point_mut(&mut nd, name)
            {
                slot.x += first.x;
                slot.y += first.y;
            }
        }
        nd.text_mid.x += first.x;
        nd.text_mid.y += first.y;
    }
    if nd != *dm {
        nd.block = None;
        nd.measurement = cadcraft_render::dimension_in(after, &nd).measurement;
    }
    nd
}

fn recompute_raw(h: Handle, dm: &Dimension, before: &Drawing, after: &Drawing) -> Dimension {
    let mut active = dm.clone();
    active
        .assoc
        .retain(|a| src_changed(before, after, a.handle) || matches!(a.snap, AssocSnap::Intersection {other} if src_changed(before,after,other)));
    let mut updated = recompute(&active, Some(dm), before, after);
    if !after.raw_dimassoc_targets(h).all(|target| src_changed(before, after, target)) {
        for name in POINTS {
            if !active.assoc.iter().any(|a| a.point == *name)
                && let Some(original) = dm.definition_point(name)
                && let Some(point) = def_point_mut(&mut updated, name)
            {
                *point = original;
            }
        }
        updated.text_mid = dm.text_mid;
    }
    updated.assoc = dm.assoc.clone();
    if updated != *dm {
        updated.block = None;
        updated.measurement = cadcraft_render::dimension_in(after, &updated).measurement;
    }
    updated
}

/// After a change: update every associative dimension whose objects changed.
/// Returns the number of dimensions updated.
pub fn update(after: &mut Arc<Drawing>, before: &Drawing) -> usize {
    let stale = after.stale_raw_dimassocs();
    let retired = if stale.is_empty() { 0 } else { Arc::make_mut(after).retire_raw_dimassocs(&stale).len() };
    let mut changes: Vec<(Handle, Dimension)> = Vec::new();
    let mut blocked = std::collections::BTreeSet::new();
    {
        let d: &Drawing = after;
        let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
        for store in stores {
            for e in store.iter() {
                let EntityKind::Dimension(dm) = &e.kind else { continue };
                // Only changed references need evaluation; unchanged opaque points stay put.
                if d.has_raw_dimassoc(e.handle) && !d.raw_dimassoc_can_update(e.handle, |target| src_changed(before, d, target)) {
                    if d.raw_dimassoc_targets(e.handle).any(|target| src_changed(before, d, target)) {
                        blocked.insert(e.handle);
                    }
                    continue;
                }
                if dm.assoc.is_empty() {
                    continue;
                }
                let touched = dm.assoc.iter().any(|a| {
                    src_changed(before, d, a.handle) || matches!(a.snap, AssocSnap::Intersection { other } if src_changed(before, d, other))
                });
                if !touched {
                    continue;
                }
                let prev = match before.entity(e.handle).map(|x| &x.kind) {
                    Some(EntityKind::Dimension(p)) => Some(p),
                    _ => None,
                };
                let nd = if d.has_raw_dimassoc(e.handle) { recompute_raw(e.handle, dm, before, d) } else { recompute(dm, prev, before, d) };
                if nd != *dm {
                    changes.push((e.handle, nd));
                }
            }
        }
    }
    if changes.is_empty() && blocked.is_empty() {
        return retired;
    }
    let d = Arc::make_mut(after);
    d.mark_raw_dimassocs_unupdated(&blocked);
    let n = changes.len();
    for (h, nd) in changes {
        let _ = d.modify_entity(h, |e| e.kind = EntityKind::Dimension(nd));
    }
    n + retired
}

/// A visible warning for opaque links, either all of them at save or those touched by an edit.
pub(crate) fn unupdated_note(d: &Drawing, before: Option<&Drawing>) -> Option<String> {
    let dimensions: Vec<_> = d
        .raw_dimassoc_dimensions()
        .filter(|h| {
            d.raw_dimassoc_unupdated(*h)
                && d.entity_including_blocks(*h).is_some_and(|e| matches!(e.kind, EntityKind::Dimension(_)))
                && before.is_none_or(|before| d.raw_dimassoc_targets(*h).any(|target| src_changed(before, d, target)))
        })
        .map(|h| h.hex())
        .collect();
    (!dimensions.is_empty())
        .then(|| format!("{} associative dimensions keep their AutoCAD link but were not updated: {}", dimensions.len(), dimensions.join(", ")))
}

/// The post-command pass: called by `Session::execute` and when an interactive command
/// finishes. `uid` (when known) is the document the command started in.
pub fn after_command(s: &mut crate::Session, before: Option<&Arc<Drawing>>, uid: Option<u64>) {
    let Some(before) = before else { return };
    let mut notes = Vec::new();
    if let Ok(st) = s.state_mut()
        && uid.is_none_or(|u| u == st.uid)
        && !Arc::ptr_eq(before, &st.doc)
    {
        update(&mut st.doc, before);
        notes = crate::cmd::hatch::refill_associative(&mut st.doc, before);
        notes.extend(unupdated_note(&st.doc, Some(before)));
    }
    notes.into_iter().for_each(|n| s.echo(n));
}

use super::common::T;
use super::{AssocSnap, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, Tag, Vec2, Vec3};

/// Associativity from a DIMASSOC object (files from other writers). The dimension kind decides
/// which definition points the references name, in the order AutoCAD writes them: `p13`/`p14`
/// for linear, aligned and arc-length dimensions, both ends of both lines (`p13`/`p14`, `defpt`/`p15`) for
/// angular dimensions, the three points of an angular-3-point dimension, `p13` for ordinate
/// dimensions, and for radial or diametric dimensions the single circle reference
/// ([`radial_links`]). CADCraft's own xdata links, when present, win.
pub(super) fn dimassoc(d: &mut Drawing, tags: &[Tag]) {
    let Some((start, dim_h)) = cadcraft_doc::foreign::dimassoc_reference(tags.iter().map(|t| (t.code, t.str()))) else { return };
    let tail = tags.get(start..).unwrap_or(&[]);
    let Some(EntityKind::Dimension(dm)) = d.entity_including_blocks(dim_h).map(|e| &e.kind) else { return };
    if !dm.assoc.is_empty() {
        return;
    }
    let links = links_of(d, dim_h, dm, tail);
    if !links.is_empty() {
        let _ = d.modify_entity_including_blocks(dim_h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.assoc = links;
            }
        });
    }
}

/// The links a DIMASSOC object (`tail`: its groups after the dimension pointer) gives dimension `dim_h`, which the reader sets and which
/// the writer compares against the dimension's own to know whether the standard object says all of them.
pub(crate) fn links_of(d: &Drawing, dim_h: Handle, dm: &Dimension, tail: &[Tag]) -> Vec<DimAssoc> {
    // A radial or diametric dimension names its circle once, by its centre.
    if matches!(dm.kind, DimKind::Radius | DimKind::Diameter) {
        return radial_links(d, dim_h, dm, tail);
    }
    let slots = dm.kind.association_points();
    let flags = T(tail).i(90).unwrap_or(0);
    let used: Vec<&str> = slots.iter().enumerate().filter(|(i, _)| flags & (1 << i) != 0).map(|(_, s)| *s).collect();
    // Point references start at each `1 AcDbOsnapPointRef`.
    let starts: Vec<usize> = tail.iter().enumerate().filter(|(_, x)| x.code == 1).map(|(i, _)| i).take(8).collect();
    let mut links = Vec::new();
    for (k, s) in starts.iter().enumerate() {
        let Some(point) = used.get(k) else { break };
        let end = starts.get(k + 1).copied().unwrap_or(tail.len());
        let r = T(tail.get(s + 1..end).unwrap_or(&[]));
        let Some(main) = r.s(331).and_then(|h| Handle::parse_hex(&h)) else { continue };
        let other = r.s(332).and_then(|h| Handle::parse_hex(&h));
        let Some(at) = dm.definition_point(point).map(|p| p.xy()) else { continue };
        // A polyline's segment (subentity type 73 = 2, edge; 91 its 1-based index).
        let segment = r.i(91).filter(|k| *k >= 1 && r.i(73) == Some(2));
        if let Some(snap) = infer_snap(d, dim_h, r.i(72).unwrap_or(0), main, other, at, segment) {
            links.push(DimAssoc { point: (*point).to_string(), handle: main, snap });
        }
    }
    links
}

/// Associations of a radial or diametric dimension from its one standard reference: the circle
/// or arc it measures, by its centre (osnap 3). The definition points are then on that circle,
/// so the dimension follows its radius or diameter.
fn radial_links(d: &Drawing, owner: Handle, dm: &Dimension, tail: &[Tag]) -> Vec<DimAssoc> {
    let starts: Vec<usize> = tail.iter().enumerate().filter(|(_, x)| x.code == 1 && x.str() == "AcDbOsnapPointRef").map(|(i, _)| i).take(8).collect();
    for (k, s) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(tail.len());
        let r = T(tail.get(s + 1..end).unwrap_or(&[]));
        if !matches!(r.i(72), Some(3 | 4)) {
            continue;
        }
        let Some(handle) = r.s(331).and_then(|h| Handle::parse_hex(&h)) else { continue };
        let Some(centre) = circle_centre(d, owner, handle) else { continue };
        let on = |point: &str, p: Vec3| {
            let angle = centre.angle_to(p.xy());
            angle.is_finite().then(|| DimAssoc { point: point.into(), handle, snap: AssocSnap::OnCircle { angle } })
        };
        let links = match dm.kind {
            DimKind::Radius => on("p15", dm.p15).map(|p15| vec![DimAssoc { point: "defpt".into(), handle, snap: AssocSnap::Center }, p15]),
            DimKind::Diameter => [on("defpt", dm.defpt), on("p15", dm.p15)].into_iter().collect::<Option<Vec<_>>>(),
            _ => None,
        };
        if let Some(links) = links {
            return links;
        }
    }
    Vec::new()
}

/// The planar centre of a circle or arc, or `None` for any other entity: the point a radial or
/// diametric dimension's single DIMASSOC reference names.
fn circle_centre(d: &Drawing, owner: Handle, h: Handle) -> Option<Vec2> {
    match &d.association_entity(owner, h)?.kind {
        EntityKind::Circle(c) => Some(c.center.xy()),
        EntityKind::Arc(a) => Some(a.center.xy()),
        _ => None,
    }
}

/// Endpoint identity comes from the dimension's own definition point, never the point-ref
/// cache (which may be absent or stale after a move). Ambiguous/off-curve identities stay raw.
fn endpoint(a: Vec2, b: Vec2, at: Vec2) -> Option<AssocSnap> {
    let (da, db) = (a.dist(at), b.dist(at));
    let tolerance = 1e-6 * (1.0 + a.dist(b));
    if da.min(db) > tolerance || da == db {
        return None;
    }
    Some(if da < db { AssocSnap::Start } else { AssocSnap::End })
}

fn infer_snap(d: &Drawing, owner: Handle, osnap: i64, main: Handle, other: Option<Handle>, at: Vec2, segment: Option<i64>) -> Option<AssocSnap> {
    let e = d.association_entity(owner, main)?;
    if let (EntityKind::LwPolyline(p), Some(k), 1 | 13) = (&e.kind, segment, osnap) {
        let n = p.vertices.len();
        let segments = if p.closed { n } else { n.saturating_sub(1) };
        let k = usize::try_from(k).ok().filter(|k| *k >= 1 && *k <= segments)?;
        let index = if osnap == 13 {
            k - 1
        } else {
            match endpoint(p.vertices.get(k - 1)?.p, p.vertices.get(k % n)?.p, at)? {
                AssocSnap::Start => k - 1,
                AssocSnap::End => k % n,
                _ => return None,
            }
        };
        return Some(AssocSnap::Vertex { index });
    }
    Some(match osnap {
        1 | 13 => match &e.kind {
            EntityKind::Line(l) => {
                if osnap == 13 {
                    AssocSnap::Start
                } else {
                    endpoint(l.a.xy(), l.b.xy(), at)?
                }
            }
            EntityKind::Arc(a) => {
                if osnap == 13 {
                    AssocSnap::Start
                } else {
                    let arc = cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
                    endpoint(arc.start_point(), arc.end_point(), at)?
                }
            }
            EntityKind::LwPolyline(p) => {
                let (index, vertex) = p.vertices.iter().enumerate().min_by(|x, y| x.1.p.dist(at).total_cmp(&y.1.p.dist(at)))?;
                if vertex.p.dist(at) > 1e-6 * (1.0 + at.x.abs().max(at.y.abs())) {
                    return None;
                }
                AssocSnap::Vertex { index }
            }
            EntityKind::Point(_) => AssocSnap::Start,
            _ => return None,
        },
        2 => AssocSnap::Mid,
        3 | 4 => AssocSnap::Center,
        // Autodesk: 5 Quadrant, 8 Perpendicular, 10 Nearest. Do not infer 8 or 10.
        5 => {
            let (center, radius) = match &e.kind {
                EntityKind::Circle(c) => (c.center, c.radius),
                EntityKind::Arc(a) => (a.center, a.radius),
                _ => return None,
            };
            let v = at - center.xy();
            let angle = v.y.atan2(v.x);
            let quarter = angle / std::f64::consts::FRAC_PI_2;
            if radius <= 0.0 || (v.len() - radius).abs() > 1e-6 * (1.0 + radius.abs()) || (quarter - quarter.round()).abs() > 1e-6 {
                return None;
            }
            AssocSnap::OnCircle { angle: quarter.round() * std::f64::consts::FRAC_PI_2 }
        }
        6 => AssocSnap::Intersection { other: other.filter(|h| d.association_entity(owner, *h).is_some())? },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Circle;
    #[test]
    fn nearest_and_perpendicular_are_not_inferred_as_fixed_circle_points() {
        let mut d = Drawing::new_metric();
        let h = d.add(&cadcraft_doc::Space::Model, Default::default(), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 10.0 })).unwrap();
        for code in [8, 10] {
            assert!(infer_snap(&d, h, code, h, None, Vec2::new(10.0, 0.0), None).is_none(), "unsupported Autodesk osnap {code}");
        }
        assert!(matches!(infer_snap(&d, h, 5, h, None, Vec2::new(10.0, 0.0), None), Some(AssocSnap::OnCircle { .. })));
    }
}

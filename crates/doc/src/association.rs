//! Supported dimension snap semantics, shared by the engine and format checks.
use crate::geom::Vec2;
use crate::{AssocSnap, DimAssoc, Entity, EntityKind, Handle};

enum ResolutionError {
    Unsupported,
    Unresolvable,
}

/// The entity a link names, when its projected 2D geometry may be read: an entity with canonical geometry
/// (its own revision or a nested one) is not given to a snap that reads `xy()` fields, so its links are unsupported.
fn source<'a>(entity: &impl Fn(Handle) -> Option<&'a Entity>, handle: Handle) -> Result<&'a Entity, ResolutionError> {
    let e = entity(handle).ok_or(ResolutionError::Unsupported)?;
    e.require_legacy_geometry().map_err(|_| ResolutionError::Unsupported)?;
    Ok(e)
}

fn resolve<'a>(link: &DimAssoc, entity: impl Fn(Handle) -> Option<&'a Entity>) -> Result<Vec2, ResolutionError> {
    let e = source(&entity, link.handle)?;
    let arc = |a: &crate::Arc| crate::geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
    let point = match (&e.kind, &link.snap) {
        (EntityKind::Line(l), AssocSnap::Start) => Some(l.a.xy()),
        (EntityKind::Line(l), AssocSnap::End) => Some(l.b.xy()),
        (EntityKind::Line(l), AssocSnap::Mid) => Some(l.a.xy().mid(l.b.xy())),
        (EntityKind::Line(l), AssocSnap::Intersection { other }) => match &source(&entity, *other)?.kind {
            EntityKind::Line(m) => crate::geom::line_line_infinite(l.a.xy(), l.b.xy(), m.a.xy(), m.b.xy()).map(|x| x.0),
            _ => return Err(ResolutionError::Unsupported),
        },
        (EntityKind::Arc(a), AssocSnap::Start) => Some(arc(a).start_point()),
        (EntityKind::Arc(a), AssocSnap::End) => Some(arc(a).end_point()),
        (EntityKind::Arc(a), AssocSnap::Mid) => Some(arc(a).mid_point()),
        (EntityKind::Arc(a), AssocSnap::Center) => Some(a.center.xy()),
        (EntityKind::Arc(a), AssocSnap::OnCircle { angle }) => Some(a.center.xy() + Vec2::from_angle(*angle) * a.radius),
        (EntityKind::Circle(c), AssocSnap::Center) => Some(c.center.xy()),
        (EntityKind::Circle(c), AssocSnap::OnCircle { angle }) => Some(c.center.xy() + Vec2::from_angle(*angle) * c.radius),
        (EntityKind::LwPolyline(p), AssocSnap::Vertex { index }) => p.vertices.get(*index).map(|v| v.p),
        (EntityKind::LwPolyline(p), AssocSnap::Start) => p.vertices.first().map(|v| v.p),
        (EntityKind::LwPolyline(p), AssocSnap::End) => p.vertices.last().map(|v| v.p),
        (EntityKind::Point(p), AssocSnap::Start | AssocSnap::Center) => Some(p.p.xy()),
        _ => return Err(ResolutionError::Unsupported),
    };
    point.filter(|p| p.is_finite()).ok_or(ResolutionError::Unresolvable)
}

/// Whether this snap/geometry pair is implemented, independent of whether its current
/// geometry has a solution (parallel lines, for example, still support intersection semantics).
pub fn association_supported<'a>(link: &DimAssoc, entity: impl Fn(Handle) -> Option<&'a Entity>) -> bool {
    !matches!(resolve(link, entity), Err(ResolutionError::Unsupported))
}

/// Resolve against the current geometry. Supported semantics do not guarantee a current point.
pub fn association_point<'a>(link: &DimAssoc, entity: impl Fn(Handle) -> Option<&'a Entity>) -> Option<Vec2> {
    resolve(link, entity).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Vec3;
    #[test]
    fn supported_intersection_is_not_the_same_as_a_current_solution() {
        let a = Entity::new(Handle(1), EntityKind::Line(crate::Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) }));
        let b = Entity::new(Handle(2), EntityKind::Line(crate::Line { a: Vec3::new(0.0, 5.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) }));
        let lookup = |h| {
            if h == a.handle {
                Some(&a)
            } else if h == b.handle {
                Some(&b)
            } else {
                None
            }
        };
        let mut link = DimAssoc { point: "p13".into(), handle: a.handle, snap: AssocSnap::Intersection { other: b.handle } };
        assert!(association_supported(&link, lookup));
        assert!(association_point(&link, lookup).is_none());
        link.snap = AssocSnap::Center;
        assert!(!association_supported(&link, lookup), "Center/Line is unsupported, not just temporarily unsolvable");
    }

    #[test]
    fn a_source_with_canonical_geometry_is_unsupported_not_read_as_projected_2d() {
        let line =
            |h: u64, y: f64| Entity::new(Handle(h), EntityKind::Line(crate::Line { a: Vec3::new(0.0, y, 0.0), b: Vec3::new(10.0, y + 5.0, 3.0) }));
        let (mut a, mut b) = (line(1, 0.0), line(2, 5.0));
        let ask = |a: &Entity, b: &Entity, snap: AssocSnap| {
            let link = DimAssoc { point: "p13".into(), handle: Handle(1), snap };
            let lookup = |h: Handle| [a, b].into_iter().find(|e| e.handle == h);
            (association_supported(&link, lookup), association_point(&link, lookup))
        };
        assert_eq!(ask(&a, &b, AssocSnap::End), (true, Some(crate::geom::Vec2::new(10.0, 5.0))));
        // The source's own revision.
        a.geometry_revision = crate::GeometryRevision::CanonicalV2;
        assert_eq!(ask(&a, &b, AssocSnap::End), (false, None));
        // The other line of an intersection.
        a.geometry_revision = crate::GeometryRevision::LegacyProjectedV1;
        b.geometry_revision = crate::GeometryRevision::CanonicalV2;
        assert_eq!(ask(&a, &b, AssocSnap::Intersection { other: Handle(2) }), (false, None));
        b.geometry_revision = crate::GeometryRevision::LegacyProjectedV1;
        assert!(ask(&a, &b, AssocSnap::Intersection { other: Handle(2) }).0);
    }
}

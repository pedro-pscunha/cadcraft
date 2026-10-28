//! Supported dimension snap semantics, shared by the engine and format reproducibility checks.

use crate::geom::Vec2;
use crate::{AssocSnap, DimAssoc, Entity, EntityKind, Handle};

/// Resolve one modelled association through the caller's entity scope. An unsupported pair is
/// not modelled merely because its DXF groups can be serialized; callers must retain it raw.
pub fn association_point<'a>(link: &DimAssoc, entity: impl Fn(Handle) -> Option<&'a Entity>) -> Option<Vec2> {
    let e = entity(link.handle)?;
    let arc = |a: &crate::Arc| crate::geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
    match (&e.kind, &link.snap) {
        (EntityKind::Line(l), AssocSnap::Start) => Some(l.a.xy()),
        (EntityKind::Line(l), AssocSnap::End) => Some(l.b.xy()),
        (EntityKind::Line(l), AssocSnap::Mid) => Some(l.a.xy().mid(l.b.xy())),
        (EntityKind::Line(l), AssocSnap::Intersection { other }) => match &entity(*other)?.kind {
            EntityKind::Line(m) => crate::geom::line_line_infinite(l.a.xy(), l.b.xy(), m.a.xy(), m.b.xy()).map(|x| x.0),
            _ => None,
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
        _ => None,
    }
    .filter(|p| p.is_finite())
}

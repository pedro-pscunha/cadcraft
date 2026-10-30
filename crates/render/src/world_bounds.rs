//! Bounds from the same spatial geometry and font layouts that display consumes.

use cadcraft_doc::{
    Arc, DocError, Drawing, Entity, EntityKind, GeometryRevision, Handle, LwPolyline, Prim, WorldGeometry, WorldPrimitive, WorldTextContent,
};
use cadcraft_geom::{Bounds2, Mat4, bulge_to_arc};

use crate::{WorldShaped, WorldTextLayout, layout_world_text};

/// Bounds in the XY view after the complete spatial parent, or an explicit geometry refusal.
pub fn entity_world_bounds(drawing: &Drawing, entity: &Entity, parent: &Mat4) -> cadcraft_doc::Result<Bounds2> {
    let mut envelopes = Bounds2::EMPTY;
    let geometry = crate::world_geometry::entity_world_geometry_visiting(drawing, entity, parent, &mut |source, complete_parent| {
        if source.geometry_revision == GeometryRevision::CanonicalV2
            && let EntityKind::LwPolyline(polyline) = &source.kind
        {
            envelopes = envelopes.union(&wide_bulge_bounds(drawing, source, polyline, complete_parent)?);
        }
        Ok(())
    })?;
    let bounds = geometry
        .primitives()
        .iter()
        .try_fold(Bounds2::EMPTY, |bounds, primitive| Ok::<_, DocError>(bounds.union(&primitive_bounds(drawing, primitive, entity.handle)?)))?;
    Ok(bounds.union(&envelopes))
}

fn wide_bulge_bounds(drawing: &Drawing, entity: &Entity, polyline: &LwPolyline, parent: &Mat4) -> cadcraft_doc::Result<Bounds2> {
    let count = if polyline.closed { polyline.vertices.len() } else { polyline.vertices.len().saturating_sub(1) };
    let mut bounds = Bounds2::EMPTY;
    for (a, b) in polyline.vertices.iter().zip(polyline.vertices.iter().cycle().skip(1)).take(count) {
        let half = if polyline.const_width > 0.0 { polyline.const_width / 2.0 } else { a.start_width.max(a.end_width).max(0.0) / 2.0 };
        if half <= 0.0 {
            continue;
        }
        let Some((local, _)) = bulge_to_arc(a.p, b.p, a.bulge) else { continue };
        // The sampled strip can miss an extremum after an affine parent. These two conics enclose every tapered radial edge.
        for radius in [local.radius - half, local.radius + half] {
            let turn = if radius < 0.0 { std::f64::consts::PI } else { 0.0 };
            let edge = Arc { center: local.center.to3(polyline.elevation), radius: radius.abs(), start: local.start + turn, end: local.end + turn };
            for primitive in cadcraft_doc::arc_primitives(&edge, entity.common.extrusion, parent)? {
                bounds = bounds.union(&primitive_bounds(drawing, &primitive, entity.handle)?);
            }
        }
    }
    Ok(bounds)
}

fn primitive_bounds(drawing: &Drawing, primitive: &WorldPrimitive, owner: Handle) -> cadcraft_doc::Result<Bounds2> {
    let bounds = match primitive {
        WorldPrimitive::Text(text) => {
            let value = match &text.content {
                WorldTextContent::Text(text) => text.value.as_str(),
                WorldTextContent::MText(text) => text.contents.as_str(),
            };
            match layout_world_text(drawing, text, value)? {
                WorldTextLayout::Text(shaped) => shaped_bounds(&shaped, owner)?,
                WorldTextLayout::MText(pieces) => {
                    pieces.iter().try_fold(Bounds2::EMPTY, |bounds, piece| Ok::<_, DocError>(bounds.union(&shaped_bounds(&piece.shaped, owner)?)))?
                }
            }
        }
        WorldPrimitive::Fill(fill) => fill
            .loops
            .iter()
            .flatten()
            .try_fold(Bounds2::EMPTY, |bounds, edge| Ok::<_, DocError>(bounds.union(&primitive_bounds(drawing, edge, owner)?)))?,
        _ => WorldGeometry::from_primitives(owner, vec![primitive.clone()])?.project_xy()?.into_iter().fold(Bounds2::EMPTY, |bounds, primitive| {
            let next = match primitive {
                Prim::Seg(segment) => segment.bounds(),
                Prim::Circle(circle) => circle.bounds(),
                Prim::Ellipse(ellipse) => ellipse.bounds(),
                Prim::Spline(spline) => spline.bounds(),
                Prim::Point(point) => Bounds2::from_points([point]),
                Prim::Fill(points) => Bounds2::from_points(points),
                Prim::Infinite { .. } => Bounds2::EMPTY,
            };
            bounds.union(&next)
        }),
    };
    if !bounds.is_empty() && (!bounds.min.is_finite() || !bounds.max.is_finite()) {
        return Err(DocError::NonFinite(owner));
    }
    Ok(bounds)
}

fn shaped_bounds(shaped: &WorldShaped, owner: Handle) -> cadcraft_doc::Result<Bounds2> {
    let mut bounds = Bounds2::EMPTY;
    for point in shaped.strokes.iter().flatten().chain(shaped.glyphs.iter().flatten().flatten()) {
        if !point.is_finite() {
            return Err(DocError::NonFinite(owner));
        }
        bounds.add(point.xy());
    }
    Ok(bounds)
}

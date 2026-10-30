//! Bounds from the same spatial geometry and font layouts that display consumes.

use cadcraft_doc::{DocError, Drawing, Entity, Handle, Prim, WorldGeometry, WorldPrimitive, WorldTextContent};
use cadcraft_geom::{Bounds2, Mat4};

use crate::{WorldShaped, WorldTextLayout, entity_world_geometry, layout_world_text};

/// Bounds in the XY view after the complete spatial parent, or an explicit geometry refusal.
pub fn entity_world_bounds(drawing: &Drawing, entity: &Entity, parent: &Mat4) -> cadcraft_doc::Result<Bounds2> {
    let geometry = entity_world_geometry(drawing, entity, parent)?;
    geometry.primitives().iter().try_fold(Bounds2::EMPTY, |bounds, primitive| Ok(bounds.union(&primitive_bounds(drawing, primitive, entity.handle)?)))
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

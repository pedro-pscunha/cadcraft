//! Resolve style-dependent layouts before the shared WCS adapter places their geometry.

use cadcraft_doc::{Drawing, Entity, EntityKind, GeometryRevision, WorldGeometry, WorldLayout, has_finite_numbers};
use cadcraft_geom::Mat4;

/// Canonical WCS geometry with the same local layouts that the drawing displays.
pub fn entity_world_geometry(drawing: &Drawing, entity: &Entity, parent: &Mat4) -> cadcraft_doc::Result<WorldGeometry> {
    if entity.geometry_revision == GeometryRevision::LegacyProjectedV1 || !has_finite_numbers(entity) {
        return entity.world_geometry(parent);
    }
    match &entity.kind {
        EntityKind::Table(table) => {
            let layout = crate::table_layout(drawing, table);
            entity.world_geometry_with_layout(parent, Some(WorldLayout::Table(&layout)))
        }
        EntityKind::Tolerance(tolerance) => {
            let layout = crate::tolerance_layout(drawing, tolerance);
            let style = drawing.dim_style(&tolerance.style).cloned().unwrap_or_default();
            entity.world_geometry_with_layout(parent, Some(WorldLayout::Tolerance { layout: &layout, text_style: &style.text_style }))
        }
        EntityKind::Leader(leader) => {
            let arrow = crate::leader_arrow_layout(drawing, leader);
            entity.world_geometry_with_layout(parent, Some(WorldLayout::Leader(&arrow)))
        }
        _ => entity.world_geometry(parent),
    }
}

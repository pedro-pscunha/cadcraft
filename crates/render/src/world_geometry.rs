//! Resolve style-dependent layouts before the shared WCS adapter places their geometry.

use cadcraft_doc::{
    DocError, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, WorldGeometry, WorldLayout, WorldPrimitive, attrib_primitives,
    has_finite_numbers,
};
use cadcraft_geom::Mat4;

/// Canonical WCS geometry with the same local layouts that the drawing displays.
pub fn entity_world_geometry(drawing: &Drawing, entity: &Entity, parent: &Mat4) -> cadcraft_doc::Result<WorldGeometry> {
    let mut remaining = crate::MAX_BLOCK_EXPANSION;
    geometry_in(drawing, entity, parent, 0, &mut remaining)
}

fn geometry_in(drawing: &Drawing, entity: &Entity, parent: &Mat4, depth: usize, remaining: &mut usize) -> cadcraft_doc::Result<WorldGeometry> {
    if entity.geometry_revision == GeometryRevision::LegacyProjectedV1 {
        return entity.world_geometry(parent);
    }
    if !has_finite_numbers(entity) || !has_finite_numbers(parent) {
        return Err(DocError::NonFinite(entity.handle));
    }
    if parent.m[3] != [0.0, 0.0, 0.0, 1.0] {
        return Err(DocError::Invalid(format!("Entity {} needs a finite affine parent.", entity.handle.hex())));
    }
    if depth > cadcraft_doc::MAX_BLOCK_DEPTH {
        return Err(DocError::Invalid("The canonical block references nest too deeply.".into()));
    }
    spend(remaining, 1, entity.handle)?;
    match &entity.kind {
        EntityKind::Insert(insert) => insert_geometry(drawing, entity, insert, parent, depth, remaining),
        EntityKind::MLeader(leader) => {
            let block = leader.block.as_ref().and_then(|insert| drawing.block(&insert.block));
            let base = block.map_or(cadcraft_geom::Vec3::ZERO, |block| block.base);
            let arrow = crate::mleader_arrow_layout(drawing, leader);
            let mode = drawing.header.i64("ATTMODE", 1);
            let shown = |attribute: &cadcraft_doc::Attrib| mode != 0 && (!attribute.invisible || mode == 2);
            let mut visible = std::borrow::Cow::Borrowed(leader);
            if leader.block.as_ref().is_some_and(|block| block.attribs.iter().any(|attribute| !shown(attribute)))
                && let Some(content) = &mut visible.to_mut().block
            {
                content.attribs.retain(shown);
            }
            let world = cadcraft_doc::mleader_primitives(&visible, base, &arrow, parent)?;
            let mut primitives = world.primitives;
            if let (Some(block), Some(matrix)) = (block, world.block) {
                spend(remaining, 1, entity.handle)?;
                append_block_geometry(drawing, block, &matrix, depth, remaining, &mut primitives)?;
            }
            WorldGeometry::from_primitives(entity.handle, primitives)
        }
        EntityKind::Dimension(dimension) => {
            let layout = crate::dimension_layout(drawing, dimension, entity.common.extrusion)?;
            entity.world_geometry_with_layout(parent, Some(WorldLayout::Dimension(&layout)))
        }
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

fn append_block_geometry(
    drawing: &Drawing,
    block: &cadcraft_doc::Block,
    parent: &Mat4,
    depth: usize,
    remaining: &mut usize,
    primitives: &mut Vec<WorldPrimitive>,
) -> cadcraft_doc::Result<()> {
    for child in block.entities.iter() {
        // Definitions are replaced by instances; unknown records have no modeled display geometry.
        if matches!(child.kind, EntityKind::AttDef(_) | EntityKind::Unknown(_))
            || !child.common.visible
            || (child.common.layer != "0" && drawing.layer(&child.common.layer).is_some_and(|layer| !layer.visible()))
        {
            continue;
        }
        let geometry = geometry_in(drawing, child, parent, depth + 1, remaining)?;
        primitives.extend_from_slice(geometry.primitives());
    }
    Ok(())
}

fn spend(remaining: &mut usize, count: usize, owner: Handle) -> cadcraft_doc::Result<()> {
    *remaining = remaining
        .checked_sub(count)
        .ok_or_else(|| DocError::Invalid(format!("Entity {} exceeds the canonical block expansion limit.", owner.hex())))?;
    Ok(())
}

fn insert_geometry(
    drawing: &Drawing,
    entity: &Entity,
    insert: &Insert,
    parent: &Mat4,
    depth: usize,
    remaining: &mut usize,
) -> cadcraft_doc::Result<WorldGeometry> {
    let mut primitives: Vec<WorldPrimitive> = Vec::new();
    if let Some(block) = drawing.block(&insert.block) {
        if insert.cols == 0 || insert.rows == 0 {
            return Err(DocError::Invalid(format!("Entity {} needs positive INSERT row and column counts.", entity.handle.hex())));
        }
        let copies = u64::from(insert.cols) * u64::from(insert.rows);
        if copies > *remaining as u64 {
            return Err(DocError::Invalid(format!("Entity {} exceeds the canonical block expansion limit.", entity.handle.hex())));
        }
        spend(remaining, copies as usize, entity.handle)?;
        for row in 0..insert.rows {
            for column in 0..insert.cols {
                let local = insert
                    .world_transform(entity.common.extrusion, block.base, column, row)
                    .map_err(|error| if matches!(error, DocError::NonFinite(_)) { DocError::NonFinite(entity.handle) } else { error })?;
                let complete = parent.mul(&local);
                append_block_geometry(drawing, block, &complete, depth, remaining, &mut primitives)?;
            }
        }
    }
    let mode = drawing.header.i64("ATTMODE", 1);
    for attribute in &insert.attribs {
        if mode == 0 || (attribute.invisible && mode != 2) {
            continue;
        }
        let definition = drawing.block(&insert.block).and_then(|block| {
            block
                .entities
                .iter()
                .find(|entity| matches!(&entity.kind, EntityKind::AttDef(definition) if definition.tag.eq_ignore_ascii_case(&attribute.tag)))
        });
        let layer = attribute.props.layer.as_deref().or_else(|| definition.map(|entity| entity.common.layer.as_str())).unwrap_or("0");
        if definition.is_some_and(|entity| !entity.common.visible) || (layer != "0" && drawing.layer(layer).is_some_and(|layer| !layer.visible())) {
            continue;
        }
        if attribute.geometry_revision != GeometryRevision::CanonicalV2
            || attribute.multiline.as_ref().is_some_and(|text| text.geometry_revision != GeometryRevision::CanonicalV2)
        {
            return Err(DocError::Invalid(format!("Entity {} has a legacy attribute payload that needs recovery.", entity.handle.hex())));
        }
        spend(remaining, 1, attribute.handle.unwrap_or(entity.handle))?;
        // Instance text owns its plane and is already placed in its containing space, outside the INSERT matrix.
        primitives.extend(attrib_primitives(attribute, parent)?);
    }
    WorldGeometry::from_primitives(entity.handle, primitives)
}

//! Paint canonical geometry only after its complete spatial parent reaches the view.

use cadcraft_doc::{DocError, Entity, EntityKind, Handle, Prim, WorldFillKind, WorldGeometry, WorldPrimitive, WorldTextContent};
use cadcraft_geom::Mat3;

use super::{Builder, Ctx, Ink, WorldTextLayout, fill, layout_world_text};

pub(crate) fn draw(
    builder: &mut Builder,
    context: &Ctx,
    entity: &Entity,
    color: Ink,
    lineweight: f32,
    linetype: Option<&cadcraft_doc::Linetype>,
) -> cadcraft_doc::Result<()> {
    if !builder.opts.text && matches!(entity.kind, EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_)) {
        return Ok(());
    }
    if context.depth > cadcraft_doc::MAX_BLOCK_DEPTH {
        return Err(DocError::Invalid("The canonical block references nest too deeply.".into()));
    }
    if let EntityKind::Insert(insert) = &entity.kind {
        return super::blocks::canonical_insert(builder, context, entity, insert, color);
    }
    if let EntityKind::MLeader(leader) = &entity.kind
        && leader.block.is_some()
    {
        return Err(unsupported("multileader block-content paint roles"));
    }
    if entity.common.thickness != 0.0
        && matches!(
            entity.kind,
            EntityKind::Line(_)
                | EntityKind::Point(_)
                | EntityKind::Circle(_)
                | EntityKind::Arc(_)
                | EntityKind::Ellipse(_)
                | EntityKind::LwPolyline(_)
                | EntityKind::Text(_)
                | EntityKind::AttDef(_)
                | EntityKind::Solid(_)
                | EntityKind::Trace(_)
        )
    {
        return Err(unsupported("extrusion thickness"));
    }
    if matches!(entity.kind, EntityKind::Table(_) | EntityKind::Tolerance(_) | EntityKind::Dimension(_) | EntityKind::MLine(_)) {
        return Err(unsupported("annotation paint roles"));
    }
    if let EntityKind::LwPolyline(polyline) = &entity.kind
        && (polyline.const_width > 0.0 || polyline.vertices.iter().any(|v| v.start_width > 0.0 || v.end_width > 0.0))
    {
        return Err(unsupported("wide-polyline paint roles"));
    }
    if matches!(entity.kind, EntityKind::Point(_)) && context.d.header.i64("PDMODE", 0) != 0 {
        return Err(unsupported("point-marker frames"));
    }
    if linetype.is_some()
        && !matches!(
            entity.kind,
            EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::Solid(_) | EntityKind::Trace(_) | EntityKind::Hatch(_) | EntityKind::Wipeout(_)
        )
    {
        return Err(unsupported("spatial line patterns"));
    }
    let geometry = super::entity_world_geometry(context.d, entity, &context.world)?;
    let projected = Ctx { xf: Mat3::IDENTITY, ..context.clone() };
    if matches!(entity.kind, EntityKind::Solid(_) | EntityKind::Trace(_) | EntityKind::Wipeout(_)) {
        let polygon: Vec<_> = geometry
            .primitives()
            .iter()
            .filter_map(|p| match p {
                WorldPrimitive::Line { a, .. } => Some(a.xy()),
                _ => None,
            })
            .collect();
        if builder.opts.fill && (matches!(entity.kind, EntityKind::Wipeout(_)) || context.d.header.i64("FILLMODE", 1) != 0) {
            let start = builder.list.prims.len();
            builder.tris(&projected, color, &fill::triangulate_evenodd(std::slice::from_ref(&polygon)));
            if matches!(entity.kind, EntityKind::Wipeout(_))
                && let Some(primitive) = builder.list.prims.get_mut(start)
            {
                primitive.kind = super::Kind::Mask;
            }
        }
        let outline = match entity.kind {
            EntityKind::Wipeout(_) => match context.d.header.i64("WIPEOUTFRAME", 1) {
                0 => false,
                2 => !builder.plotting,
                _ => true,
            },
            _ => !builder.opts.fill || context.d.header.i64("FILLMODE", 1) == 0,
        };
        if !outline {
            return Ok(());
        }
    }
    paint_geometry(builder, &projected, &geometry, entity.handle, color, lineweight)
}

pub(crate) fn paint_geometry(
    builder: &mut Builder,
    context: &Ctx,
    geometry: &WorldGeometry,
    owner: Handle,
    color: Ink,
    lineweight: f32,
) -> cadcraft_doc::Result<()> {
    for primitive in geometry.primitives() {
        match primitive {
            WorldPrimitive::Text(text) => {
                if !builder.opts.text {
                    continue;
                }
                let value = match &text.content {
                    WorldTextContent::Text(text) => text.value.as_str(),
                    WorldTextContent::MText(text) => text.contents.as_str(),
                };
                match layout_world_text(context.d, text, value)? {
                    WorldTextLayout::Text(shaped) => builder.shaped(context, color, lineweight, &shaped.project_xy()),
                    WorldTextLayout::MText(pieces) => {
                        for piece in pieces {
                            builder.shaped(context, super::text_entity::mtext_color(piece.color, color), lineweight, &piece.shaped.project_xy());
                        }
                    }
                }
            }
            WorldPrimitive::Fill(fill) => {
                if !builder.opts.fill || context.d.header.i64("FILLMODE", 1) == 0 {
                    continue;
                }
                if !matches!(fill.kind, WorldFillKind::Solid | WorldFillKind::Wipeout) || fill.style != 0 {
                    return Err(unsupported("pattern, gradient or island paint roles"));
                }
                let projected = WorldGeometry::from_primitives(owner, vec![primitive.clone()])?.project_xy()?;
                let polygons: Vec<_> = projected
                    .into_iter()
                    .filter_map(|p| match p {
                        Prim::Fill(points) => Some(points),
                        _ => None,
                    })
                    .collect();
                let start = builder.list.prims.len();
                builder.tris(context, color, &fill::triangulate_evenodd(&polygons));
                if matches!(fill.kind, WorldFillKind::Wipeout)
                    && let Some(primitive) = builder.list.prims.get_mut(start)
                {
                    primitive.kind = super::Kind::Mask;
                }
            }
            _ => {
                for primitive in WorldGeometry::from_primitives(owner, vec![primitive.clone()])?.project_xy()? {
                    let mut points = Vec::new();
                    match primitive {
                        Prim::Seg(segment) => segment.tessellate(builder.opts.tolerance, &mut points),
                        Prim::Circle(circle) => circle.tessellate(builder.opts.tolerance, &mut points),
                        Prim::Ellipse(ellipse) => ellipse.tessellate(builder.opts.tolerance, &mut points),
                        Prim::Spline(spline) => points = spline.tessellate(builder.opts.tolerance),
                        Prim::Point(point) => builder.point(context, color, point),
                        Prim::Infinite { base, dir, ray } => builder.infinite(context, color, base, dir, ray),
                        Prim::Fill(_) => return Err(unsupported("an unexpected projected fill")),
                    }
                    builder.polyline(context, color, lineweight, &points);
                }
            }
        }
    }
    Ok(())
}

fn unsupported(feature: &str) -> DocError {
    DocError::Invalid(format!("Canonical display does not support {feature} yet. The source geometry remains unchanged."))
}

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
    linetype_scale: f64,
) -> cadcraft_doc::Result<()> {
    // An extrusion sweeps the outline along the normal: the outline, its far copy and the side edges.
    if entity.common.thickness != 0.0 && super::world_roles::swept(context.d, entity) {
        let far = super::world_roles::extrusion_offset(context, entity)?;
        draw_flat(builder, context, entity, color, lineweight, linetype, linetype_scale)?;
        let moved = Ctx { world: super::world_roles::translation(far).mul(&context.world), ..context.clone() };
        draw_flat(builder, &moved, entity, color, lineweight, linetype, linetype_scale)?;
        return super::world_roles::side_edges(builder, context, entity, far, color, lineweight);
    }
    draw_flat(builder, context, entity, color, lineweight, linetype, linetype_scale)
}

fn draw_flat(
    builder: &mut Builder,
    context: &Ctx,
    entity: &Entity,
    color: Ink,
    lineweight: f32,
    linetype: Option<&cadcraft_doc::Linetype>,
    linetype_scale: f64,
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
        && let Some(content) = &leader.block
    {
        return super::blocks::canonical_mleader_block(builder, context, entity, leader, content, color, lineweight);
    }
    if let EntityKind::Dimension(dimension) = &entity.kind
        && dimension.block.as_ref().is_some_and(|name| context.d.block(name).is_some())
    {
        return super::draw::draw_dimension(builder, context, entity, dimension, lineweight, linetype, linetype_scale);
    }
    if matches!(entity.kind, EntityKind::Table(_) | EntityKind::Tolerance(_) | EntityKind::Dimension(_)) {
        let geometry = super::entity_world_geometry(context.d, entity, &context.world)?;
        let matrix = geometry.layout_to_world().ok_or_else(|| DocError::Invalid("The annotation has no shared local-layout placement.".into()))?;
        let xy = Mat3 { a: matrix.m[0][0], b: matrix.m[1][0], c: matrix.m[0][1], d: matrix.m[1][1], e: matrix.m[0][3], f: matrix.m[1][3] };
        let local = Ctx { world: *matrix, xf: xy, ..context.clone() };
        match &entity.kind {
            EntityKind::Table(table) => {
                let at_origin = super::table::local_table(table);
                super::table::draw(builder, &local, entity, &at_origin, color, lineweight);
            }
            EntityKind::Tolerance(tolerance) => {
                let at_origin = super::tolerance::local_tolerance(tolerance);
                let tolerance = builder.opts.tolerance / super::max_scale(&xy).max(1e-12);
                super::tolerance::draw(builder, &local, entity, &at_origin, lineweight, tolerance);
            }
            EntityKind::Dimension(dimension) => {
                let localized = dimension.localized(entity.common.extrusion)?;
                super::dim_draw::dimension(builder, &local, entity, &localized, lineweight, linetype, linetype_scale);
            }
            _ => return Err(unsupported("a mismatched annotation layout")),
        }
        return Ok(());
    }
    if let EntityKind::Hatch(hatch) = &entity.kind {
        super::entity_world_geometry(context.d, entity, &context.world)?;
        let local = super::world_roles::local_context(context, entity, hatch.elevation)?;
        let tolerance = builder.opts.tolerance / super::max_scale(&local.xf).max(1e-12);
        let layout = super::hatch::local_layout(entity.handle, hatch, tolerance)?;
        super::hatch::draw(builder, &local, entity, &layout, color, lineweight, tolerance);
        return Ok(());
    }
    if let EntityKind::MLine(mline) = &entity.kind {
        super::entity_world_geometry(context.d, entity, &context.world)?;
        let layout = mline.layout_canonical(entity.common.extrusion, &context.world)?;
        let local = super::world_roles::placed_context(context, *layout.local_to_world());
        let tolerance = builder.opts.tolerance / super::max_scale(&local.xf).max(1e-12);
        super::mline::draw_geometry(builder, &local, entity, mline, layout.geometry(), color, lineweight, tolerance);
        return Ok(());
    }
    if let EntityKind::LwPolyline(polyline) = &entity.kind
        && (polyline.const_width > 0.0 || polyline.vertices.iter().any(|v| v.start_width > 0.0 || v.end_width > 0.0))
    {
        return super::world_roles::wide_polyline(builder, context, entity, polyline, color, lineweight, linetype, linetype_scale);
    }
    if let EntityKind::Point(point) = &entity.kind
        && context.d.header.i64("PDMODE", 0) != 0
    {
        return super::world_roles::point_marker(builder, context, entity, point, color, lineweight);
    }
    // A line pattern runs along the curve in WCS. The kinds the legacy painter strokes get it; any other kind with a linetype that
    // the legacy painter would not ignore refuses.
    let patterned = matches!(
        entity.kind,
        EntityKind::Line(_)
            | EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Polyline3d(_)
            | EntityKind::Spline(_)
            | EntityKind::Helix(_)
            | EntityKind::Leader(_)
    );
    if linetype.is_some()
        && !patterned
        && !matches!(
            entity.kind,
            EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::Solid(_) | EntityKind::Trace(_) | EntityKind::Hatch(_) | EntityKind::Wipeout(_)
        )
    {
        return Err(unsupported("spatial line patterns"));
    }
    let geometry = super::entity_world_geometry(context.d, entity, &context.world)?;
    let projected = Ctx { xf: Mat3::IDENTITY, ..context.clone() };
    if let (true, Some(lt)) = (patterned, linetype) {
        let pattern =
            super::world_roles::Pattern { linetype: lt, scale: linetype_scale * context.xf.scale_factor().max(1e-12), min: builder.opts.min_dash };
        let plane = cadcraft_geom::OcsFrame::new(entity.common.extrusion);
        let primitives = geometry.primitives();
        // One pattern over the whole line for a leader (its vertices, then its arrowhead plain) and for a polyline with generation
        // on; one per primitive otherwise.
        let chained = match &entity.kind {
            EntityKind::Leader(leader) => leader.vertices.len().saturating_sub(1),
            EntityKind::LwPolyline(p) if p.plinegen => primitives.len(),
            _ => 0,
        };
        let (chain, rest) = primitives.split_at(chained.min(primitives.len()));
        let mut groups: Vec<Vec<&WorldPrimitive>> = Vec::new();
        if !chain.is_empty() {
            groups.push(chain.iter().collect());
        }
        groups.extend(rest.iter().map(|p| vec![p]));
        for group in groups {
            if !super::world_roles::dashed(builder, &projected, color, lineweight, &pattern, plane, &group) {
                for primitive in group {
                    let one = WorldGeometry::from_primitives(entity.handle, vec![primitive.clone()])?;
                    paint_geometry(builder, &projected, &one, entity.handle, color, lineweight)?;
                }
            }
        }
        return Ok(());
    }
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

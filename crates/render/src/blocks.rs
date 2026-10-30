use super::draw::entity;
use super::props::{resolve, resolve_common, transparency};
use super::text_entity::{mtext, place_text_entity};
use super::{Builder, Color, Ctx, Entity, EntityKind, Ink, Lineweight, Mat3, Vec2};

pub(crate) fn sub_ctx<'a>(ctx: &Ctx<'a>, e: &Entity, m: Mat3) -> Ctx<'a> {
    let layer = if e.common.layer == "0" { ctx.block_layer.clone() } else { Some(e.common.layer.clone()) };
    let color = match e.common.color {
        Color::ByBlock => ctx.block_color,
        Color::ByLayer => ctx.layer_color(layer.as_deref().unwrap_or("0")).unwrap_or(Color::Index(7)),
        c => c,
    };
    let lw = match e.common.lineweight {
        Lineweight::ByBlock => ctx.block_lw,
        Lineweight::ByLayer => ctx.d.layer(layer.as_deref().unwrap_or("0")).map(|l| l.lineweight).unwrap_or(Lineweight::Default),
        x => x,
    };
    let lt = match e.common.linetype.to_ascii_lowercase().as_str() {
        "byblock" => ctx.block_ltype.clone(),
        "bylayer" => ctx.d.layer(layer.as_deref().unwrap_or("0")).map(|l| l.linetype.clone()).unwrap_or_else(|| "Continuous".into()),
        _ => e.common.linetype.clone(),
    };
    Ctx {
        d: ctx.d,
        xf: ctx.xf.then_before(m),
        world: ctx.world.mul(&cadcraft_geom::Mat4::from_mat3(&m)),
        block_color: color,
        block_layer: layer,
        block_lw: lw,
        block_ltype: lt,
        depth: ctx.depth + 1,
        top: ctx.top,
        frozen: ctx.frozen,
        vp_colors: ctx.vp_colors,
        // The reference's own linetype scale (group 48) multiplies that of the objects inside,
        // as LTSCALE does for the drawing (tests/block_ltscale.rs).
        lt_factor: ctx.lt_factor * if e.common.ltscale.is_finite() && e.common.ltscale > 0.0 { e.common.ltscale } else { 1.0 },
        block_pstyle: ctx.block_pstyle,
        // The reference's own, set by the caller (`Builder::transparency` while it draws).
        block_transparency: ctx.block_transparency,
    }
}

pub(crate) fn insert(b: &mut Builder, ctx: &Ctx, e: &Entity, ins: &cadcraft_doc::Insert, rgb: Ink) {
    if ctx.depth >= cadcraft_doc::MAX_BLOCK_DEPTH {
        return;
    }
    if let Some(blk) = ctx.d.block(&ins.block) {
        let cols = ins.cols.clamp(1, 10_000);
        let rows = ins.rows.clamp(1, 10_000);
        'copies: for r in 0..rows {
            for c in 0..cols {
                if !b.expand() {
                    break 'copies;
                }
                let off = Vec2::new(ins.col_spacing * f64::from(c), ins.row_spacing * f64::from(r)).rotate(ins.rotation);
                let m = Mat3::translate(off).then_before(ins.transform(blk.base.xy()));
                let sub = Ctx { block_pstyle: b.named, block_transparency: b.transparency, ..sub_ctx(ctx, e, m) };
                for be in blk.entities.iter() {
                    // Constant/visible attribute definitions inside blocks are not drawn; attribs are.
                    if matches!(be.kind, EntityKind::AttDef(_)) {
                        continue;
                    }
                    if !b.expand() {
                        break 'copies;
                    }
                    entity(b, &sub, be);
                }
            }
        }
    }
    if b.opts.text {
        // ATTMODE (ATTDISP): 0 shows no attributes, 1 the visible ones, 2 all of them.
        let attmode = ctx.d.header.i64("ATTMODE", 1);
        // The reference's transparency; each attribute draws with its own.
        let reference = b.transparency;
        for a in &ins.attribs {
            if attmode == 0 || (a.invisible && attmode != 2) {
                continue;
            }
            // A multiline attribute is drawn as its multiline text.
            let multiline = a.multiline_text();
            let sh = if multiline.is_none() { Some(place_text_entity(ctx.d, &a.text, &a.text.value).0) } else { None };
            let (ink, lw, shown, t) = attribute_appearance(b, ctx, e, ins, a, rgb, reference);
            if shown {
                b.transparency = t;
                let lw = if b.opts.lineweights { lw } else { 0.0 };
                match (&multiline, &sh) {
                    (Some(m), _) => mtext(b, ctx, m, ink, lw),
                    (None, Some(sh)) => b.shaped(ctx, ink, lw, sh),
                    (None, None) => {}
                }
            }
        }
        b.transparency = reference;
    }
}

fn attribute_appearance(
    b: &Builder,
    ctx: &Ctx,
    e: &Entity,
    ins: &cadcraft_doc::Insert,
    attribute: &cadcraft_doc::Attrib,
    rgb: Ink,
    reference: u8,
) -> (Ink, f32, bool, u8) {
    let definition = ctx.d.block(&ins.block).and_then(|block| block.entities.iter().find(|entity| attdef_tag(entity, &attribute.tag)));
    match (definition, attribute.props.is_empty()) {
        (None, true) => (rgb, resolve(ctx, e, b.plotting).1, true, reference),
        (definition, _) => {
            let common = attribute.props.over(&definition.map(|entity| entity.common.clone()).unwrap_or_else(by_block));
            let (ink, lw, _, _, shown, layer_t) = resolve_common(&sub_ctx(ctx, e, Mat3::IDENTITY), &common, b.plotting);
            let t = if b.opts.transparency { transparency(common.transparency, layer_t, reference) } else { 0 };
            (ink, lw, shown, t)
        }
    }
}

/// One cell of a canonical block reference: the children of `block` placed by `matrix` (block to WCS, relative to the parent of
/// `ctx`), with the properties of the referencing entity `e` for ByBlock.
fn canonical_block_cell(
    b: &mut Builder,
    ctx: &Ctx,
    e: &Entity,
    block: &cadcraft_doc::Block,
    matrix: &cadcraft_geom::Mat4,
) -> cadcraft_doc::Result<()> {
    use cadcraft_doc::{DocError, GeometryRevision};
    let xy = Mat3 { a: matrix.m[0][0], b: matrix.m[1][0], c: matrix.m[0][1], d: matrix.m[1][1], e: matrix.m[0][3], f: matrix.m[1][3] };
    let sub = Ctx { world: ctx.world.mul(matrix), block_pstyle: b.named, block_transparency: b.transparency, ..sub_ctx(ctx, e, xy) };
    for child in block.entities.iter() {
        if matches!(child.kind, EntityKind::AttDef(_) | EntityKind::Unknown(_)) || !resolve(&sub, child, b.plotting).4 {
            continue;
        }
        if child.geometry_revision != GeometryRevision::CanonicalV2 {
            return Err(DocError::Invalid("A canonical block cannot use a legacy child geometry payload.".into()));
        }
        if !b.expand() {
            return Err(DocError::Invalid("The canonical block expansion limit was reached.".into()));
        }
        super::draw::try_entity(b, &sub, child)?;
    }
    Ok(())
}

/// The attributes of a canonical block reference `ins` of `e`, each placed by `placement` (the parent for instances already in
/// WCS; the parent after the block matrix for attributes kept in block coordinates), with the appearance their definition gives.
fn canonical_attributes(
    b: &mut Builder,
    ctx: &Ctx,
    e: &Entity,
    ins: &cadcraft_doc::Insert,
    placement: &cadcraft_geom::Mat4,
    rgb: Ink,
) -> cadcraft_doc::Result<()> {
    use cadcraft_doc::{DocError, GeometryRevision, WorldGeometry, attrib_primitives};
    if !b.opts.text {
        return Ok(());
    }
    let mode = ctx.d.header.i64("ATTMODE", 1);
    let reference = b.transparency;
    for attribute in &ins.attribs {
        if mode == 0 || (attribute.invisible && mode != 2) {
            continue;
        }
        let (ink, lineweight, shown, transparency) = attribute_appearance(b, ctx, e, ins, attribute, rgb, reference);
        if !shown {
            continue;
        }
        if attribute.geometry_revision != GeometryRevision::CanonicalV2
            || attribute.multiline.as_ref().is_some_and(|text| text.geometry_revision != GeometryRevision::CanonicalV2)
        {
            return Err(DocError::Invalid("A canonical INSERT cannot display a legacy attribute payload.".into()));
        }
        if attribute.plane.thickness != 0.0 {
            return Err(DocError::Invalid("Canonical attribute extrusion display is not available yet.".into()));
        }
        let owner = attribute.handle.unwrap_or(e.handle);
        let geometry = WorldGeometry::from_primitives(owner, attrib_primitives(attribute, placement)?)?;
        let projected = Ctx { xf: Mat3::IDENTITY, ..ctx.clone() };
        b.transparency = transparency;
        let lineweight = if b.opts.lineweights { lineweight } else { 0.0 };
        super::world_paint::paint_geometry(b, &projected, &geometry, owner, ink, lineweight)?;
    }
    b.transparency = reference;
    Ok(())
}

pub(crate) fn canonical_insert(b: &mut Builder, ctx: &Ctx, e: &Entity, ins: &cadcraft_doc::Insert, rgb: Ink) -> cadcraft_doc::Result<()> {
    use cadcraft_doc::DocError;
    if ctx.depth > cadcraft_doc::MAX_BLOCK_DEPTH {
        return Err(DocError::Invalid("The canonical block references nest too deeply.".into()));
    }
    if let Some(block) = ctx.d.block(&ins.block) {
        if ins.cols == 0 || ins.rows == 0 || u64::from(ins.cols) * u64::from(ins.rows) > super::MAX_BLOCK_EXPANSION as u64 {
            return Err(DocError::Invalid("The canonical INSERT array exceeds the display limits.".into()));
        }
        for row in 0..ins.rows {
            for column in 0..ins.cols {
                if !b.expand() {
                    return Err(DocError::Invalid("The canonical block expansion limit was reached.".into()));
                }
                let matrix = ins.world_transform(e.common.extrusion, block.base, column, row)?;
                canonical_block_cell(b, ctx, e, block, &matrix)?;
            }
        }
    }
    canonical_attributes(b, ctx, e, ins, &ctx.world, rgb)
}

/// A canonical multileader with block content: its lines, doglegs, arrowheads and text, then the block placed by the multileader's
/// block matrix and its attributes (kept in block coordinates), all with the multileader's own properties for ByBlock, like a
/// block reference.
pub(crate) fn canonical_mleader_block(
    b: &mut Builder,
    ctx: &Ctx,
    e: &Entity,
    leader: &cadcraft_doc::MLeader,
    ins: &cadcraft_doc::Insert,
    rgb: Ink,
    lineweight: f32,
) -> cadcraft_doc::Result<()> {
    use cadcraft_doc::{DocError, WorldGeometry};
    if ctx.depth > cadcraft_doc::MAX_BLOCK_DEPTH {
        return Err(DocError::Invalid("The canonical block references nest too deeply.".into()));
    }
    leader.require_canonical()?;
    let block = ctx.d.block(&ins.block);
    let base = block.map_or(cadcraft_geom::Vec3::ZERO, |block| block.base);
    // The lines and text without the attributes, which are painted with the appearance of their definitions below.
    let mut bare = leader.clone();
    if let Some(content) = &mut bare.block {
        content.attribs.clear();
    }
    let arrow = super::mleader_arrow_layout(ctx.d, leader);
    let world = cadcraft_doc::mleader_primitives(&bare, base, &arrow, &ctx.world)?;
    let projected = Ctx { xf: Mat3::IDENTITY, ..ctx.clone() };
    super::world_paint::paint_geometry(b, &projected, &WorldGeometry::from_primitives(e.handle, world.primitives)?, e.handle, rgb, lineweight)?;
    let matrix = leader.block_world_transform(base)?;
    if let (Some(block), Some(matrix)) = (block, &matrix) {
        if !b.expand() {
            return Err(DocError::Invalid("The canonical block expansion limit was reached.".into()));
        }
        canonical_block_cell(b, ctx, e, block, matrix)?;
    }
    canonical_attributes(b, ctx, e, ins, &matrix.map_or(ctx.world, |m| ctx.world.mul(&m)), rgb)
}

/// Properties that follow the block reference's (an attribute without a definition).
fn by_block() -> cadcraft_doc::Common {
    cadcraft_doc::Common {
        color: Color::ByBlock,
        linetype: "ByBlock".into(),
        lineweight: Lineweight::ByBlock,
        transparency: cadcraft_doc::Transparency::ByBlock,
        ..cadcraft_doc::Common::default()
    }
}

/// The attribute definition with tag `tag` (any case).
fn attdef_tag(e: &Entity, tag: &str) -> bool {
    matches!(&e.kind, EntityKind::AttDef(d) if d.tag.eq_ignore_ascii_case(tag))
}

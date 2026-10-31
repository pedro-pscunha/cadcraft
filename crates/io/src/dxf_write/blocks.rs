use super::{
    Block, Color, Common, Dimension, Drawing, Entity, EntityKind, EntityStore, Handle, HashMap, Line, MText, Solid, Table, Vec2, Vec3, dxf_ext,
};

/// What happens to the anonymous `*D` blocks read from a file (#495): a dimension that still
/// names its block regenerates its geometry under that name, keeping the block's entity
/// handles. Unreferenced blocks stay ordinary blocks: saving is not PURGE.
pub(super) struct DimBlockPlan {
    /// Dimension (model or paper space) → the read block whose name and handles it reuses.
    pub(super) reuse: HashMap<Handle, std::sync::Arc<Block>>,
    /// Upper-case names of dimension blocks (`*D`, or named ones only dimensions use) not written
    /// as ordinary blocks.
    pub(super) skip: std::collections::HashSet<String>,
}

fn is_dim_block(name: &str) -> bool {
    name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("*D"))
}

pub(super) fn dim_block_plan(d: &Drawing) -> DimBlockPlan {
    let spaces: Vec<&EntityStore> = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).collect();
    // Block names other objects use: inserts and multileader contents anywhere, and dimensions
    // inside block definitions (their blocks are written as read).
    let mut used = std::collections::HashSet::new();
    let note = |e: &Entity, dims: bool, used: &mut std::collections::HashSet<String>| match &e.kind {
        EntityKind::Insert(i) => {
            used.insert(i.block.to_ascii_uppercase());
        }
        EntityKind::MLeader(m) => used.extend(m.block.as_ref().map(|b| b.block.to_ascii_uppercase())),
        EntityKind::Dimension(dm) if dims || e.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2 => {
            used.extend(dm.block.as_ref().map(|n| n.to_ascii_uppercase()));
        }
        // An entity of a type CADCraft doesn't model is written back as read, so the block its
        // group 2 names (an ARC_DIMENSION's `*D` block) stays as read too.
        EntityKind::Unknown(u) => {
            used.extend(u.tags.iter().filter(|t| t.code == 2 && is_dim_block(t.value.trim())).map(|t| t.value.trim().to_ascii_uppercase()))
        }
        _ => {}
    };
    for e in spaces.iter().flat_map(|s| s.iter()) {
        note(e, false, &mut used);
    }
    for e in d.blocks.values().flat_map(|b| b.entities.iter()) {
        note(e, true, &mut used);
    }
    // A dimension's graphics block can have any name (another program wrote `DIMBLOCK1-…`): a
    // block that only dimensions use is theirs, like a `*D` block.
    let dim_refs: std::collections::HashSet<String> = spaces
        .iter()
        .flat_map(|s| s.iter())
        .filter_map(|e| if let EntityKind::Dimension(dm) = &e.kind { dm.block.as_ref().map(|n| n.to_ascii_uppercase()) } else { None })
        .collect();
    let skip = d.blocks.keys().map(|n| n.to_ascii_uppercase()).filter(|n| dim_refs.contains(n) && !used.contains(n)).collect();
    let mut plan = DimBlockPlan { reuse: HashMap::new(), skip };
    let mut claimed = std::collections::HashSet::new();
    for e in spaces.iter().flat_map(|s| s.iter()) {
        let EntityKind::Dimension(dm) = &e.kind else { continue };
        let Some(b) = dm.block.as_deref().and_then(|n| d.block(n)) else { continue };
        let key = b.name.to_ascii_uppercase();
        if plan.skip.contains(&key) && claimed.insert(key) {
            plan.reuse.insert(e.handle, b.clone());
        }
    }
    plan
}

/// Give regenerated dimension geometry the handles of the block it replaces, in order; the rest
/// keep handle 0 and get fresh ones.
pub(super) fn reuse_handles(ents: &mut [Entity], old: &Block, taken: &mut std::collections::HashSet<Handle>) {
    let mut handles = old.entities.iter().map(|e| e.handle).filter(|h| h.0 != 0);
    for e in ents.iter_mut() {
        match handles.by_ref().find(|h| taken.insert(*h)) {
            Some(h) => e.handle = h,
            None => break,
        }
    }
}

/// Anonymous dimension blocks (`*D1`…) with the rendered geometry, as consumers expect.
///
/// AutoCAD stores the picture in the dimension's OCS. The DIMENSION's extrusion places the whole block,
/// so the block entities use the local axes, not a second copy of that extrusion.
pub(super) fn dim_block_entities(d: &Drawing, dm: &Dimension, layer: &str, normal: Vec3, revision: cadcraft_doc::GeometryRevision) -> Vec<Entity> {
    let Ok(local) = dm.localized_for_revision(normal, revision) else { return Vec::new() };
    let drawn = &local;
    // The dimension supplies the plane elevation. Its graphics block stays at local Z 0.
    let z = 0.0;
    let geometry_revision = Default::default();
    // The dimension as drawn: its overrides, DIMSCALE (0 = the drawing's) and text font.
    let style = d.dim_style(&dm.style).cloned().unwrap_or_default().with_overrides(&dm.overrides);
    let g = cadcraft_render::dimension_in(d, drawn);
    let ocs = |p: Vec2| p.to3(z);
    let mut out = Vec::new();
    // Each part in its style colour (DIMCLRD, DIMCLRE, DIMCLRT), ByBlock when the style says so,
    // as AutoCAD writes them: readers that draw the block drew every part ByBlock, i.e. in the
    // dimension's own colour (#1052).
    let in_color = |color: Color| Common { layer: layer.into(), color, ..Common::default() };
    let (dim_c, ext_c, text_c) = (in_color(style.dim_line_color), in_color(style.ext_line_color), in_color(style.text_color));
    let line_entities = |i: usize, l: &[Vec2]| -> Vec<Entity> {
        let c = match g.line_roles.get(i) {
            Some(cadcraft_render::LineRole::Ext) => &ext_c,
            _ => &dim_c,
        };
        l.windows(2)
            .filter_map(|seg| match seg {
                [a, b] => Some(Entity {
                    geometry_revision: Default::default(),
                    handle: Handle(0),
                    common: Common { extrusion: Vec3::Z, ..c.clone() },
                    kind: EntityKind::Line(Line { a: ocs(*a), b: ocs(*b) }),
                }),
                _ => None,
            })
            .collect()
    };
    // The text's marks (arc length symbol, DIMINSPECT frame) go after the text background.
    for (i, l) in g.lines.iter().enumerate().filter(|(i, _)| !g.text_marks.contains(i)) {
        out.extend(line_entities(i, l));
    }
    for t in &g.fills {
        if let [a, b, cc] = t.as_slice() {
            out.push(Entity {
                geometry_revision,
                handle: Handle(0),
                common: dim_c.clone(),
                kind: EntityKind::Solid(Solid { corners: [ocs(*a), ocs(*b), ocs(*cc), ocs(*cc)] }),
            });
        }
    }
    // DIMTFILL 2: the text background in DIMTFILLCLR, under the text and its marks (SOLID
    // corners go 1, 2, 4, 3).
    if style.text_fill == 2
        && let [p0, p1, p2, p3] = g.text_fill.as_slice()
    {
        out.push(Entity {
            geometry_revision,
            handle: Handle(0),
            common: in_color(style.text_fill_color),
            kind: EntityKind::Solid(Solid { corners: [ocs(*p0), ocs(*p1), ocs(*p3), ocs(*p2)] }),
        });
    }
    for (i, l) in g.lines.iter().enumerate().filter(|(i, _)| g.text_marks.contains(i)) {
        out.extend(line_entities(i, l));
    }
    if !g.value.is_empty() {
        // The text shares the block's local axes and elevation.
        out.push(Entity {
            geometry_revision,
            handle: Handle(0),
            common: text_c,
            kind: EntityKind::MText(MText {
                insert: ocs(g.text_pos),
                height: g.text_height,
                width: 0.0,
                attach: 5,
                rotation: g.text_angle,
                style: style.text_style.clone(),
                contents: g.mtext_contents(),
                line_spacing: 1.0,
                line_spacing_exact: false,
                columns: None,
                direction: None,
                drawing_direction: None,
            }),
        });
    }
    out
}

/// Rows and columns written for a table (bounded like the renderer).
pub(super) fn table_size(t: &Table) -> (usize, usize) {
    (t.row_heights.len().min(10_000), t.col_widths.len().min(10_000))
}

/// The key of what a table's graphics block is drawn from: the table, its layer, its table style and the text styles that style names.
/// A table the file drew a block for keeps that block while the key is the one it had when it was read.
pub(crate) fn table_graphics_key(d: &Drawing, t: &Table, layer: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let style = d.table_styles.iter().find(|s| s.name.eq_ignore_ascii_case(&t.style));
    let texts: Vec<_> = style.iter().flat_map(|s| s.cell_style_names().into_iter().map(|n| d.text_style(&s.cell_style(&n).text_style))).collect();
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&(t, layer, style, texts)).unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

/// The anonymous `*T` block of a table: cell fills, borders and texts as its cell styles draw
/// them, for readers that only draw the block.
///
/// The block is relative to its own origin: the ACAD_TABLE reference adds the insertion point.
pub(crate) fn table_block_entities(d: &Drawing, t: &Table, layer: &str) -> Vec<Entity> {
    let at_origin = Table { insert: Vec3::ZERO, ..t.clone() };
    let g = cadcraft_render::table_geometry(d, &at_origin);
    let c = Common { layer: layer.into(), color: Color::ByBlock, ..Common::default() };
    let mut out = Vec::new();
    for f in &g.fills {
        let [a, b, cc, dd] = f.corners.map(|p| p.to3(0.0));
        // SOLID corners go 1, 2, 4, 3.
        out.push(Entity {
            geometry_revision: Default::default(),
            handle: Handle(0),
            common: Common { color: f.color, ..c.clone() },
            kind: EntityKind::Solid(Solid { corners: [a, b, dd, cc] }),
        });
    }
    for l in &g.lines {
        out.push(Entity {
            geometry_revision: Default::default(),
            handle: Handle(0),
            common: Common { color: l.color, lineweight: l.lineweight, ..c.clone() },
            kind: EntityKind::Line(Line { a: l.a.to3(0.0), b: l.b.to3(0.0) }),
        });
    }
    let mut texts = Vec::new();
    for cell in &g.texts {
        if !cell.text.is_empty() {
            let style = if d.text_style(&cell.style).is_some() { cell.style.clone() } else { "Standard".into() };
            texts.push(Entity {
                geometry_revision: Default::default(),
                handle: Handle(0),
                common: Common { color: cell.color, ..c.clone() },
                kind: EntityKind::MText(MText {
                    insert: cell.at.to3(0.0),
                    height: cell.height,
                    width: 0.0,
                    attach: cell.attach,
                    rotation: 0.0,
                    style,
                    contents: cell.text.clone(),
                    line_spacing: 1.0,
                    line_spacing_exact: false,
                    columns: None,
                    direction: None,
                    drawing_direction: None,
                }),
            });
        }
    }
    out.extend(texts);
    // The blocks the cells hold, as references in the cell.
    out.extend(g.blocks.into_iter().map(|b| Entity {
        geometry_revision: Default::default(),
        handle: Handle(0),
        common: c.clone(),
        kind: EntityKind::Insert(b.insert),
    }));
    out
}

/// Our own geometry for an arrowhead block: unit size, tip at the origin, pointing along +X.
pub(super) fn arrow_block_entities(kind: cadcraft_render::Arrowhead) -> Vec<Entity> {
    let g = cadcraft_render::arrowhead(kind, Vec2::ZERO, Vec2::X, 1.0);
    let c = Common { layer: "0".into(), color: Color::ByBlock, ..Common::default() };
    let mut out = Vec::new();
    for l in &g.lines {
        for seg in l.windows(2) {
            if let [a, b] = seg {
                out.push(Entity {
                    geometry_revision: Default::default(),
                    handle: Handle(0),
                    common: c.clone(),
                    kind: EntityKind::Line(Line { a: a.to3(0.0), b: b.to3(0.0) }),
                });
            }
        }
    }
    for t in g.tris.chunks(3) {
        if let [a, b, cc] = t {
            out.push(Entity {
                geometry_revision: Default::default(),
                handle: Handle(0),
                common: c.clone(),
                kind: EntityKind::Solid(Solid { corners: [a.to3(0.0), b.to3(0.0), cc.to3(0.0), cc.to3(0.0)] }),
            });
        }
    }
    out
}

/// Whether the entities of the block `name` are the arrowhead block CADCraft writes for that
/// standard arrow name ([`arrow_block_entities`]): the same lines and solids with the same
/// properties (layer 0, ByBlock). Any other content (AutoCAD's own `_Dot`, a user's
/// redefinition, an older CADCraft's geometry) is the file's.
pub(crate) fn is_generated_arrow(name: &str, entities: &[&Entity]) -> bool {
    let Some((_, kind)) = dxf_ext::arrow_block_name(name) else { return false };
    let ours = arrow_block_entities(kind);
    ours.len() == entities.len() && ours.iter().zip(entities).all(|(a, b)| a.kind == b.kind && a.common == b.common)
}

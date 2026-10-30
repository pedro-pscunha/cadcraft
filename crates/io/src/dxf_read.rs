//! DXF → Drawing.

use std::collections::{BTreeMap, HashMap, HashSet};

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{
    Arc, AssocSnap, Attrib, AttribModes, AttribPlane, AttribProps, Block, Circle, Common, DashElement, DimAssoc, DimKind, DimStyle, Dimension,
    Drawing, Ellipse, Entity, EntityKind, Face3d, Gradient, Group, HAlign, HVal, Handle, Hatch, HatchLoop, Helix, HelixConstrain, Insert, Layer,
    Layout, Leader, Line, Linetype, Lineweight, LwPolyline, MAX_FRAME_CHARS, MLeaderStyle, MText, MultilineAttrib, NamedView, PageSetup, Point,
    Polyline3d, RawTag, RayLine, Solid, Table, TableCell, Text, TextStyle, Tolerance, Transparency, Ucs, Unknown, VAlign, Viewport, VportConfig,
    VportTile, Wipeout,
};
use cadcraft_dxf::{Section, Tag, records, sections};
use cadcraft_geom::{PolyVertex, Spline, Vec2, Vec3};

use crate::{IoError, Result};

mod common;
mod curve_canonical;
pub(crate) mod dimassoc;
mod document;
mod entities;
mod fill_canonical;
mod hatch;
mod mleader_canonical;
mod objects;
mod tables;
mod text;
mod text_canonical;

pub use curve_canonical::{canonical_curve_from_groups, canonical_polyline_from_groups};
pub use fill_canonical::canonical_fill_from_groups;
pub use mleader_canonical::canonical_mleader_from_groups;
pub use text_canonical::{canonical_attrib_from_groups, canonical_layout_from_groups, canonical_text_from_groups};

use document::{
    apply_sortents, count_skipped, outside_sections, parse_entities, place_blocks, place_entities, read_blocks, read_objects, setup_layouts,
    shift_dimension_blocks,
};
#[cfg(test)]
use entities::acad_table;
use entities::{entity, plot_style_names};
use objects::Objects;
pub(crate) use tables::{MAX_VPORT_TILES, kept_vport};
use tables::{header, tables};

pub(crate) mod mtext_dxf;

pub(crate) fn page_setup_from_groups(tags: &[Tag]) -> PageSetup {
    objects::page_setup(&common::T(tags))
}

/// Handle maps and references gathered while reading.
#[derive(Default)]
struct Rx {
    /// The DXF was made from a DWG file by the DWG reader.
    from_dwg: bool,
    /// STYLE record handle (upper case) → text style name.
    styles: HashMap<String, String>,
    /// BLOCK_RECORD handle (upper case) → block name.
    brs: HashMap<String, String>,
    /// Block record → its attached extension dictionary (not any dictionary claiming this owner).
    block_xdicts: HashMap<Handle, Handle>,
    /// LTYPE record handle (upper case) → linetype name (DIMLTYPE, DIMLTEX1, DIMLTEX2).
    ltypes: HashMap<String, String>,
    /// Upper-case block name → (insertion units, explodable, scale uniformly) from its BLOCK_RECORD.
    block_props: HashMap<String, (u8, bool, bool)>,
    /// Upper-case names of anonymous `*T` blocks drawn by ACAD_TABLE entities.
    table_blocks: std::collections::HashSet<String>,
    /// Upper-case block names used by INSERTs.
    inserted: std::collections::HashSet<String>,
    /// Upper-case names of blocks that xdata outside dimensions names (1005), such as a leader's
    /// DIMLDRBLK override: an arrowhead block named there is the file's, kept as read.
    xdata_blocks: HashSet<String>,
    /// Tables whose style is known only by TABLESTYLE handle: (table, style handle).
    table_style_fix: Vec<(Handle, String)>,
    /// Multileaders' style handles and block attribute values, resolved with the objects.
    mleader_fix: Vec<(Handle, crate::dxf_mleader::Pending)>,
    /// Upper-case xref block name → the external drawing's path (BLOCK group 1).
    xref_paths: HashMap<String, String>,
    /// Plot style name placeholder handle (upper case) → plot style name (`ACAD_PLOTSTYLENAME`).
    pstyles: HashMap<String, String>,
    /// Upper-case block name → associative array parameters from its BLOCK_RECORD.
    block_arrays: HashMap<String, cadcraft_doc::ArrayParams>,
    /// Upper-case block name → the values of its BLOCK_RECORD preview (310) groups.
    block_previews: HashMap<String, Vec<String>>,
    /// LAYER record handle (upper case) → layer name (VIEWPORT group 331 frozen layers).
    layers: HashMap<String, String>,
    /// Upper-case `*D` block name → the shift AutoCAD draws it with: its dimension's group 12.
    dim_block_shifts: Vec<(String, cadcraft_geom::Vec2)>,
    /// Upper-case names of the layout blocks that hold a paper-space viewport CADCraft wrote (see `dxf_write::paper_view`).
    marked_sheets: HashSet<String>,
}

/// Remember the layouts that have viewports but no paper-space viewport (id 1) in the file: AutoCAD files have some (31 viewports
/// in 9 such layouts of one Citycon drawing), and a save does not add one to them. A layout CADCraft made has its sheet written
/// (`dxf_write::paper_view`); the one it wrote is read implicit, so a layout with that mark is not in this set.
fn note_layouts_without_sheet(d: &mut Drawing, rx: &Rx, paper_block_for: &HashMap<String, String>) {
    let marked: HashSet<&String> = paper_block_for.iter().filter(|(block, _)| rx.marked_sheets.contains(*block)).map(|(_, layout)| layout).collect();
    let names: Vec<String> = d
        .layouts
        .iter()
        .filter(|l| !marked.contains(&l.name))
        .filter(|l| {
            let mut viewports = l.entities.iter().filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { Some(v.id) } else { None }).peekable();
            viewports.peek().is_some() && viewports.all(|id| id != 1)
        })
        .map(|l| l.name.to_ascii_uppercase())
        .collect();
    if !names.is_empty() {
        std::sync::Arc::make_mut(&mut d.source_symbols).layouts_without_sheet.extend(names);
    }
}

/// Upper-case names of the blocks whose BLOCK_RECORD xdata handles (1005) point at, outside
/// dimensions and dimension styles (CADCraft models their DSTYLE overrides and arrowheads).
fn xdata_blocks(secs: &[Section], brs: &HashMap<String, String>) -> HashSet<String> {
    secs.iter()
        .flat_map(|s| records(&s.tags))
        .filter(|(k, _)| k != "DIMENSION" && k != "DIMSTYLE")
        .flat_map(|(_, tg)| {
            tg.into_iter()
                .filter(|t| t.code == 1005)
                .filter_map(|t| Handle::parse_hex(t.str().trim()))
                .filter_map(|h| brs.get(&h.hex()).map(|n| n.to_ascii_uppercase()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Read a DXF file into a drawing.
pub fn read(bytes: &[u8]) -> Result<Drawing> {
    read_budgeted(bytes, 4_000_000, false)
}

/// Read the DXF that the DWG reader made from a DWG file. Groups that the DWG reader writes as fixed values, and a DWG does not
/// hold, are not the file's data and are left unread.
pub fn read_from_dwg(bytes: &[u8]) -> Result<Drawing> {
    read_budgeted(bytes, 4_000_000, true)
}

/// A block definition read from BLOCKS: name, base point, description, entities (owner, paper
/// flag, entity) and block flags.
type BlockDef = (String, Vec3, String, Vec<(Option<String>, bool, Entity)>, i64);

/// A layout read from OBJECTS: name, tab order, paper block handle and page setup.
type LayoutDef = (String, u32, String, PageSetup);

/// Import DXF sections while sharing one materialized-table-cell budget across blocks and entity sections.
fn read_budgeted(bytes: &[u8], mut table_cells_left: usize, from_dwg: bool) -> Result<Drawing> {
    let tags = cadcraft_dxf::parse(bytes).map_err(|e| IoError::Format(e.to_string()))?;
    let mut secs = sections(&tags);
    if secs.is_empty() {
        return Err(IoError::Format("no sections found".into()));
    }
    let mut d = Drawing::new_imperial();
    d.layouts.clear();
    // A file without $INSUNITS declares no units (#1209); only new drawings take the template's.
    d.header.set_i64("INSUNITS", 0);
    let mut rx = Rx { pstyles: plot_style_names(&secs), from_dwg, ..Rx::default() };
    d.plot_style_names = rx.pstyles.values().cloned().collect();
    d.plot_style_names.sort();
    let mut blocks: Vec<BlockDef> = Vec::new();
    let mut layout_objs: Vec<LayoutDef> = Vec::new();
    let mut entities = Vec::new();
    let mut objs = Objects::default();
    // Every handle another program's file already uses is taken before anything gets a new one:
    // its $HANDSEED can be lower than its handles, and entities made while reading (the faces of
    // a polyface mesh) would otherwise get handles that records further on still have. A file
    // CADCraft wrote names its own next handle, above which its rebuilt records are numbered
    // (#495), so it is left to that.
    let ours = secs.iter().any(|s| s.name == "OBJECTS" && s.tags.iter().any(|t| t.code == 3 && t.str() == crate::dxf_ext::HANDSEED_KEY));
    reserve_instance_handles(&mut secs, &mut d, ours);
    let mut skipped = BTreeMap::new();
    count_skipped(&mut skipped, "(no section)", outside_sections(&tags));
    for s in &secs {
        match s.name.as_str() {
            "HEADER" => header(&s.tags, &mut d, &rx),
            "TABLES" => tables(&s.tags, &mut d, &mut rx),
            "BLOCKS" => blocks.extend(read_blocks(&s.tags, &mut d, &mut rx, &mut table_cells_left, &mut skipped)),
            // A damaged file may split its entities over several ENTITIES sections (#865).
            "ENTITIES" => {
                let mut n = 0;
                entities.extend(parse_entities(&records(&s.tags), &mut d, &mut rx, &mut n, &mut table_cells_left));
                count_skipped(&mut skipped, "ENTITIES", n);
            }
            "OBJECTS" => read_objects(&s.tags, &mut objs, &mut layout_objs),
            _ => {}
        }
    }
    // A file CADCraft wrote names its own next handle (#495): its table records and objects
    // are numbered above it and rebuilt on save, so following $HANDSEED would drift every save.
    let own_seed = objs.xrecord_text(crate::dxf_ext::HANDSEED_KEY).and_then(|t| Handle::parse_hex(t.trim()));
    rx.xdata_blocks = xdata_blocks(&secs, &rx.brs);
    let (paper_block_for, first_layout) = setup_layouts(&mut d, &mut layout_objs, &rx);
    place_blocks(&mut d, blocks, &mut rx, &paper_block_for, &first_layout);
    shift_dimension_blocks(&mut d, &rx)?;
    place_entities(&mut d, entities, &rx, &paper_block_for, &first_layout);
    note_layouts_without_sheet(&mut d, &rx, &paper_block_for);
    let sort_tables = apply_sortents(&mut d, &objs, &rx, &paper_block_for, &first_layout);
    objs.apply(&mut d, &rx)?;
    crate::dxf_image::resolve(&secs, &mut d);
    crate::dxf_mline::resolve(&secs, &mut d);
    // Keep handles unique against the header's seed.
    let seed = d.header.str("HANDSEED", "");
    if let Some(h) = own_seed.or_else(|| Handle::parse_hex(&seed)) {
        d.bump_handseed(Handle(h.0.saturating_sub(1)));
    }
    if d.layer("0").is_none() {
        d.layers.insert(0, Layer::default());
    }
    crate::dxf_raw::read(&secs, &mut d);
    std::sync::Arc::make_mut(&mut d.foreign).sort_tables = sort_tables;
    // Read kept objects and symbol roles before repairing missing identities: both still use
    // their input namespace in foreign references until writing remaps them.
    let kept: Vec<_> = d.foreign.objects.iter().map(|o| o.handle).chain(d.foreign.targets.keys().copied()).collect();
    for h in kept {
        d.bump_handseed(h);
    }
    d.allocate_missing_attribute_handles();
    d.remember_raw_dimassocs();
    d.skipped_records = skipped;
    keep_block_previews(&mut d, rx.block_previews);
    Ok(d)
}

/// Give each block the preview its file had, keyed to the contents it has now: the last step of reading, after
/// nothing changes the blocks any more.
fn keep_block_previews(d: &mut Drawing, mut previews: HashMap<String, Vec<String>>) {
    for (name, block) in &mut d.blocks {
        if let Some(groups) = previews.remove(&name.to_ascii_uppercase()) {
            let block = std::sync::Arc::make_mut(block);
            block.preview = Some(cadcraft_doc::BlockPreview { groups, key: block.content_key() });
        }
    }
}

/// Reserve persistent child identities even in CADCraft files with a private handseed. A
/// colliding input identity is omitted before raw-data reading, so its dictionary cannot be
/// attached to whichever different record already owns the handle.
fn reserve_instance_handles(secs: &mut [Section], d: &mut Drawing, ours: bool) {
    let mut used = HashSet::new();
    for section in secs.iter().filter(|s| s.name != "HEADER") {
        let mut kind = String::new();
        let mut seen = false;
        for t in &section.tags {
            if t.code == 0 {
                kind = t.str();
                seen = false;
            } else if !kind.is_empty() && !matches!(kind.as_str(), "ATTRIB" | "VERTEX" | "SEQEND") && matches!(t.code, 5 | 105) && !seen {
                if let Some(h) = Handle::parse_hex(&t.str()) {
                    used.insert(h);
                    if !ours {
                        d.bump_handseed(h);
                    }
                }
                seen = true;
            }
        }
    }
    for section in secs.iter_mut().filter(|s| s.name != "HEADER") {
        let mut instance = false;
        let mut seen = false;
        for t in &mut section.tags {
            if t.code == 0 {
                instance = matches!(t.str().as_str(), "ATTRIB" | "VERTEX" | "SEQEND");
                seen = false;
            } else if instance && t.code == 5 {
                let h = Handle::parse_hex(&t.str()).filter(|h| h.0 != 0 && h.0 != u64::MAX);
                if !seen && h.is_some_and(|h| used.insert(h)) {
                    if let Some(h) = h {
                        d.bump_handseed(h);
                    }
                } else {
                    *t = Tag::s(999, "Rejected duplicate or invalid child identity");
                }
                seen = true;
            }
        }
    }
}

/// An entity from the groups of a DXF record of type `kind` (without its `0` group), as the
/// reader makes it. AutoLISP's `entmake` and `entmod` are built on it.
pub fn entity_from_groups(kind: &str, tags: &[Tag]) -> Option<(Common, EntityKind)> {
    entity(&kind.to_ascii_uppercase(), tags)
}

#[cfg(test)]
mod table_budget_tests {
    use super::*;
    /// Build synthetic table tags with a requested row and column count for allocation-budget checks.
    fn table_tags(rows: i64, cols: i64) -> Vec<Tag> {
        vec![Tag::s(100, "AcDbTable"), Tag::i(91, rows), Tag::i(92, cols), Tag::i(171, 1), Tag::s(1, "cell")]
    }
    /// A table that cannot fit allocates no cell grid, while a smaller table spends only its cell count.
    #[test]
    fn exhausted_budget_rejects_before_materializing_cells() {
        let mut left = 3;
        let table = acad_table(&table_tags(2, 2), &mut left);
        assert!(table.cells.is_empty() && table.row_heights.is_empty() && table.col_widths.is_empty());
        assert_eq!(left, 3);
        let table = acad_table(&table_tags(1, 2), &mut left);
        assert_eq!(table.cells[0].len(), 2);
        assert_eq!(left, 1);
    }
    /// Tables in blocks and repeated entity sections consume one document-wide cell allowance.
    #[test]
    fn blocks_and_entity_sections_share_one_budget() {
        let table = "0\nACAD_TABLE\n100\nAcDbTable\n91\n1\n92\n2\n171\n1\n1\ncell\n";
        let dxf = format!(
            "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nTEST\n10\n0\n20\n0\n{table}0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{table}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{table}0\nENDSEC\n0\nEOF\n"
        );
        let d = read_budgeted(dxf.as_bytes(), 2, false).unwrap();
        let block = d.block("TEST").unwrap();
        let count = |e: &Entity| match &e.kind {
            EntityKind::Table(t) => t.cells.iter().map(Vec::len).sum::<usize>(),
            _ => 0,
        };
        assert_eq!(block.entities.iter().map(|e| count(e)).sum::<usize>(), 2);
        assert_eq!(d.model.iter().map(|e| count(e)).sum::<usize>(), 0);
    }
}

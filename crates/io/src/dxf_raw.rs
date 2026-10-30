//! What a DXF file holds that CADCraft does not model, kept as raw groups so that saving writes
//! it back (#528): entities and objects of other types, other applications' xdata, extension
//! dictionaries, persistent reactors, colour book colours and other unmodelled entity properties.
//!
//! Reading ([`read`]) keeps the data with the file's handles in [`Drawing::foreign`], plus the
//! role of every handle it refers to that CADCraft writes itself (a table record, a block, an attribute, the
//! named object dictionary…). Writing ([`finish`]) runs over the finished tag list: it places the
//! entities of types CADCraft doesn't model among the written ones (the entity writer leaves them
//! out), puts the data back on the records that still exist and points each reference at what now
//! has the role it named. Kept objects get new handles above the written ones (so they never
//! collide with the handles the writer hands out). A reference to something that is gone is
//! dropped (from reactor lists and dictionaries) or written as the null handle `0`; objects whose
//! owner is gone go with it.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use cadcraft_doc::foreign::{Foreign, ForeignExtra, ForeignKey, ForeignObject, MAX_FOREIGN_DEPTH, dimassoc_reference, is_dictionary, is_pointer};
use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, RawTag, Unknown};
use cadcraft_dxf::{Section, Tag};

/// Named object dictionary entries CADCraft writes itself from its model (plus every key that
/// starts with `CADCRAFT`): they are read into the model, not kept raw. A change that starts
/// modelling another entry adds it here. (An entry the writer writes is also never written
/// twice: the kept one is left out.)
const MODELLED_KEYS: &[&str] = &[
    "ACAD_GROUP",
    "ACAD_LAYOUT",
    "ACAD_MLEADERSTYLE",
    "ACAD_MLINESTYLE",
    "ACAD_PLOTSTYLENAME",
    "ACAD_SCALELIST",
    "ACAD_TABLESTYLE",
    "ACAD_IMAGE_DICT",
];

/// Extension dictionary entries always written from their models. Dimension associativity is
/// modelled only when the writer can reproduce every point reference (see [`modelled_xdict_entry`]).
const MODELLED_XDICT_KEYS: &[&str] = &["ACAD_SORTENTS"];

/// Objects CADCraft writes from its model, by name, in dictionaries of the named object
/// dictionary. Their extension dictionary and xdata that the writer doesn't write are kept like
/// those of a table record (`Foreign::records`, under the role `NOD/<dictionary>`).
const MODELLED_OBJECTS: &[&str] = &["LAYOUT", "MLEADERSTYLE", "TABLESTYLE", "SCALE"];

fn modelled_xdict_entry(key: &str, target: &str, modelled_assoc: &HashSet<String>) -> bool {
    let key = up(key);
    MODELLED_XDICT_KEYS.contains(&key.as_str()) || (key == "ACAD_DIMASSOC" && modelled_assoc.contains(&norm(target)))
}

/// Applications whose xdata CADCraft reads and writes itself. `ACAD` holds dimension overrides
/// (DSTYLE) on dimensions and dimension styles, `AcadAnnotative` the annotative flag of text and
/// dimension styles while it is on (see [`annotative_on`]), `AcCmTransparency` and `AcAecLayerStandard` a layer's transparency and
/// description (see [`layer_xdata_modelled`]); elsewhere they are kept.
const OWN_APPS: &[&str] = &["CADCRAFT"];

/// Whether `AcadAnnotative` xdata (`chunk`) has the annotative flag (its second `1070`) on, as the
/// model reads it from a style and writes it back. With the flag off the data is kept as read.
fn annotative_on(chunk: &[Tag]) -> bool {
    chunk.iter().filter(|t| t.code == 1070).nth(1).is_some_and(|t| t.i64() != 0)
}

/// Whether a layer's xdata of `app` (upper case) holds what the model writes back: a transparency
/// above 0 (or no value: acadrust drops a layer whose `AcCmTransparency` has none) for
/// `AcCmTransparency`, a description (the second string, after the layer standard's name) for
/// `AcAecLayerStandard`. Any other content is kept as read.
fn layer_xdata_modelled(app: &str, chunk: &[Tag]) -> bool {
    match app {
        "ACCMTRANSPARENCY" => {
            chunk.iter().find(|t| t.code == 1071).is_none_or(|t| crate::dxf_ext::transparency_from_dxf(t.i64()).is_some_and(|p| p > 0))
        }
        "ACAECLAYERSTANDARD" => {
            let mut strings = chunk.iter().filter(|t| t.code == 1000);
            strings.next().is_some_and(|name| name.str().is_empty()) && strings.next().is_some_and(|description| !description.str().is_empty())
        }
        _ => false,
    }
}

/// Groups of a layout's `AcDbLayout` subclass the writer has no model for: its flags (paper space
/// linetype scaling, limits checking), limits, insertion base, extents, elevation, UCS and
/// orthographic view type, and the pointers after its block record: the viewport last active (331) and
/// the named and base UCS (345, 346).
const LAYOUT_FRAME_CODES: &[i32] =
    &[70, 10, 20, 11, 21, 12, 22, 32, 14, 24, 34, 15, 25, 35, 146, 13, 23, 33, 16, 26, 36, 17, 27, 37, 76, 331, 345, 346];
/// Groups of a layout's `AcDbPlotSettings` subclass `PageSetup` has no field for: the plot origin,
/// shade plot mode, resolution and DPI, the paper image origin, and the shade plot handle (333, a
/// pointer to a visual style).
const PLOT_EXTRA_CODES: &[i32] = &[46, 47, 76, 77, 78, 148, 149, 333];
/// Plot layout flag bits (group 70) that `PageSetup` models: show plot styles, centred, standard
/// scale, lineweights. The others (plot with plot styles, draw viewports first, hide objects,
/// scale lineweights...) stay as read.
const PLOT_FLAGS_MODELLED: i64 = 2 | 4 | 16 | 128;

#[derive(Clone, Copy, PartialEq)]
enum LayoutPart {
    Plot,
    Frame,
    Other,
}

fn layout_part(subclass: &str) -> LayoutPart {
    match subclass.trim() {
        "AcDbPlotSettings" => LayoutPart::Plot,
        "AcDbLayout" => LayoutPart::Frame,
        _ => LayoutPart::Other,
    }
}

/// A layout's groups to keep: its whole plot settings (the Model layout has no page setup in the
/// model) and the [`LAYOUT_FRAME_CODES`] of its layout groups, each part after its `100` marker.
fn layout_groups(tags: &[Tag]) -> Vec<RawTag> {
    let mut out = Vec::new();
    let mut part = LayoutPart::Other;
    for t in tags.iter().take_while(|t| t.code < 1000) {
        if t.code == 100 {
            part = layout_part(&t.str());
            if part != LayoutPart::Other {
                out.push(raw(t));
            }
        } else if part == LayoutPart::Plot || (part == LayoutPart::Frame && LAYOUT_FRAME_CODES.contains(&t.code)) {
            out.push(raw(t));
        }
    }
    out
}

fn first_group(from: &[RawTag], code: i32) -> Option<&RawTag> {
    from.iter().find(|t| t.code == code)
}

/// Keep independent source plot fields until a page edit changes their meaning.
fn plot_replay_codes(plot: &[RawTag], page: Option<&cadcraft_doc::PageSetup>) -> HashSet<i32> {
    let mut codes: HashSet<i32> = PLOT_EXTRA_CODES.iter().copied().chain([1]).collect();
    let Some(page) = page else { return codes };
    let tags: Vec<Tag> = plot.iter().map(tag).collect();
    let source = crate::dxf_read::page_setup_from_groups(&tags);
    if page.paper_mm == source.paper_mm {
        codes.insert(72);
    }
    if (page.width_mm, page.height_mm, page.landscape) == (source.width_mm, source.height_mm, source.landscape) {
        codes.extend([44, 45, 73]);
    }
    if page.plot_area == source.plot_area {
        codes.extend([6, 74]);
    }
    if (page.scale, page.scale_to_fit) == (source.scale, source.scale_to_fit) {
        codes.extend([75, 142, 143, 147]);
    }
    codes
}

/// A written layout with the groups it has constants for taken from the file's (`kept`, see
/// [`layout_groups`]), and the Model layout's plot settings replaced by the file's.
fn patch_layout(b: &[Tag], kept: &[RawTag], page: Option<&cadcraft_doc::PageSetup>, saved: impl Fn(&str) -> Option<String>) -> Vec<Tag> {
    let at = kept.iter().position(|t| t.code == 100 && layout_part(&t.value) == LayoutPart::Frame).unwrap_or(kept.len());
    let (plot, frame) = (kept.get(1..at).unwrap_or(&[]), kept.get(at + 1..).unwrap_or(&[]));
    let model = b
        .iter()
        .skip_while(|t| !(t.code == 100 && layout_part(&t.str()) == LayoutPart::Frame))
        .find(|t| t.code == 1)
        .is_some_and(|t| t.str().trim().eq_ignore_ascii_case("Model"));
    let replay = plot_replay_codes(plot, page);
    let modelled_flags = if replay.contains(&75) { PLOT_FLAGS_MODELLED & !16 } else { PLOT_FLAGS_MODELLED };
    // A kept plot group, a pointer remapped to the saved object (null when it is gone).
    let plot_tag = |k: &RawTag| if is_pointer(k.code) { Tag::s(k.code, saved(&k.value).unwrap_or_else(|| "0".into())) } else { tag(k) };
    let mut out = Vec::with_capacity(b.len() + plot.len());
    let mut part = LayoutPart::Other;
    let mut plot_codes: HashSet<i32> = HashSet::new();
    for t in b {
        if t.code == 100 {
            let next = layout_part(&t.str());
            if part == LayoutPart::Plot && next == LayoutPart::Frame && !model {
                out.extend([76, 77, 78, 333].into_iter().filter(|c| !plot_codes.contains(c)).filter_map(|c| first_group(plot, c)).map(plot_tag));
            }
            part = next;
            out.push(t.clone());
            if part == LayoutPart::Plot && model && !plot.is_empty() {
                out.extend(plot.iter().map(plot_tag));
            }
            continue;
        }
        // The pointers of the layout the writer has none for follow its block record, those still
        // naming an object of the saved file.
        if part == LayoutPart::Frame && t.code == 330 {
            out.push(t.clone());
            out.extend(
                [331, 345, 346]
                    .into_iter()
                    .filter_map(|c| first_group(frame, c))
                    .filter_map(|k| Some(Tag::s(k.code, saved(&k.value).filter(|h| h != "0")?))),
            );
            continue;
        }
        let replaced = match part {
            LayoutPart::Plot if model && !plot.is_empty() => continue,
            LayoutPart::Plot => {
                plot_codes.insert(t.code);
                match t.code {
                    70 => first_group(plot, 70)
                        .map(|k| Tag::i(70, (t.i64() & modelled_flags) | (k.value.trim().parse::<i64>().unwrap_or(0) & !modelled_flags))),
                    // The paper's canonical name: the model keeps it with spaces, the writer writes underscores.
                    4 => first_group(plot, 4).filter(|k| k.value.replace('_', " ") == t.str().replace('_', " ")).map(tag),
                    code => first_group(plot, code).filter(|_| replay.contains(&code)).map(plot_tag),
                }
            }
            LayoutPart::Frame => first_group(frame, t.code).filter(|_| LAYOUT_FRAME_CODES.contains(&t.code)).map(tag),
            LayoutPart::Other => None,
        };
        out.push(replaced.unwrap_or_else(|| t.clone()));
    }
    out
}

/// Most objects kept from one file.
const MAX_OBJECTS: usize = 1_000_000;
/// Most xdata groups kept per record (AutoCAD allows 16 KB of xdata per object).
const MAX_XDATA_GROUPS: usize = 16_384;

/// Record types that structure a file: never written as an entity of another type.
const STRUCTURAL: &[&str] = &["", "SECTION", "ENDSEC", "EOF", "TABLE", "ENDTAB", "BLOCK", "ENDBLK", "SEQEND", "VERTEX", "ATTRIB", "CLASS"];

fn up(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// A handle as text, normalised (`002a` → `2A`); other text upper-cased.
fn norm(s: &str) -> String {
    Handle::parse_hex(s).map_or_else(|| up(s), Handle::hex)
}

fn raw(t: &Tag) -> RawTag {
    RawTag { code: t.code, value: t.str() }
}

/// A kept group with the value type its code has (so binary output encodes it right); text that
/// doesn't parse as that type stays text.
pub(crate) fn tag(r: &RawTag) -> Tag {
    use cadcraft_dxf::{Kind, Value, kind_of};
    let v = r.value.trim();
    let value = match kind_of(r.code) {
        Kind::Real => v.parse::<f64>().ok().filter(|x| x.is_finite()).map(Value::Real),
        Kind::Int16 | Kind::Int32 | Kind::Int64 => v.parse::<i64>().ok().map(Value::Int),
        Kind::Bool => v.parse::<i64>().ok().map(|x| Value::Bool(x != 0)),
        Kind::Hex => Some(Value::Hex(v.to_string())),
        Kind::Str => None,
    };
    Tag::new(r.code, value.unwrap_or_else(|| Value::Str(r.value.clone())))
}

/// A handle value we believe: hexadecimal and not null. DXF handles are up to 16 hex digits and
/// AutoCAD writes some above 2^48 (e.g. 0xDAD1…), so the whole 64-bit range is believed.
fn handle(s: &str) -> Option<Handle> {
    Handle::parse_hex(s).filter(|h| h.0 != 0)
}

/// The records of a run of tags: (type, index of the record's `0` group, groups after it).
fn split(tags: &[Tag]) -> Vec<(String, usize, &[Tag])> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize)> = None;
    for (i, t) in tags.iter().enumerate() {
        if t.code == 0 {
            if let Some((k, s)) = open.take() {
                out.push((k, s, tags.get(s + 1..i).unwrap_or(&[])));
            }
            open = Some((t.str(), i));
        }
    }
    if let Some((k, s)) = open {
        out.push((k, s, tags.get(s + 1..).unwrap_or(&[])));
    }
    out
}

type Recs<'a> = Vec<(String, usize, &'a [Tag])>;

/// A record's handle, owner, reactors and extension dictionary (groups before its xdata).
#[derive(Default)]
struct Head {
    handle: Option<String>,
    owner: Option<String>,
    reactors: Vec<String>,
    xdict: Option<String>,
    has_reactors: bool,
}

fn head(tags: &[Tag]) -> Head {
    let mut h = Head { owner: cadcraft_doc::foreign::object_owner(tags.iter().map(|t| (t.code, t.str()))).map(Handle::hex), ..Head::default() };
    let mut group: Option<String> = None;
    for t in tags.iter().take_while(|t| t.code < 1000) {
        match (t.code, group.as_deref()) {
            (102, _) => {
                let v = t.str();
                group = v.trim().strip_prefix('{').map(str::to_string);
                h.has_reactors |= group.as_deref() == Some("ACAD_REACTORS");
            }
            (_, Some("ACAD_REACTORS")) => h.reactors.push(norm(&t.str())),
            (_, Some("ACAD_XDICTIONARY")) if h.xdict.is_none() => {
                h.xdict = Some(norm(&t.str()));
            }
            (_, Some(_)) => {}
            (5 | 105, None) if h.handle.is_none() => h.handle = Some(norm(&t.str())),
            _ => {}
        }
    }
    h
}

fn handle_of_record(tags: &[Tag]) -> Option<Handle> {
    head(tags).handle.and_then(|h| handle(&h))
}

fn val(tags: &[Tag], code: i32) -> Option<String> {
    tags.iter().take_while(|t| t.code < 1000).find(|t| t.code == code).map(Tag::str)
}

/// A dictionary's entries: (key, upper-case handle).
fn entries(tags: &[Tag]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut key: Option<String> = None;
    for t in tags.iter().take_while(|t| t.code < 1000) {
        match t.code {
            3 => key = Some(t.str()),
            350 | 360 => {
                if let Some(k) = key.take() {
                    out.push((k, norm(&t.str())));
                }
            }
            _ => {}
        }
    }
    out
}

fn modelled_key(key: &str) -> bool {
    let k = up(key);
    k.starts_with("CADCRAFT") || MODELLED_KEYS.contains(&k.as_str())
}

/// The roles of a file's handles (see [`ForeignKey`]): table records and tables by type and
/// name, blocks by name, the named object dictionary, its entries, and their entries.
fn identities(tables: &Recs, blocks: &Recs, objects: &Recs) -> HashMap<String, ForeignKey> {
    let mut out: HashMap<String, ForeignKey> = HashMap::new();
    let mut put = |h: Option<String>, k: ForeignKey| {
        if let Some(h) = h {
            out.entry(h).or_insert(k);
        }
    };
    // The tiles of one VPORT configuration share its name: each tile the reader keeps is a role of
    // its own, numbered in file order (the first by the name alone).
    let mut tiles: HashMap<String, usize> = HashMap::new();
    for (kind, _, t) in tables {
        let mut name = up(&val(t, 2).unwrap_or_default());
        if kind == "VPORT" {
            let n = tiles.entry(name.clone()).or_default();
            if !crate::dxf_read::kept_vport(t) || *n >= crate::dxf_read::MAX_VPORT_TILES {
                continue;
            }
            if *n > 0 {
                name = format!("{name}|{n}");
            }
            *n += 1;
        }
        if kind == "TABLE" || !name.is_empty() {
            put(head(t).handle, (if kind == "TABLE" { "TABLE".into() } else { kind.clone() }, name));
        }
    }
    let mut block = String::new();
    for (kind, _, t) in blocks {
        match kind.as_str() {
            "BLOCK" => {
                block = up(&val(t, 2).unwrap_or_default());
                put(head(t).handle, ("BLOCK".into(), block.clone()));
            }
            "ENDBLK" => put(head(t).handle, ("ENDBLK".into(), block.clone())),
            _ => {}
        }
    }
    let Some((_, _, root)) = objects.first().filter(|(k, _, _)| is_dictionary(k)) else { return out };
    put(head(root).handle, ("NOD".into(), String::new()));
    let by_handle: HashMap<String, (&str, &[Tag])> = objects.iter().filter_map(|(k, _, t)| Some((head(t).handle?, (k.as_str(), *t)))).rev().collect();
    for (key, h) in entries(root) {
        put(Some(h.clone()), ("NOD".into(), up(&key)));
        if let Some((k, t)) = by_handle.get(&h)
            && is_dictionary(k)
        {
            for (name, eh) in entries(t) {
                // A scale's dictionary key is arbitrary (the writer numbers them): its name is group 300.
                let scale = by_handle.get(&eh).filter(|_| up(&key) == "ACAD_SCALELIST").and_then(|(_, t)| val(t, 300));
                put(Some(eh), (format!("NOD/{}", up(&key)), up(&scale.unwrap_or(name))));
            }
        }
    }
    out
}

/// Remember the file's handle of each role CADCraft writes itself. A role two records share (a
/// repeated name) keeps the lowest.
fn keep_source_handles(d: &mut Drawing, ids: &HashMap<String, ForeignKey>) {
    let mut found: Vec<(Handle, &ForeignKey)> = ids.iter().filter_map(|(h, k)| Some((handle(h)?, k))).collect();
    found.sort();
    let source = Arc::make_mut(&mut d.source_symbols);
    for (h, k) in found {
        source.handles.entry(k.clone()).or_insert(h);
    }
}

/// Remember the order of the persistent reactors of the entities that have more than one.
fn keep_reactor_order(d: &mut Drawing, blocks: &Recs, entities: &Recs) {
    let source = Arc::make_mut(&mut d.source_symbols);
    for (_, _, t) in blocks.iter().chain(entities).filter(|(kind, _, _)| !matches!(kind.as_str(), "BLOCK" | "ENDBLK")) {
        let h = head(t);
        let order: Vec<Handle> = h.reactors.iter().filter_map(|r| handle(r)).collect();
        if let (Some(own), true) = (h.handle.as_deref().and_then(handle), order.len() >= 2) {
            source.reactor_order.entry(own).or_insert(order);
        }
    }
}

/// Put the persistent reactors of each entity in the order the file listed them, the ones it did not have after them.
pub(crate) fn order_reactors(tags: &mut [Tag], d: &Drawing) {
    if d.source_symbols.reactor_order.is_empty() {
        return;
    }
    let mut section = String::new();
    let mut opening = false;
    let mut i = 0;
    while i < tags.len() {
        let t = &tags[i];
        if t.code == 0 {
            match t.str().as_str() {
                "SECTION" => opening = true,
                "ENDSEC" => section.clear(),
                _ => {}
            }
        } else if opening && t.code == 2 {
            section = t.str();
            opening = false;
        }
        let entity_section = matches!(section.as_str(), "ENTITIES" | "BLOCKS");
        if entity_section
            && t.code == 5
            && tags.get(i + 1).is_some_and(|n| n.code == 102 && n.str().trim() == "{ACAD_REACTORS")
            && let Some(source) = handle(&t.str()).and_then(|h| d.source_symbols.reactor_order.get(&h))
        {
            let from = i + 2;
            let len = tags[from..].iter().take_while(|g| g.code == 330).count();
            tags[from..from + len].sort_by_key(|g| handle(&g.str()).and_then(|h| source.iter().position(|s| *s == h)).unwrap_or(usize::MAX));
        }
        i += 1;
    }
}

/// Remember the CLASS records of the file (their groups 1, 2, 3, 90, 280 and 281), in order.
fn keep_classes(d: &mut Drawing, classes: &Recs) {
    let source = Arc::make_mut(&mut d.source_symbols);
    for (_, _, t) in classes.iter().filter(|(kind, _, _)| kind == "CLASS").take(MAX_OBJECTS) {
        let groups: Vec<RawTag> = t.iter().filter(|g| matches!(g.code, 1 | 2 | 3 | 90 | 280 | 281)).map(raw).collect();
        if groups.iter().any(|g| g.code == 1) {
            source.classes.push(groups);
        }
    }
}

/// Remember the order of the `{BLKREFS` list of each BLOCK_RECORD.
fn keep_block_references(d: &mut Drawing, tables: &Recs) {
    let source = Arc::make_mut(&mut d.source_symbols);
    for (_, _, t) in tables.iter().filter(|(kind, _, _)| kind == "BLOCK_RECORD") {
        let (Some(name), Some(open)) = (val(t, 2), t.iter().position(|g| g.code == 102 && g.str().trim() == "{BLKREFS")) else { continue };
        let refs: Vec<Handle> = t[open + 1..].iter().take_while(|g| g.code == 331).filter_map(|g| handle(&g.str())).collect();
        if !refs.is_empty() {
            source.block_refs.entry(up(&name)).or_insert(refs);
        }
    }
}

/// The order AutoCAD writes (and reads) the groups of a subclass in, by subclass marker. A coordinate's x, y and z groups share the
/// place of the first. Codes not listed follow the listed ones in the order they were written.
/// - `AcDbDimension`: the version (280) first, then the block, the points, the type and presentation groups, the measurement, the text,
///   the rotation, the normal and the style (DXF Reference).
/// - `AcDbEntity`: measured on the entities of the AutoCAD 2010 and 2018 saves of Citycon, DPInc and Planeta.
/// - `AcDbMText`, `AcDbText`, `AcDbWipeout`, `AcDbRasterImage`, `AcDbTextStyleTableRecord`, `AcDbLeader`, `AcDbPlotSettings`: the same.
const SUBCLASS_ORDER: [(&str, &[i32]); 9] = [
    ("AcDbDimension", &[280, 2, 10, 11, 12, 70, 1, 71, 72, 41, 42, 73, 74, 75, 53, 51, 54, 210, 3]),
    ("AcDbEntity", &[67, 410, 8, 6, 347, 62, 420, 430, 440, 48, 60, 92, 160, 310, 370, 380, 390, 284, 348]),
    ("AcDbMText", &[10, 40, 41, 46, 71, 72, 3, 1, 7, 210, 11, 73, 44, 90, 63, 421, 45, 441]),
    ("AcDbText", &[39, 10, 40, 1, 50, 41, 51, 7, 71, 72, 11, 210, 73]),
    ("AcDbWipeout", &[90, 10, 11, 12, 13, 340, 70, 280, 281, 282, 283, 290, 360, 71, 91, 14]),
    ("AcDbRasterImage", &[90, 10, 11, 12, 13, 340, 70, 280, 281, 282, 283, 290, 360, 71, 91, 14]),
    ("AcDbTextStyleTableRecord", &[2, 70, 40, 41, 50, 71, 42, 3, 4]),
    ("AcDbLeader", &[3, 71, 72, 73, 74, 75, 40, 41, 76, 10, 77, 340, 210, 211, 212, 213]),
    ("AcDbPlotSettings", &[1, 2, 4, 6, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 140, 141, 142, 143, 70, 72, 73, 74, 7, 75, 147, 76, 77, 78, 148, 149]),
];

/// The place of a group in a subclass `order`.
fn group_rank(order: &[i32], code: i32) -> usize {
    let base = match code {
        20 | 30 => 10,
        21 | 31 => 11,
        22 | 32 => 12,
        23 => 13,
        24 => 14,
        220 | 230 => 210,
        221 | 231 => 211,
        222 | 232 => 212,
        223 | 233 => 213,
        c => c,
    };
    order.iter().position(|c| *c == base).unwrap_or(order.len())
}

/// Put the groups of the subclasses in `SUBCLASS_ORDER`, of every record, in AutoCAD's order. The writer and the replays of the
/// source's groups (the versioned presentation groups, the geometry, the properties) each place theirs, and AutoCAD refuses a file
/// whose version group (280) does not follow a dimension's subclass marker ("unexpected DXF group code: 280"). A multiline text's
/// embedded object (after its 101) and the xdata are not subclass groups.
pub(crate) fn order_subclass_groups(tags: &mut [Tag]) {
    let mut at = 0;
    while at < tags.len() {
        if tags[at].code != 100 {
            at += 1;
            continue;
        }
        let marker = tags[at].str();
        let start = at + 1;
        let stop = tags[start..].iter().position(|t| matches!(t.code, 0 | 100 | 101) || t.code >= 1000).map_or(tags.len(), |n| start + n);
        if let Some((_, order)) = SUBCLASS_ORDER.iter().find(|(m, _)| *m == marker.trim()) {
            tags[start..stop].sort_by_key(|t| group_rank(order, t.code));
        }
        at = stop;
    }
}

/// Give the written tables, symbol records, blocks and dictionary entries the handles the file
/// the drawing came from gave them, wherever a group names one: the written handle of a role the
/// file had becomes the file's, unless another record of the written file already has it. A role
/// the file did not have keeps its new handle.
pub(crate) fn restore_source_handles(tags: &mut [Tag], d: &Drawing) {
    if d.source_symbols.handles.is_empty() {
        return;
    }
    let recs = written_records(tags);
    let body = |r: &Rec| tags.get(r.start + 1..r.end).unwrap_or(&[]);
    let by_sec = |name: &str| -> Recs {
        recs.iter().filter(|r| r.sec == name && r.kind != "SECTION" && r.kind != "ENDSEC").map(|r| (r.kind.clone(), r.start, body(r))).collect()
    };
    let ids = identities(&by_sec("TABLES"), &by_sec("BLOCKS"), &by_sec("OBJECTS"));
    let used: HashSet<String> = recs.iter().filter_map(|r| head(body(r)).handle).collect();
    let renamed: HashMap<String, String> = ids
        .iter()
        .filter_map(|(written, role)| {
            let old = d.source_symbols.handles.get(role)?.hex();
            (norm(written) != old && !used.contains(&old)).then(|| (norm(written), old))
        })
        .collect();
    if renamed.is_empty() {
        return;
    }
    let mut section = String::new();
    let mut opening = false;
    for t in tags.iter_mut() {
        if t.code == 0 {
            match t.str().as_str() {
                "SECTION" => opening = true,
                "ENDSEC" => section.clear(),
                _ => {}
            }
            continue;
        }
        if opening && t.code == 2 {
            section = t.str();
            opening = false;
            continue;
        }
        // The header's group 5 is the next free handle, not a record's.
        if !(is_pointer(t.code) || matches!(t.code, 5 | 105) && section != "HEADER") {
            continue;
        }
        if let Some(new) = renamed.get(&norm(&t.str())) {
            *t = Tag::s(t.code, new.clone());
        }
    }
}

/// Other applications' xdata of a record of type `kind`: whole applications, each with valid
/// groups (codes 1000 and up, balanced `1002` braces), up to [`MAX_XDATA_GROUPS`].
fn foreign_xdata(kind: &str, tags: &[Tag]) -> Vec<RawTag> {
    let Some(start) = tags.iter().position(|t| t.code == 1001) else { return Vec::new() };
    let rest = tags.get(start..).unwrap_or(&[]);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(first) = rest.get(i) {
        let len = rest.get(i + 1..).map_or(0, |r| r.iter().position(|t| t.code == 1001).unwrap_or(r.len()));
        let chunk = rest.get(i..i + 1 + len).unwrap_or(&[]);
        i += 1 + len;
        let app = up(&first.str());
        // An MTEXT's column record is read into the model and written from it. With other ACAD data
        // beside it, the rest stays foreign and the record's place is kept as an empty one (BEGIN, END).
        let columns = (kind == "MTEXT" && app == "ACAD")
            .then(|| crate::dxf_read::mtext_dxf::read_xdata_columns(chunk).and(crate::dxf_read::mtext_dxf::column_record(chunk)))
            .flatten();
        let own = OWN_APPS.contains(&app.as_str())
            || (app == "ACAD" && matches!(kind, "DIMENSION" | "DIMSTYLE"))
            || (kind == "BLOCK_RECORD" && crate::dxf_ext::design_center_units(chunk).is_some())
            || (app == "ACADANNOTATIVE" && matches!(kind, "STYLE" | "DIMSTYLE") && annotative_on(chunk))
            || (kind == "LAYER" && layer_xdata_modelled(&app, chunk))
            || columns.is_some_and(|(b, e)| b == 1 && e + 1 == chunk.len());
        if app.is_empty() || own || out.len() + chunk.len() > MAX_XDATA_GROUPS {
            continue;
        }
        let mut depth = 0i32;
        let mut ok = true;
        for t in chunk.iter().skip(1) {
            match (t.code, t.str().trim()) {
                (c, _) if c < 1000 => ok = false,
                (1002, "{") => depth += 1,
                (1002, "}") => {
                    depth -= 1;
                    ok &= depth >= 0;
                }
                _ => {}
            }
        }
        if ok && depth == 0 {
            match columns {
                Some((b, e)) => out.extend(chunk.iter().enumerate().filter(|(i, _)| *i <= b || *i >= e).map(|(_, t)| raw(t))),
                None => out.extend(chunk.iter().map(raw)),
            }
        }
    }
    out
}

/// The groups of an entity's AcDbEntity subclass (after `100 AcDbEntity`, up to the next `100`).
fn entity_groups(tags: &[Tag]) -> &[Tag] {
    let Some(start) = tags.iter().position(|t| t.code == 100 && t.str().trim() == "AcDbEntity") else { return &[] };
    let rest = tags.get(start + 1..).unwrap_or(&[]);
    let end = rest.iter().position(|t| t.code == 100 || t.code >= 1000).unwrap_or(rest.len());
    rest.get(..end).unwrap_or(&[])
}

/// What an entity record (`entity`) or a table record carries beyond the model; reactors and
/// the extension dictionary only when kept (`kept`; an entity's reactors may also name entities).
fn extra(kind: &str, tags: &[Tag], entity: bool, unknown: bool, kept: &HashSet<String>) -> ForeignExtra {
    let h = head(tags);
    let mut x = ForeignExtra {
        xdict: h.xdict.filter(|x| kept.contains(x)).and_then(|x| handle(&x)),
        reactors: h.reactors.iter().filter(|r| kept.contains(*r)).filter_map(|r| handle(r)).collect(),
        xdata: foreign_xdata(kind, tags),
        ..ForeignExtra::default()
    };
    if kind == "LAYOUT" && !entity {
        x.layout = layout_groups(tags);
    }
    // An unknown entity's own groups are written as they are; of table records, layers have a
    // colour book colour and a material.
    if !unknown && (entity || matches!(kind, "LAYER" | "VPORT" | "VIEW" | "UCS" | "STYLE")) {
        let props =
            if entity { entity_groups(tags) } else { tags.get(..tags.iter().position(|t| t.code >= 1000).unwrap_or(tags.len())).unwrap_or(&[]) };
        let book = props.iter().find(|t| t.code == 430).map(Tag::str);
        let rgb = props.iter().find(|t| t.code == 420).map(Tag::str);
        if let (Some(rgb), Some(index)) = (rgb.as_ref(), props.iter().find(|t| t.code == 62)) {
            x.color_index = Some((rgb.clone(), index.i64()));
        }
        if let (Some(book), Some(rgb)) = (book, rgb)
            && !book.trim().is_empty()
        {
            x.color_book = Some((rgb, book));
        }
        x.props = props.iter().filter(|t| matches!(t.code, 347 | 348 | 284)).map(raw).collect();
        x.subclass = subclass_groups(kind, tags);
    }
    if matches!(kind, "BLOCK" | "ENDBLK") {
        x.props.extend(tags.iter().find(|t| t.code == 8).map(raw));
        x.props.extend(entity_groups(tags).iter().filter(|t| matches!(t.code, 6 | 48 | 62 | 370 | 440)).map(raw));
    }
    if symbol_flag_mask(kind) != 0 {
        x.subclass.extend(tags.iter().find(|t| t.code == 70).map(raw));
    }
    // A TrueType style AutoCAD makes has an empty font file (3) and the typeface in its ACAD xdata.
    if kind == "STYLE" && typeface(&x.xdata).is_some() {
        x.subclass.extend(tags.iter().find(|t| t.code == 3 && t.str().trim().is_empty()).map(raw));
    }
    // The text of group 1 of a BLOCK that is not an external reference ("Acad:XRef" on the blocks AutoCAD makes of one).
    if kind == "BLOCK" && !tags.iter().any(|t| t.code == 70 && t.i64() & 4 != 0) {
        x.subclass.extend(tags.iter().find(|t| t.code == 1 && !t.str().trim().is_empty()).map(raw));
    }
    if !entity && is_dictionary(kind) {
        x.subclass.extend(tags.iter().find(|t| t.code == 281).map(raw));
    }
    x
}

/// The typeface a style's `ACAD` xdata names (its first string), as a text style of a TrueType font has it.
fn typeface(xdata: &[RawTag]) -> Option<&str> {
    let at = xdata.iter().position(|t| t.code == 1001 && t.value.trim().eq_ignore_ascii_case("ACAD"))?;
    xdata.get(at + 1).filter(|t| t.code == 1000).map(|t| t.value.trim()).filter(|v| !v.is_empty())
}

/// Whether the group `t` the writer wrote for a record of `kind` is replaced by one the file had: the fields of a table record (not
/// its marker, handle and owner), and the empty text (group 1) of a BLOCK that the file gave a text of its own.
fn replaced_by_kept(kind: &str, entity: bool, t: &Tag, x: Option<&ForeignExtra>, replay: &dyn Fn(&RawTag) -> bool) -> bool {
    let Some(x) = x else { return false };
    // The empty font file of such a style stays empty while the style's font is still the typeface of the xdata.
    if kind == "STYLE" && t.code == 3 {
        return x.subclass.iter().any(|p| p.code == 3) && typeface(&x.xdata).is_some_and(|face| face == t.str().trim());
    }
    if kind == "BLOCK" {
        return t.code == 1 && x.subclass.iter().any(|p| p.code == 1);
    }
    !entity
        && matches!(kind, "VPORT" | "VIEW" | "UCS" | "STYLE")
        && x.props.iter().chain(&x.subclass).any(|p| p.code == t.code && replay(p))
        && !matches!(t.code, 0 | 5 | 100 | 330)
}

fn nonconstant_attributes(block: &cadcraft_doc::Block) -> bool {
    block.entities.iter().any(|entity| matches!(&entity.kind, EntityKind::AttDef(attribute) if !attribute.constant))
}

fn symbol_flag_mask(kind: &str) -> i64 {
    match kind {
        "LAYER" | "LTYPE" | "VIEW" | "UCS" | "VPORT" | "APPID" => 16 | 32,
        "STYLE" => 1,
        "BLOCK" => 2,
        // The bit that says the dimension's block is its own (32) is the file's: AutoCAD sets it on some dimensions of a block.
        "DIMENSION" => 32,
        _ => 0,
    }
}

fn source_record_group(t: &Tag, extra: Option<&ForeignExtra>, kind: &str, attributes: Option<bool>) -> Tag {
    let mask = symbol_flag_mask(kind);
    if is_dictionary(kind) && t.code == 281 {
        return extra.and_then(|extra| first_group(&extra.subclass, 281)).map_or_else(|| t.clone(), tag);
    }
    if matches!(kind, "BLOCK" | "ENDBLK") && t.code == 8 {
        return extra.and_then(|extra| first_group(&extra.props, 8)).map_or_else(|| t.clone(), tag);
    }
    let source =
        extra.filter(|_| t.code == 70 && mask != 0).and_then(|extra| first_group(&extra.subclass, 70)).and_then(|tag| tag.value.parse::<i64>().ok());
    if t.code == 70
        && mask == 2
        && let Some((old, current)) = extra.and_then(|extra| extra.block_attributes).zip(attributes)
        && old != current
    {
        return Tag::i(70, (t.i64() & !2) | if current { 2 } else { 0 });
    }
    // A block keeps the anonymous bit (1) the file gave it, even when its name says otherwise (a `*U…` block AutoCAD does not call anonymous).
    let anonymous_bit = if mask == 2 { 1 } else { 0 };
    source.map_or_else(|| t.clone(), |source| Tag::i(70, (t.i64() & !(mask | anonymous_bit)) | (source & (mask | anonymous_bit))))
}

/// The groups of an entity's or a VPORT record's own subclass that the model has no field for, as
/// read: a viewport's view (direction, target, twist, lens, clipping), UCS (345, 346, 79 and its
/// origins and axes) and visual style (348), a leader's annotation link (340), its hook line and
/// offsets. The writer puts them after the groups it writes in that subclass, with handles
/// remapped, apart from the `AcDbEntity` properties. A group the model writes, or keeps elsewhere
/// (frozen layers by 331 and xdata, the sun by link), is not one.
fn subclass_groups(kind: &str, tags: &[Tag]) -> Vec<RawTag> {
    let retain_listed = matches!(kind, "MTEXT" | "DIMENSION" | "HATCH" | "IMAGE" | "WIPEOUT");
    let (marker, codes): (&str, &[i32]) = match kind {
        // The reference column height (46) and the background fill: its setting (90), colour (63, 421), box scale (45) and transparency (441).
        "MTEXT" => ("AcDbMText", &[46, 90, 63, 421, 45, 441]),
        "DIMENSION" => ("AcDbDimension", &[71, 72, 41, 73, 74, 75, 280, 12, 22, 32]),
        // The clip mode of an image (290) and the null pointers a wipeout has where an image has its definition (340, 360).
        "IMAGE" => ("AcDbRasterImage", &[290]),
        "WIPEOUT" => ("AcDbWipeout", &[290, 340, 360]),
        "HATCH" if tags.iter().any(|t| t.code == 450 && t.i64() == 0) => ("AcDbHatch", &[450, 451, 452, 453, 460, 461, 462, 463, 63, 421, 470]),
        "VIEWPORT" => ("AcDbViewport", &[10, 20, 30, 40, 41, 12, 22, 45, 68, 69, 90, 331, 340, 341, 361]),
        // The vertices, the plane and the annotation's size and offsets are the geometry snapshot's.
        "LEADER" => ("AcDbLeader", &[3, 10, 20, 30, 71, 72, 73, 74, 75, 76, 340, 40, 41, 210, 220, 230, 211, 221, 231, 212, 222, 232, 213, 223, 233]),
        // The view the writer fixes in plan (16, 17, 42) is kept too: `record` drops the written ones.
        "VPORT" => ("AcDbViewportTableRecord", &[2, 70, 10, 20, 11, 21, 12, 22, 40, 41, 361]),
        "VIEW" => ("AcDbViewTableRecord", &[2, 70, 10, 20, 40, 41, 50]),
        "STYLE" => ("AcDbTextStyleTableRecord", &[2, 70, 40, 41, 50, 71, 3, 4]),
        "UCS" => ("AcDbUCSTableRecord", &[2, 70, 10, 20, 30, 11, 21, 31, 12, 22, 32, 79]),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut in_subclass = false;
    let mut in_group = false;
    for t in tags.iter().take_while(|t| t.code < 1000) {
        match t.code {
            100 => in_subclass = t.str().trim() == marker,
            101 => in_subclass = false,
            // An application group (reactors, extension dictionary).
            102 => in_group = t.str().trim().starts_with('{'),
            c if in_subclass && !in_group && codes.contains(&c) == retain_listed => out.push(raw(t)),
            _ => {}
        }
    }
    out
}

/// Every entity of the drawing (model space, layouts, blocks).
fn all_entities(d: &Drawing) -> impl Iterator<Item = &Entity> {
    std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter().map(|e| e.as_ref()))
}

/// Give each entity the xdata read for it (`xdata` by entity handle).
fn put_xdata(d: &mut Drawing, mut xdata: HashMap<Handle, Vec<RawTag>>) {
    if xdata.is_empty() {
        return;
    }
    let mut give = |store: &mut cadcraft_doc::EntityStore| {
        let records: Vec<_> = store.iter().filter(|e| xdata.contains_key(&e.handle)).map(|e| e.as_ref().clone()).collect();
        for mut entity in records {
            if let Some(xd) = xdata.remove(&entity.handle) {
                entity.common.xdata = xd;
                store.replace(entity);
            }
        }
    };
    give(&mut d.model);
    for l in &mut d.layouts {
        give(&mut l.entities);
    }
    for b in d.blocks.values_mut() {
        give(&mut Arc::make_mut(b).entities);
    }
}

/// Unnamed styles cannot be selected by name. Keep their records and handle links instead.
fn unnamed_styles(tables: &Recs<'_>) -> Vec<ForeignObject> {
    tables
        .iter()
        .filter_map(|(kind, _, tags)| {
            let unnamed = val(tags, 2).is_none_or(|name| name.trim().is_empty());
            if !matches!(kind.as_str(), "STYLE" | "DIMSTYLE") || !unnamed {
                return None;
            }
            let h = head(tags).handle.and_then(|h| handle(&h))?;
            Some(ForeignObject { handle: h, kind: kind.clone(), tags: tags.iter().map(raw).collect() })
        })
        .take(MAX_OBJECTS)
        .collect()
}

/// AutoCAD writes a dynamic block proxy node's evaluation groups only: a trailing, empty subclass marker (which the
/// DWG bridge adds) makes it refuse the object ("premature end of object").
fn strip_empty_node_marker(kind: &str, tags: &mut Vec<RawTag>) {
    if kind == "ACDB_DYNAMICBLOCKPROXYNODE" && tags.last().is_some_and(|r| r.code == 100 && r.value.trim() == "AcDbDynamicBlockProxyNode") {
        tags.pop();
    }
}

/// Keep what `secs` holds that the reader did not model in `d.foreign`.
pub(crate) fn read(secs: &[Section], d: &mut Drawing) {
    let get = |name: &str| secs.iter().find(|s| s.name == name).map(|s| split(&s.tags)).unwrap_or_default();
    let (classes, tables, blocks, entities, objects) = (get("CLASSES"), get("TABLES"), get("BLOCKS"), get("ENTITIES"), get("OBJECTS"));
    // The drawing's entities by handle: whether each is of a type CADCraft doesn't model.
    let ents = entity_types(d);
    let ids = identities(&tables, &blocks, &objects);
    keep_source_handles(d, &ids);
    keep_block_references(d, &tables);
    keep_classes(d, &classes);
    keep_reactor_order(d, &blocks, &entities);
    let modelled_assoc: HashSet<String> = objects
        .iter()
        .filter(|(kind, _, _)| kind == "DIMASSOC")
        .filter_map(|(_, _, t)| {
            let (_, dimension) = dimassoc_reference(t.iter().map(|t| (t.code, t.str())))?;
            crate::dxf_write::dimassoc_reproducible(d, dimension, t).then(|| head(t).handle).flatten()
        })
        .collect();
    // Extension dictionaries of entities, records and the objects CADCraft writes itself: owner → dictionary.
    let xdict_of = extension_dictionaries(&tables, &blocks, &entities, &objects);
    // Objects by handle: the first of each, and none that has an entity's handle.
    let mut by_handle: HashMap<String, usize> = HashMap::new();
    for (i, (_, _, t)) in objects.iter().enumerate() {
        if let Some(h) = head(t).handle.filter(|h| handle(h).is_some() && !ents.contains_key(h)) {
            by_handle.entry(h).or_insert(i);
        }
    }
    let root = objects.first().filter(|(k, _, _)| is_dictionary(k)).and_then(|(_, _, t)| head(t).handle);
    let root_entries = objects.first().filter(|(k, _, _)| is_dictionary(k)).map(|(_, _, t)| entries(t)).unwrap_or_default();
    let root_key = |h: &str| root_entries.iter().find(|(_, eh)| eh == h).map(|(k, _)| k.clone());
    let viewports: HashSet<String> =
        blocks.iter().chain(&entities).filter(|(k, _, _)| k == "VIEWPORT").filter_map(|(_, _, t)| head(t).handle).collect();
    let table_entities: HashSet<String> = all_entities(d).filter(|e| matches!(e.kind, EntityKind::Table(_))).map(|e| e.handle.hex()).collect();
    let scan = Scan {
        objects: &objects,
        entities: &ents,
        records: &ids,
        by_handle: &by_handle,
        root_entries: &root_entries,
        xdict_of: &xdict_of,
        modelled_assoc: &modelled_assoc,
        viewports: &viewports,
        tables: &table_entities,
    };
    let memo = retained_objects(&scan);
    let mut f = Foreign::default();
    f.table_records = unnamed_styles(&tables);
    let mut kept: HashSet<String> = f.table_records.iter().map(|o| o.handle.hex()).collect();
    for (i, (kind, _, t)) in objects.iter().enumerate() {
        if memo.get(&i) != Some(&true) || f.objects.len() >= MAX_OBJECTS {
            continue;
        }
        let Some(h) = head(t).handle.and_then(|h| handle(&h)) else { continue };
        let mut tags: Vec<RawTag> = t.iter().map(raw).collect();
        if scan.is_xdict(i) {
            cadcraft_doc::foreign::strip_dictionary_entries(&mut tags, |key, target| modelled_xdict_entry(key, target, &modelled_assoc));
        }
        if head(t).owner == root
            && let Some(key) = root_key(&h.hex())
        {
            f.dictionary.push((key, h));
        }
        kept.insert(h.hex());
        strip_empty_node_marker(kind, &mut tags);
        f.objects.push(ForeignObject { handle: h, kind: kind.clone(), tags });
    }
    // Extras of entities, tables, table records, blocks and attributes. An entity keeps the reactors
    // that name kept objects or other entities of the file (a hatch that lists its boundary).
    let reactor_targets: HashSet<String> = kept.iter().cloned().chain(ents.keys().cloned()).collect();
    let mut xdata_of: HashMap<Handle, Vec<RawTag>> = HashMap::new();
    for (kind, _, t) in blocks.iter().chain(&entities) {
        if matches!(kind.as_str(), "BLOCK" | "ENDBLK") {
            continue;
        }
        let Some(h) = head(t).handle else { continue };
        let Some(&unknown) = ents.get(&h) else { continue };
        let mut x = extra(kind, t, true, unknown, &reactor_targets);
        let Some(hh) = handle(&h) else { continue };
        // Child instances own their extras by Handle; top-level entities carry xdata in Common.
        if !matches!(kind.as_str(), "ATTRIB" | "VERTEX" | "SEQEND") {
            let xd = std::mem::take(&mut x.xdata);
            if !xd.is_empty() {
                xdata_of.entry(hh).or_insert(xd);
            }
        }
        if !x.is_empty() {
            f.entities.entry(hh).or_insert(x);
        }
    }
    put_xdata(d, xdata_of);
    let source = blocks.iter().chain(&entities).filter(|(k, ..)| !matches!(k.as_str(), "BLOCK" | "ENDBLK"));
    crate::dxf_geometry::capture_all(d, source.map(|(k, _, t)| (k.as_str(), *t)));
    crate::dxf_mleader_proxy::capture_all(d, blocks.iter().chain(&entities).map(|(kind, _, tags)| (kind.as_str(), *tags)), &mut f);
    let block_attributes: HashMap<_, _> = d.blocks.iter().map(|(name, block)| (up(name), nonconstant_attributes(block))).collect();
    let modelled_objects = objects.iter().filter(|(k, _, _)| MODELLED_OBJECTS.contains(&k.as_str()) || is_dictionary(k));
    for (kind, _, t) in tables.iter().chain(blocks.iter().filter(|(k, _, _)| matches!(k.as_str(), "BLOCK" | "ENDBLK"))).chain(modelled_objects) {
        let Some(key) = head(t).handle.and_then(|h| ids.get(&h).cloned()) else { continue };
        let mut x = extra(kind, t, kind == "BLOCK", false, &kept);
        x.block_attributes = (kind == "BLOCK").then(|| block_attributes.get(&key.1).copied()).flatten();
        if !x.is_empty() {
            f.records.entry(key).or_insert(x);
        }
    }
    link_suns(&mut f, &viewports, &ids);
    // The classes of kept objects and of entities of types CADCraft doesn't model.
    let mut types: HashSet<String> = f.objects.iter().map(|o| up(&o.kind)).collect();
    types.extend(all_entities(d).filter_map(|e| match &e.kind {
        EntityKind::Unknown(u) => Some(up(&u.dxf_type)),
        _ => None,
    }));
    // A proxy's class is the one its group 91 numbers in the file's CLASSES: keep it too, and the
    // file's class order, by which saving renumbers the proxy.
    let class_order: Vec<String> =
        classes.iter().filter(|(k, _, _)| k == "CLASS").take(MAX_OBJECTS).map(|(_, _, t)| up(&val(t, 1).unwrap_or_default())).collect();
    let mut proxies = false;
    let proxy_tags = f.objects.iter().filter(|o| is_proxy(&o.kind)).map(|o| &o.tags);
    let proxy_tags = proxy_tags.chain(all_entities(d).filter_map(|e| match &e.kind {
        EntityKind::Unknown(u) if is_proxy(&u.dxf_type) => Some(&u.tags),
        _ => None,
    }));
    for tags in proxy_tags {
        proxies = true;
        if let Some(name) = class_index(class_number(tags.iter().map(|r| (r.code, r.value.as_str())))).and_then(|i| class_order.get(i)) {
            types.insert(name.clone());
        }
    }
    if proxies {
        f.class_order = class_order;
    }
    for (kind, _, t) in &classes {
        if kind == "CLASS" && val(t, 1).is_some_and(|n| types.contains(&up(&n))) && f.classes.len() < MAX_OBJECTS {
            f.classes.push(t.iter().map(raw).collect());
        }
    }
    // The roles of the handles kept data refers to.
    let mut refs: Vec<String> = Vec::new();
    let mut note = |tags: &[RawTag]| refs.extend(tags.iter().filter(|r| is_pointer(r.code)).map(|r| norm(&r.value)));
    for o in f.objects.iter().chain(&f.table_records) {
        note(&o.tags);
    }
    for x in f.entities.values().chain(f.records.values()) {
        note(&x.xdata);
        note(&x.props);
        note(&x.subclass);
        note(&x.layout);
    }
    for e in all_entities(d) {
        note(&e.common.xdata);
    }
    for e in all_entities(d) {
        if let EntityKind::Unknown(u) = &e.kind {
            note(&u.tags);
        }
    }
    for r in &refs {
        if let (Some(h), Some(k), false) = (handle(r), ids.get(r), kept.contains(r)) {
            f.targets.insert(h, k.clone());
        }
    }
    // The data store (ACDSDATA): the ACIS geometry of solids, regions and bodies from 2013 on.
    if let Some(ds) = secs.iter().find(|s| s.name.eq_ignore_ascii_case("ACDSDATA")) {
        f.data_store = ds.tags.iter().take(MAX_DATA_STORE_GROUPS).map(raw).collect();
    }
    if !f.is_empty() {
        d.foreign = Arc::new(f);
    }
}

/// A sun is hard-owned by the viewport or VPORT record it lights, which points at it (group 361).
/// The owner is written from the model, so the link is rebuilt from the sun's owner (a DWG's
/// viewport entity has none in its DXF text).
fn link_suns(f: &mut Foreign, viewports: &HashSet<String>, ids: &HashMap<String, ForeignKey>) {
    let owner_of = |o: &ForeignObject| cadcraft_doc::foreign::object_owner(o.tags.iter().map(|t| (t.code, t.value.as_str())));
    let suns: Vec<(Handle, RawTag)> =
        f.objects.iter().filter(|o| up(&o.kind) == "SUN").filter_map(|o| Some((owner_of(o)?, RawTag { code: 361, value: o.handle.hex() }))).collect();
    for (owner, link) in suns {
        if viewports.contains(&owner.hex()) {
            f.entities.entry(owner).or_default().subclass.push(link);
        } else if let Some(key) = ids.get(&owner.hex()).filter(|k| k.0 == "VPORT") {
            f.records.entry(key.clone()).or_default().subclass.push(link);
        }
    }
}

fn entity_types(drawing: &Drawing) -> HashMap<String, bool> {
    let mut types = HashMap::new();
    for entity in all_entities(drawing) {
        types.entry(entity.handle.hex()).or_insert(matches!(entity.kind, EntityKind::Unknown(_) | EntityKind::Raw(_)));
        if let EntityKind::Raw(raw) = &entity.kind {
            types.extend(raw.followers().iter().map(|record| (record.handle().hex(), true)));
        }
        for h in entity.kind.attributes().iter().filter_map(|a| a.handle).chain(entity.kind.polyline_records().filter_map(|r| r.handle)) {
            types.entry(h.hex()).or_insert(false);
        }
    }
    types
}

fn extension_dictionaries(
    tables: &[(String, usize, &[Tag])],
    blocks: &[(String, usize, &[Tag])],
    entities: &[(String, usize, &[Tag])],
    objects: &[(String, usize, &[Tag])],
) -> HashMap<String, String> {
    let mut dictionaries = HashMap::new();
    let modelled = objects.iter().filter(|(kind, _, _)| MODELLED_OBJECTS.contains(&kind.as_str()));
    for (kind, _, tags) in tables.iter().chain(blocks).chain(entities).chain(modelled) {
        if kind == "ENDBLK" {
            continue;
        }
        let header = head(tags);
        if let (Some(owner), Some(dictionary)) = (header.handle, header.xdict) {
            dictionaries.entry(owner).or_insert(dictionary);
        }
    }
    dictionaries
}

/// What deciding which objects of a file are kept looks at.
struct Scan<'a> {
    objects: &'a Recs<'a>,
    /// The drawing's entities by handle: whether each is of a type CADCraft doesn't model.
    entities: &'a HashMap<String, bool>,
    /// The roles of the table records, blocks and named object dictionary entries.
    records: &'a HashMap<String, ForeignKey>,
    /// Objects by handle (those that are no entity).
    by_handle: &'a HashMap<String, usize>,
    root_entries: &'a [(String, String)],
    /// Extension dictionaries: owner → dictionary.
    xdict_of: &'a HashMap<String, String>,
    modelled_assoc: &'a HashSet<String>,
    /// The viewport entities of the file.
    viewports: &'a HashSet<String>,
    /// The table entities of the file, which hard-own the FIELD objects of their cells.
    tables: &'a HashSet<String>,
}

impl Scan<'_> {
    /// Whether `owner` is a viewport or a VPORT record, which hard-own a SUN (and write group 361).
    fn sun_owner(&self, owner: &str) -> bool {
        self.viewports.contains(owner) || self.records.get(owner).is_some_and(|key| key.0 == "VPORT")
    }

    /// Whether object `index` is the extension dictionary of something CADCraft writes itself: an
    /// entity, a table record or block, or an object of [`MODELLED_OBJECTS`] (not of another object).
    fn is_xdict(&self, index: usize) -> bool {
        let Some((_, _, tags)) = self.objects.get(index) else { return false };
        let header = head(tags);
        let (Some(handle), Some(owner)) = (header.handle, header.owner) else { return false };
        let by_model =
            self.by_handle.get(&owner).is_none_or(|&i| self.objects.get(i).is_some_and(|(kind, _, _)| MODELLED_OBJECTS.contains(&kind.as_str())));
        by_model && self.xdict_of.get(&owner) == Some(&handle) && (self.entities.contains_key(&owner) || self.records.contains_key(&owner))
    }
}

/// Determine which objects belong to unmodelled dictionary entries or owners.
fn retained_objects(scan: &Scan) -> HashMap<usize, bool> {
    let Scan { objects, entities, root_entries, by_handle, modelled_assoc, .. } = *scan;
    let root = objects.first().filter(|(kind, _, _)| is_dictionary(kind)).and_then(|(_, _, tags)| head(tags).handle);
    let root_key = |handle: &str| root_entries.iter().find(|(_, entry_handle)| entry_handle == handle).map(|(key, _)| key);
    let is_xdict = |index: usize| scan.is_xdict(index);
    let modelled_entry = |tags: &[Tag], handle: &str| {
        entries(tags).iter().any(|(key, entry_handle)| entry_handle == handle && modelled_xdict_entry(key, entry_handle, modelled_assoc))
    };
    let mut retained = HashMap::new();
    for index in 0..objects.len() {
        let mut path = Vec::new();
        let mut current = index;
        let keep = loop {
            if let Some(&known) = retained.get(&current) {
                break known;
            }
            if path.contains(&current) || path.len() >= MAX_FOREIGN_DEPTH {
                break false;
            }
            path.push(current);
            let Some((kind, _, tags)) = objects.get(current) else { break false };
            // The VX table has no owner (its control record's is null): the bridge keeps it for the DWG.
            if up(kind) == VX_CONTROL {
                break true;
            }
            let header = head(tags);
            let (Some(handle), Some(owner)) = (header.handle, header.owner) else { break false };
            if by_handle.get(&handle) != Some(&current) {
                break false;
            }
            if root.as_deref() == Some(owner.as_str()) {
                break root_key(&handle).is_some_and(|key| !modelled_key(key));
            }
            if is_xdict(current) {
                // An extension dictionary with no entry at all is kept (AutoCAD keeps them on the objects that had one); one whose
                // entries are all modelled is written from the model.
                let all = entries(tags);
                break all.is_empty() || all.iter().any(|(key, h)| !modelled_xdict_entry(key, h, modelled_assoc));
            }
            if let Some(&parent) = by_handle.get(&owner) {
                if is_xdict(parent) && objects.get(parent).is_some_and(|object| modelled_entry(object.2, &handle)) {
                    break false;
                }
                current = parent;
                continue;
            }
            break entities.get(&owner) == Some(&true)
                || (kind == "SUN" && scan.sun_owner(&owner))
                || (up(kind) == "FIELD" && scan.tables.contains(&owner));
        };
        for visited in path {
            retained.insert(visited, keep);
        }
    }
    retained
}

/// Proxy records (DXF Reference, ACAD_PROXY_OBJECT and ACAD_PROXY_ENTITY): another application's
/// objects and entities, whose class is named by number in group 91.
fn is_proxy(kind: &str) -> bool {
    matches!(up(kind).as_str(), "ACAD_PROXY_OBJECT" | "ACAD_PROXY_ENTITY")
}

/// Where a proxy's class number is: its first group 91 outside the `102 {NAME` … `102 }` groups
/// (an application's can hold a 91 of its own) and before any xdata.
fn class_group<S: AsRef<str>>(groups: impl IntoIterator<Item = (i32, S)>) -> Option<usize> {
    let mut in_group = false;
    for (i, (code, value)) in groups.into_iter().enumerate() {
        match code {
            1000.. => return None,
            102 => in_group = value.as_ref().trim().starts_with('{'),
            91 if !in_group => return Some(i),
            _ => {}
        }
    }
    None
}

/// A proxy's class number (see [`class_group`]).
fn class_number<S: AsRef<str>>(groups: impl IntoIterator<Item = (i32, S)> + Clone) -> Option<i64> {
    let i = class_group(groups.clone())?;
    groups.into_iter().nth(i).and_then(|(_, v)| v.as_ref().trim().parse().ok())
}

/// The place in CLASSES a class number names (500 is the first class).
fn class_index(number: Option<i64>) -> Option<usize> {
    usize::try_from(number?.checked_sub(500)?).ok()
}

/// Set a proxy's class number (see [`class_group`]).
fn set_class_number(tags: &mut [Tag], n: i64) {
    if let Some(t) = class_group(tags.iter().map(|t| (t.code, t.str()))).and_then(|i| tags.get_mut(i)) {
        *t = Tag::i(91, n);
    }
}

/// Most groups of a data store section kept (a damaged file's can be endless).
const MAX_DATA_STORE_GROUPS: usize = 1 << 26;

/// Write the drawing's kept data store section (ACDSDATA) before the end of `tags`, without the
/// records of entities that are no longer written. Only for files of AutoCAD 2013 and later
/// (`modern`), whose solids, regions and bodies keep their geometry there; the DWG bridge reads it
/// too. Nothing is written when no record's entity remains.
pub(crate) fn data_store(tags: &mut Vec<Tag>, d: &Drawing, modern: bool) {
    let ds = &d.foreign.data_store;
    if !modern || ds.is_empty() {
        return;
    }
    let written: HashSet<String> = tags.iter().filter(|t| t.code == 5).map(|t| norm(&t.str())).collect();
    // The section's own groups before its first record, then its schemas and records.
    let mut out: Vec<Tag> = vec![Tag::s(0, "SECTION"), Tag::s(2, "ACDSDATA")];
    let first = ds.iter().position(|t| t.code == 0).unwrap_or(ds.len());
    out.extend(ds.iter().take(first).map(tag));
    let mut records = 0usize;
    let mut i = first;
    while i < ds.len() {
        let end = ds.iter().skip(i + 1).position(|t| t.code == 0).map_or(ds.len(), |k| i + 1 + k);
        let rec = ds.get(i..end).unwrap_or(&[]);
        let is_record = rec.first().is_some_and(|t| t.value.trim().eq_ignore_ascii_case("ACDSRECORD"));
        let owner = rec.iter().find(|t| t.code == 320).map(|t| norm(&t.value));
        if !is_record || owner.as_ref().is_some_and(|h| written.contains(h)) {
            records += usize::from(is_record);
            out.extend(rec.iter().map(tag));
        }
        i = end;
    }
    if records == 0 {
        return;
    }
    out.push(Tag::s(0, "ENDSEC"));
    let at = tags.iter().rposition(|t| t.code == 0 && t.str() == "EOF").unwrap_or(tags.len());
    tags.splice(at..at, out);
}

/// An entity of a type CADCraft doesn't model, as read, with its handle, owner, space and layer
/// from the drawing. Its reactors, extension dictionary and xdata are added by [`finish`] (they
/// are kept with the drawing's other foreign data); its references are remapped there too.
pub(crate) fn unknown_entity(u: &Unknown, e: &Entity, owner: &str, paper: bool) -> Vec<Tag> {
    let kind = up(&u.dxf_type);
    if STRUCTURAL.contains(&kind.as_str()) || u.dxf_type.trim() != u.dxf_type {
        return Vec::new();
    }
    let mut out = vec![Tag::s(0, u.dxf_type.clone()), Tag::s(5, e.handle.hex()), Tag::s(330, owner)];
    let fixed = out.len();
    let mut in_group = false;
    let mut owner_seen = false;
    let mut layer = false;
    let mut paper_done = !paper;
    for r in u.tags.iter().take_while(|r| r.code < 1000) {
        match r.code {
            102 => {
                in_group = r.value.trim().starts_with('{');
                continue;
            }
            _ if in_group => continue,
            5 | 67 => continue,
            330 if !owner_seen => {
                owner_seen = true;
                continue;
            }
            8 => {
                layer = true;
                out.push(Tag::s(8, e.common.layer.clone()));
                continue;
            }
            _ => {}
        }
        out.push(tag(r));
        if r.code == 100 && r.value.trim() == "AcDbEntity" && !paper_done {
            out.push(Tag::i(67, 1));
            paper_done = true;
        }
    }
    let mut lead = Vec::new();
    if !paper_done {
        lead.push(Tag::i(67, 1));
    }
    if !layer {
        lead.push(Tag::s(8, e.common.layer.clone()));
    }
    out.splice(fixed..fixed, lead);
    out
}

/// A written record: section, type, the index of its `0` group and the end of its groups.
struct Rec {
    sec: String,
    kind: String,
    start: usize,
    end: usize,
}

fn written_records(tags: &[Tag]) -> Vec<Rec> {
    let mut out: Vec<Rec> = Vec::new();
    let mut sec = String::new();
    for (kind, start, body) in split(tags) {
        if kind == "SECTION" {
            sec = body.iter().find(|t| t.code == 2).map(Tag::str).unwrap_or_default();
        }
        out.push(Rec { sec: sec.clone(), kind, start, end: start + 1 + body.len() });
        if out.last().is_some_and(|r| r.kind == "ENDSEC") {
            sec.clear();
        }
    }
    out
}

/// Remaps the kept data's handles onto a written file.
struct Map<'a> {
    f: &'a Foreign,
    /// Kept objects that are written: file handle → new handle.
    alive: HashMap<String, String>,
    /// Kept objects, written or not, and their original owners.
    objects: HashMap<String, &'a ForeignObject>,
    owners: HashMap<String, String>,
    /// Role → handle in the written file.
    roles: HashMap<ForeignKey, String>,
    /// Entities in the written file.
    entities: HashSet<String>,
    /// Parent and follower records that belong to opaque source units.
    raw: HashSet<String>,
    /// The snapshots of the source geometry that are written back, by entity or attribute handle.
    geometry: HashMap<Handle, Arc<cadcraft_doc::GeometrySnapshot>>,
    /// The file's `$ACADVER` as a number (1015 for AC1015), 0 when unknown.
    version: u32,
    /// The actual save target, which can differ from the DWG bridge's DXF declaration.
    target: cadcraft_dxf::Version,
    pages: HashMap<String, &'a cadcraft_doc::PageSetup>,
    block_attributes: HashMap<String, bool>,
    table_aliases: HashMap<String, &'a ForeignObject>,
}

impl Map<'_> {
    fn write_record(
        &self,
        record_info: &Rec,
        body: &[Tag],
        extra: Option<&ForeignExtra>,
        geometry: Option<&cadcraft_doc::GeometryRecord>,
        root: bool,
        out: &mut Vec<Tag>,
    ) {
        if head(body).handle.is_some_and(|handle| self.raw.contains(&handle)) {
            crate::dxf_write::remap_raw_references(body, |handle| self.get(handle), out);
        } else if extra.is_none() && geometry.is_none() && !root {
            out.extend(body.iter().cloned());
        } else {
            record(self, record_info, body, extra, geometry, false, root, out);
        }
    }

    fn replayed_record(&self, rec: &Rec, body: &[Tag], dictionaries: &HashMap<String, &ForeignObject>) -> Option<Vec<Tag>> {
        match (rec.sec.as_str(), rec.kind.as_str()) {
            ("TABLES", "STYLE") => head(body).handle.and_then(|handle| self.table_aliases.get(&handle)).map(|source| {
                let mut tags = Vec::new();
                self.object(source, &mut tags);
                tags
            }),
            ("OBJECTS", kind) if is_dictionary(kind) => {
                head(body).handle.and_then(|handle| dictionaries.get(&handle)).map(|source| merge_xdict(self, source, body))
            }
            _ => None,
        }
    }

    /// The written handle for a kept reference, `Some("0")` for the null handle, `None` when what
    /// it named is gone.
    fn get(&self, old: &str) -> Option<String> {
        let v = norm(old);
        if v.is_empty() || v == "0" {
            return Some("0".into());
        }
        if self.objects.contains_key(&v) {
            return self.alive.get(&v).cloned();
        }
        let h = handle(&v)?;
        if let Some(role) = self.f.targets.get(&h) {
            return self.roles.get(role).cloned();
        }
        self.entities.contains(&v).then_some(v)
    }

    fn owns_xdict(&self, dictionary: &str, owner: &str) -> bool {
        self.objects.get(dictionary).is_some_and(|o| is_dictionary(&o.kind))
            && self.owners.get(dictionary).and_then(|h| self.get(h)).is_some_and(|h| h == owner)
    }

    /// A kept group with its handle remapped (a dangling one becomes null).
    fn group(&self, r: &RawTag) -> Tag {
        if is_pointer(r.code) { Tag::s(r.code, self.get(&r.value).unwrap_or_else(|| "0".into())) } else { tag(r) }
    }

    /// A `102 {NAME` group of the handles that still exist (nothing if none does).
    fn handle_group(&self, out: &mut Vec<Tag>, name: &str, code: i32, handles: &[String]) {
        let live: Vec<String> = handles.iter().filter_map(|h| self.get(h)).filter(|h| h != "0").collect();
        if live.is_empty() {
            return;
        }
        out.push(Tag::s(102, format!("{{{name}")));
        out.extend(live.into_iter().map(|h| Tag::s(code, h)));
        out.push(Tag::s(102, "}"));
    }

    /// A kept object's groups, remapped.
    fn object(&self, o: &ForeignObject, out: &mut Vec<Tag>) {
        out.push(Tag::s(0, o.kind.clone()));
        let dict = is_dictionary(&o.kind);
        let mut i = 0;
        while let Some(r) = o.tags.get(i) {
            i += 1;
            if r.code == 102 && r.value.trim().starts_with('{') {
                // A `{NAME` group is a handle list (`{ACAD_REACTORS 330 … }`) only when every
                // group inside it is a reference. Application data groups also use 102 brackets
                // with mixed values and their own closing form — the AcDs thumbnail schema's
                // `{ATTRRECORD 341 h 2 name 280 1 291 1 ATTRRECORD}` — and collapsing those into
                // a handle list rewrote the handle under the LAST code read (291, a bool) and
                // dropped the rest, which strict DXF readers reject. Those are written as read,
                // references remapped, by the plain path below.
                let end = (i..o.tags.len()).find(|&k| o.tags[k].code == 102).unwrap_or(o.tags.len());
                let inner = &o.tags[i..end];
                if !inner.is_empty() && !inner.iter().all(|n| is_pointer(n.code)) {
                    out.push(tag(r));
                    continue;
                }
                let name = r.value.trim().trim_start_matches('{').to_string();
                let mut hs = Vec::new();
                let mut code = 330;
                while let Some(n) = o.tags.get(i) {
                    if n.code == 102 {
                        if n.value.trim() == "}" {
                            i += 1;
                        }
                        break;
                    }
                    code = n.code;
                    hs.push(n.value.clone());
                    i += 1;
                }
                if name == "ACAD_XDICTIONARY" {
                    let owner = self.get(&o.handle.hex());
                    hs.retain(|h| owner.as_ref().is_some_and(|owner| self.owns_xdict(&norm(h), owner)));
                }
                self.handle_group(out, &name, code, &hs);
                continue;
            }
            if dict && r.code == 3 && o.tags.get(i).is_some_and(|n| matches!(n.code, 350 | 360)) {
                let target = o.tags.get(i).map(|n| (n.code, self.get(&n.value)));
                i += 1;
                if let Some((code, Some(h))) = target.filter(|(_, h)| h.as_deref().is_some_and(|h| h != "0")) {
                    out.push(tag(r));
                    out.push(Tag::s(code, h));
                }
                continue;
            }
            if matches!(r.code, 5 | 105) {
                out.push(Tag::s(r.code, self.get(&r.value).unwrap_or_else(|| "0".into())));
                continue;
            }
            out.push(self.group(r));
        }
    }
}

/// The reactors the writer listed for a record (an associative hatch's boundary has its hatch) keep the ones the file had besides:
/// at the end of the `ACAD_REACTORS` group (`t` is its closing `102`), those of `x` that still exist and are not listed.
/// `listed` holds the handles of the group being read.
fn add_kept_reactors(t: &Tag, x: Option<&ForeignExtra>, map: &Map, listed: &mut Option<HashSet<String>>, out: &mut Vec<Tag>) {
    if t.code == 102 {
        if t.str().trim() == "{ACAD_REACTORS" {
            *listed = Some(HashSet::new());
        } else if let (Some(seen), Some(x)) = (listed.take(), x) {
            out.extend(x.reactors.iter().filter_map(|r| map.get(&r.hex())).filter(|r| r != "0" && !seen.contains(r)).map(|r| Tag::s(330, r)));
        }
    } else if let Some(seen) = listed.as_mut()
        && is_pointer(t.code)
    {
        seen.insert(norm(&t.str()));
    }
}

/// Keep the raw dictionary's metadata but give modelled keys their newly written values.
/// The raw dictionary is aliased to this written handle, so every child/foreign pointer shares
/// the same owner. Values here are already remapped: generated references must not be remapped twice.
fn merge_xdict(map: &Map<'_>, foreign: &ForeignObject, written: &[Tag]) -> Vec<Tag> {
    let mut raw = Vec::new();
    map.object(foreign, &mut raw);
    let pairs: Vec<_> = written
        .windows(2)
        .filter_map(|p| match p {
            [key, target] if key.code == 3 && matches!(target.code, 350 | 360) => Some((key, target)),
            _ => None,
        })
        .collect();
    let keys: HashSet<_> = pairs.iter().map(|(key, _)| up(&key.str())).collect();
    let mut merged = Vec::with_capacity(raw.len() + pairs.len() * 2);
    let mut i = 0;
    while let Some(t) = raw.get(i) {
        if t.code == 3 && keys.contains(&up(&t.str())) && raw.get(i + 1).is_some_and(|p| matches!(p.code, 350 | 360)) {
            i += 2;
            continue;
        }
        merged.push(t.clone());
        i += 1;
    }
    let at = merged.iter().position(|t| t.code == 1001).unwrap_or(merged.len());
    merged.splice(at..at, pairs.into_iter().flat_map(|(key, target)| [key.clone(), target.clone()]));
    let mut in_reactors = false;
    let mut seen = HashSet::new();
    for t in merged.iter().take_while(|t| t.code < 1000) {
        if t.code == 102 {
            in_reactors = t.str().trim() == "{ACAD_REACTORS";
        } else if in_reactors && is_pointer(t.code) {
            seen.insert(norm(&t.str()));
        }
    }
    let needed: Vec<_> = head(written).reactors.into_iter().filter(|h| h != "0" && seen.insert(h.clone())).map(|h| Tag::s(330, h)).collect();
    if !needed.is_empty() {
        let closing = merged
            .iter()
            .position(|t| t.code == 102 && t.str().trim() == "{ACAD_REACTORS")
            .and_then(|start| merged.iter().enumerate().skip(start + 1).find(|(_, t)| t.code == 102).map(|(i, _)| i));
        if let Some(at) = closing {
            merged.splice(at..at, needed);
        } else {
            let at = merged.iter().position(|t| t.code == 5).map_or(1, |i| i + 1);
            let mut group = vec![Tag::s(102, "{ACAD_REACTORS")];
            group.extend(needed);
            group.push(Tag::s(102, "}"));
            merged.splice(at..at, group);
        }
    }
    merged
}

/// What the extras of a written record are looked up in.
struct Extras<'a> {
    f: &'a Foreign,
    xd: &'a XdataOf<'a>,
    /// The written file's roles, which name the objects of [`MODELLED_OBJECTS`].
    ids: &'a HashMap<String, ForeignKey>,
}

impl<'a> Extras<'a> {
    /// The extras of a written record, if the drawing keeps any for it.
    fn of(&self, rec: &Rec, body: &[Tag]) -> Option<Cow<'a, ForeignExtra>> {
        let f = self.f;
        match (rec.sec.as_str(), rec.kind.as_str()) {
            ("TABLES", "VPORT") => f.records.get(self.ids.get(&head(body).handle?)?).map(Cow::Borrowed),
            ("TABLES", kind) => f.records.get(&(kind.to_string(), up(&val(body, 2).unwrap_or_default()))).map(Cow::Borrowed),
            ("BLOCKS", "BLOCK") => f.records.get(&("BLOCK".into(), up(&val(body, 2).unwrap_or_default()))).map(Cow::Borrowed),
            ("BLOCKS", "ENDBLK") => f.records.get(self.ids.get(&head(body).handle?)?).map(Cow::Borrowed),
            ("ENTITIES" | "BLOCKS", _) => entity_extra(f, self.xd, handle(&head(body).handle?)?),
            ("OBJECTS", kind) if MODELLED_OBJECTS.contains(&kind) || is_dictionary(kind) => {
                f.records.get(self.ids.get(&head(body).handle?)?).map(Cow::Borrowed)
            }
            _ => None,
        }
    }
}

/// Plan standalone/merged xdict ownership separately from writing their object graphs.
fn plan_xdict_attachments(
    extras: &Extras,
    recs: &[Rec],
    tags: &[Tag],
    roles: &HashMap<ForeignKey, String>,
    objects: &HashMap<String, &ForeignObject>,
    owners: &HashMap<String, String>,
) -> (HashSet<String>, HashMap<String, String>) {
    let mut attached = HashSet::new();
    let mut merged = HashMap::new();
    for r in recs {
        let body = tags.get(r.start + 1..r.end).unwrap_or(&[]);
        if let Some(x) = extras.of(r, body)
            && let Some(dictionary) = x.xdict
        {
            let old = dictionary.hex();
            let header = head(body);
            let owned = owners.get(&old).is_some_and(|owner| {
                let current = handle(owner).and_then(|h| extras.f.targets.get(&h)).and_then(|role| roles.get(role)).unwrap_or(owner);
                header.handle.as_ref() == Some(current)
            });
            if !(owned && objects.get(&old).is_some_and(|o| is_dictionary(&o.kind))) {
                continue;
            }
            attached.insert(old.clone());
            if let Some(written) = header.xdict {
                merged.insert(old, written);
            }
        }
    }
    (attached, merged)
}

/// Entities' own xdata ([`cadcraft_doc::Common::xdata`]), by handle.
type XdataOf<'a> = HashMap<Handle, &'a [RawTag]>;

/// The extras of entity `h`: what the drawing keeps for it, with the entity's own xdata.
fn entity_extra<'a>(f: &'a Foreign, xd: &XdataOf, h: Handle) -> Option<Cow<'a, ForeignExtra>> {
    let kept = f.entities.get(&h);
    match xd.get(&h) {
        None => kept.map(Cow::Borrowed),
        Some(xdata) => {
            let mut x = kept.cloned().unwrap_or_default();
            x.xdata = xdata.to_vec();
            Some(Cow::Owned(x))
        }
    }
}

fn raw_handles(d: &Drawing) -> HashSet<String> {
    all_entities(d)
        .filter_map(|e| if let EntityKind::Raw(raw) = &e.kind { Some(raw) } else { None })
        .flat_map(|raw| raw.records().map(|record| record.handle().hex()))
        .collect()
}

/// Every Unknown entity's handle (upper case).
fn unknown_handles(d: &Drawing) -> HashSet<String> {
    all_entities(d).filter(|e| matches!(e.kind, EntityKind::Unknown(_))).map(|e| e.handle.hex()).collect()
}

fn shape_style(object: &ForeignObject) -> bool {
    object.kind == "STYLE" && first_group(&object.tags, 70).is_some_and(|t| t.value.parse::<i64>().is_ok_and(|flags| flags & 1 != 0))
}

fn allocate_table_records<'a>(
    f: &'a Foreign,
    roles: &HashMap<ForeignKey, String>,
    tables: &Recs<'_>,
    alive: &mut HashMap<String, String>,
    next: &mut u64,
) -> HashMap<String, &'a ForeignObject> {
    let mut generated: HashMap<_, _> = tables
        .iter()
        .filter(|(kind, _, tags)| {
            kind == "STYLE" && val(tags, 2).is_none_or(|name| name.is_empty()) && tags.iter().any(|t| t.code == 70 && t.i64() & 1 != 0)
        })
        .filter_map(|(_, _, tags)| Some((up(&val(tags, 3)?), head(tags).handle?)))
        .collect();
    let mut aliases = HashMap::new();
    for object in &f.table_records {
        if !roles.contains_key(&("TABLE".into(), object.kind.clone())) {
            continue;
        }
        // A linetype shape file already has a generated slot. Reuse it, including its
        // source fields, rather than adding another copy on every save.
        let slot = shape_style(object).then(|| first_group(&object.tags, 3)).flatten().and_then(|font| generated.remove(&up(&font.value)));
        if let Some(handle) = slot {
            alive.insert(object.handle.hex(), handle.clone());
            aliases.insert(handle, object);
        } else {
            alive.insert(object.handle.hex(), format!("{next:X}"));
            *next = next.saturating_add(1);
        }
    }
    aliases
}

/// The names a user DXF gives the nameless STYLE and DIMSTYLE records kept from a DWG: AutoCAD
/// refuses a table record with an empty name (it renames them itself, to AUDIT_I_...). Each name is
/// unique among the written records of its table. A DWG bridge (`dwg_bridge`) keeps the records nameless.
/// Shape-file entries legitimately remain nameless in both formats.
fn names_for_unnamed(f: &Foreign, tables: &Recs<'_>, dwg_bridge: bool) -> HashMap<Handle, String> {
    let mut used: HashSet<(String, String)> = tables.iter().map(|(kind, _, t)| (kind.clone(), up(&val(t, 2).unwrap_or_default()))).collect();
    let mut out = HashMap::new();
    for o in f.table_records.iter().filter(|o| !dwg_bridge && matches!(o.kind.as_str(), "STYLE" | "DIMSTYLE") && !shape_style(o)) {
        let n = (1..).map(|n| format!("UNNAMED_{}_{n}", o.kind)).find(|n| used.insert((o.kind.clone(), up(n)))).unwrap_or_default();
        out.insert(o.handle, n);
    }
    out
}

/// Write retained records before ENDTAB; return the count to add to a TABLE header. `names` are the
/// names to give nameless records (see [`names_for_unnamed`]).
fn write_table_records(map: &Map<'_>, r: &Rec, body: &[Tag], table: &mut String, names: &HashMap<Handle, String>, out: &mut Vec<Tag>) -> usize {
    if r.sec != "TABLES" {
        return 0;
    }
    let standalone = |object: &ForeignObject| map.alive.get(&object.handle.hex()).is_some_and(|handle| !map.table_aliases.contains_key(handle));
    if r.kind == "TABLE" {
        *table = val(body, 2).unwrap_or_default();
        return map.f.table_records.iter().filter(|o| o.kind == *table && standalone(o)).count();
    }
    if r.kind == "ENDTAB" {
        for o in map.f.table_records.iter().filter(|o| o.kind == *table && standalone(o)) {
            let start = out.len();
            map.object(o, out);
            if let Some(name) = names.get(&o.handle)
                && let Some(t) = out.get_mut(start..).and_then(|t| t.iter_mut().find(|t| t.code == 2))
            {
                *t = Tag::s(2, name.clone());
            }
        }
    }
    0
}

fn raw_dwg_object(object: &ForeignObject) -> bool {
    object.tags.iter().any(|t| t.code == 100 && t.value == "AcDbAcadrustRawDwgObject")
}

/// The private records with which the DWG bridge carries the VX table (the viewport entity headers of a drawing
/// of 2004 or later, which AutoCAD's DXFOUT does not write): the control record and one record per entry.
const VX_CONTROL: &str = "ACADRUST_VXCONTROL";
const VX_RECORD: &str = "ACADRUST_VXRECORD";

fn vx_table_object(object: &ForeignObject) -> bool {
    let kind = up(&object.kind);
    kind == VX_CONTROL || kind == VX_RECORD
}

fn public_foreign(f: &Foreign) -> Foreign {
    let mut public = f.clone();
    let private: HashSet<String> = public.objects.iter().filter(|o| raw_dwg_object(o)).map(|o| up(&o.kind)).collect();
    public.objects.retain(|o| !raw_dwg_object(o) && !vx_table_object(o));
    // AutoCAD writes a dynamic block's purge preventer without its block (group 340), and refuses
    // a DXF preventer that has one; the DWG bridge keeps it.
    for o in public.objects.iter_mut().filter(|o| up(&o.kind) == "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION") {
        if let Some(marker) = o.tags.iter().position(|t| t.code == 100 && t.value.trim().eq_ignore_ascii_case("AcDbDynamicBlockPurgePreventer")) {
            let mut at = 0;
            o.tags.retain(|t| {
                at += 1;
                at <= marker + 1 || t.code != 340
            });
        }
    }
    let kept: HashSet<String> = public.objects.iter().map(|o| up(&o.kind)).collect();
    public.classes.retain(|c| c.iter().find(|t| t.code == 1).map(|n| up(&n.value)).is_none_or(|n| !private.contains(&n) || kept.contains(&n)));
    public
}

/// The upper-case names of the classes only private DWG records use: a user DXF does not name them (the DWG bridge keeps them).
pub(crate) fn private_classes(d: &Drawing) -> HashSet<String> {
    let public = public_foreign(&d.foreign);
    let name = |c: &Vec<RawTag>| c.iter().find(|t| t.code == 1).map(|n| up(&n.value));
    let kept: HashSet<String> = public.classes.iter().filter_map(name).collect();
    d.foreign.classes.iter().filter_map(name).filter(|n| !kept.contains(n)).collect()
}

/// The form a kept record has in a DXF of `release` (an `$ACADVER` number), for the kinds whose form depends on
/// it: two records are the same form when these are equal.
fn release_form(kind: &str, release: u32) -> Option<u32> {
    match kind {
        "VISUALSTYLE" => Some(match release {
            ..=1023 => 0,
            1024..=1026 => 1,
            _ => 2,
        }),
        "ACDBASSOC2DCONSTRAINTGROUP" => Some(u32::from(release >= 1027)),
        // The embedded DWG entities are encoded with the DWG layout of their release.
        "ACSH_SWEEP_CLASS"
        | "ACSH_EXTRUSION_CLASS"
        | "ACSH_LOFT_CLASS"
        | "ACSH_REVOLVE_CLASS"
        | "EXTRUDEDSURFACE"
        | "LOFTEDSURFACE"
        | "REVOLVEDSURFACE"
        | "SWEPTSURFACE" => Some(release),
        _ => None,
    }
}

/// The form a constraint group record has, by its groups: the DWG bridge writes the registry layout (1) whatever the
/// release, and AutoCAD below 2013 writes each node's class name (group 1) followed by its owner (330), the inline
/// layout (0).
fn constraint_group_form(record: &[Tag]) -> u32 {
    let inline = record.iter().skip_while(|t| t.code != 1).nth(1).is_some_and(|t| t.code == 330);
    u32::from(!inline)
}

/// The kept records that were written for release `source` and are not in the form of release `target` (both
/// `$ACADVER` numbers) are written again in that form, by acadrust. A record that is already in it stays as it
/// is, and so does one acadrust does not read. The DWG bridge's embedded entity bytes say their own release in
/// group 455, which `source` does not replace.
pub(crate) fn reform_for_version(tags: &mut Vec<Tag>, source: u32, target: u32) {
    let starts: Vec<usize> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    let mut out: Vec<Tag> = Vec::with_capacity(tags.len());
    out.extend(tags.get(..starts.first().copied().unwrap_or(tags.len())).unwrap_or_default().iter().cloned());
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(tags.len());
        let record = tags.get(start..end).unwrap_or_default();
        let kind = record.first().map(|t| up(&t.str())).unwrap_or_default();
        let from = record.iter().find(|t| t.code == 455).map_or(source, |t| u32::try_from(t.i64()).unwrap_or(source));
        let have = if kind == "ACDBASSOC2DCONSTRAINTGROUP" { Some(constraint_group_form(record)) } else { release_form(&kind, from) };
        let reformed = (have != release_form(&kind, target) && release_form(&kind, target).is_some())
            .then(|| {
                let groups: Vec<(i32, String)> = record.iter().map(|t| (t.code, t.str())).collect();
                cadcraft_dwg::reform_object(&kind, &groups, &format!("AC{from}"), &format!("AC{target}"))
            })
            .flatten();
        match reformed {
            Some(groups) => out.extend(groups.iter().map(|(code, value)| tag(&RawTag { code: *code, value: value.clone() }))),
            None => out.extend(record.iter().cloned()),
        }
    }
    *tags = out;
}

/// The `$ACADVER` of the written file as a number (1015 for AC1015), 0 when it has none. The DXF of the DWG bridge says AC1015 but has
/// the fit data count of spline edges, which acadrust reads from there: it counts as 2010 (1024).
fn declared_version(tags: &[Tag], dwg_bridge: bool) -> u32 {
    let version = tags
        .windows(2)
        .find(|w| w[0].code == 9 && w[0].str() == "$ACADVER")
        .and_then(|w| w[1].str().trim().strip_prefix("AC").and_then(|v| v.parse().ok()))
        .unwrap_or(0);
    if dwg_bridge { version.max(1024) } else { version }
}

/// Put the drawing's foreign data back into a written file's tags (see the module comment).
pub(crate) fn finish(tags: &mut Vec<Tag>, d: &Drawing, dwg_bridge: bool, target: cadcraft_dxf::Version) {
    let public = (!dwg_bridge).then(|| public_foreign(&d.foreign));
    let f = public.as_ref().unwrap_or(&d.foreign);
    let opaque = raw_handles(d);
    let mut unknown = unknown_handles(d);
    unknown.extend(opaque.iter().cloned());
    let geometry = crate::dxf_geometry::eligible(d);
    let xd: XdataOf = all_entities(d).filter(|e| !e.common.xdata.is_empty()).map(|e| (e.handle, e.common.xdata.as_slice())).collect();
    if f.is_empty() && unknown.is_empty() && xd.is_empty() && d.registered_apps.is_empty() && geometry.is_empty() {
        // No kept object a header pointer could name.
        remap_header_pointers(tags, |_| "0".into());
        remap_table_fields(tags, |_| None);
        return;
    }
    // APPID roles must exist before handle remapping, not only after foreign xdata is emitted.
    let apps = f
        .objects
        .iter()
        .chain(&f.table_records)
        .flat_map(|o| o.tags.iter())
        .chain(f.entities.values().chain(f.records.values()).flat_map(|x| x.xdata.iter()))
        .chain(xd.values().flat_map(|tags| tags.iter()))
        .filter(|t| t.code == 1001)
        .map(|t| t.value.clone())
        // The registered names come before the roles, which only have the upper-case name: the first spelling seen is written.
        .chain(d.registered_apps.iter().cloned())
        .chain(f.targets.values().chain(f.records.keys()).filter(|key| key.0 == "APPID").map(|key| key.1.clone()));
    add_appids(tags, apps);
    let recs = written_records(tags);
    let body = |r: &Rec| tags.get(r.start + 1..r.end).unwrap_or(&[]);
    let by_sec = |name: &str| -> Recs {
        recs.iter().filter(|r| r.sec == name && r.kind != "SECTION" && r.kind != "ENDSEC").map(|r| (r.kind.clone(), r.start, body(r))).collect()
    };
    let (tables, blocks, objects) = (by_sec("TABLES"), by_sec("BLOCKS"), by_sec("OBJECTS"));
    let ids = identities(&tables, &blocks, &objects);
    let mut roles: HashMap<ForeignKey, String> = HashMap::new();
    for (h, k) in &ids {
        roles.entry(k.clone()).or_insert_with(|| h.clone());
    }
    let mut written: HashSet<String> = recs.iter().filter_map(|r| head(body(r)).handle).collect();
    // The written CLASSES as numbered (500 for the first): the writer's own, then the kept ones it
    // doesn't write (appended below, in this order). A kept proxy's group 91 is renumbered to its
    // class's place here; a proxy whose class isn't written is dropped (#1812).
    let class_names: HashSet<String> = recs.iter().filter(|r| r.kind == "CLASS").filter_map(|r| val(body(r), 1)).map(|n| up(&n)).collect();
    let appended = f.classes.iter().filter_map(|c| c.iter().find(|t| t.code == 1)).map(|n| up(&n.value)).filter(|n| !class_names.contains(n));
    let mut numbers: HashMap<String, i64> = HashMap::new();
    for (i, n) in recs.iter().filter(|r| r.kind == "CLASS").filter_map(|r| val(body(r), 1)).map(|n| up(&n)).chain(appended).enumerate() {
        numbers.entry(n).or_insert(500_i64.saturating_add(i64::try_from(i).unwrap_or(i64::MAX)));
    }
    let renumber = |old: Option<i64>| class_index(old).and_then(|i| f.class_order.get(i)).and_then(|n| numbers.get(n).copied());
    // Entities of types CADCraft doesn't model, placed among the written ones.
    let mut planned = plan_unknown(d, &recs, tags, &roles);
    for list in planned.values_mut() {
        list.retain_mut(|(_, t)| {
            if !t.first().is_some_and(|z| is_proxy(&z.str())) {
                return true;
            }
            let Some(n) = renumber(class_number(t.iter().map(|x| (x.code, x.str())))) else { return false };
            set_class_number(t, n);
            true
        });
    }
    for list in planned.values() {
        for (_, t) in list {
            written.extend(head(t.get(1..).unwrap_or(&[])).handle);
        }
    }
    let root = objects.first().filter(|(k, _, _)| is_dictionary(k)).map(|(_, s, _)| *s);
    let root_keys: HashSet<String> =
        objects.iter().find(|(_, s, _)| Some(*s) == root).map(|(_, _, t)| entries(t).into_iter().map(|(k, _)| up(&k)).collect()).unwrap_or_default();
    let objs: HashMap<String, &ForeignObject> = f.objects.iter().chain(&f.table_records).map(|o| (o.handle.hex(), o)).collect();
    let owners: HashMap<String, String> = f
        .objects
        .iter()
        .chain(&f.table_records)
        .filter_map(|o| Some((o.handle.hex(), cadcraft_doc::foreign::object_owner(o.tags.iter().map(|t| (t.code, t.value.as_str())))?.hex())))
        .collect();
    let extras = Extras { f, xd: &xd, ids: &ids };
    let (mut attached, merged) = plan_xdict_attachments(&extras, &recs, tags, &roles, &objs, &owners);
    for (_, t) in planned.values().flatten() {
        let header = head(t.get(1..).unwrap_or(&[]));
        let x = header.handle.as_ref().and_then(|h| handle(h)).and_then(|h| entity_extra(f, &xd, h));
        if let Some(dictionary) = x.and_then(|x| x.xdict).map(Handle::hex)
            && owners.get(&dictionary) == header.handle.as_ref()
            && objs.get(&dictionary).is_some_and(|o| is_dictionary(&o.kind))
        {
            attached.insert(dictionary);
        }
    }
    // The viewports written: a kept SUN hangs on one of them (or on a written VPORT record).
    let viewports: HashSet<String> =
        all_entities(d).filter(|e| matches!(e.kind, EntityKind::Viewport(_))).map(|e| e.handle.hex()).filter(|h| written.contains(h)).collect();
    // Which kept objects are written: those whose owner chain ends at a written owner.
    let dict_entries: HashMap<String, &str> = f.dictionary.iter().map(|(k, h)| (h.hex(), k.as_str())).collect();
    let nod = f.targets.iter().filter(|(_, k)| k.0 == "NOD" && k.1.is_empty()).map(|(h, _)| h.hex()).collect::<HashSet<_>>();
    let table_handles: HashSet<String> = all_entities(d).filter(|e| matches!(e.kind, EntityKind::Table(_))).map(|e| e.handle.hex()).collect();
    let mut alive_memo: HashMap<String, bool> = HashMap::new();
    for o in f.objects.iter().filter(|o| is_proxy(&o.kind) && renumber(class_number(o.tags.iter().map(|r| (r.code, r.value.as_str())))).is_none()) {
        alive_memo.insert(o.handle.hex(), false);
    }
    // The VX table's control record has no owner: the bridge keeps it, and its records with it.
    alive_memo.extend(f.objects.iter().filter(|o| up(&o.kind) == VX_CONTROL).map(|o| (o.handle.hex(), true)));
    for o in &f.objects {
        let mut path: Vec<String> = Vec::new();
        let mut cur = o.handle.hex();
        let verdict = loop {
            if let Some(&v) = alive_memo.get(&cur) {
                break v;
            }
            if path.contains(&cur) || path.len() >= MAX_FOREIGN_DEPTH {
                break false;
            }
            path.push(cur.clone());
            let Some(owner) = owners.get(&cur).cloned() else { break false };
            if objs.contains_key(&owner) {
                cur = owner;
                continue;
            }
            if nod.contains(&owner) {
                break dict_entries.get(&cur).is_some_and(|k| !root_keys.contains(&up(k))) && root.is_some();
            }
            if attached.contains(&cur) {
                break true;
            }
            let sun = o.kind.eq_ignore_ascii_case("SUN")
                && (viewports.contains(&owner)
                    || handle(&owner).and_then(|h| f.targets.get(&h)).is_some_and(|key| key.0 == "VPORT" && roles.contains_key(key)));
            let field = objs.get(&cur).is_some_and(|c| up(&c.kind) == "FIELD") && table_handles.contains(&owner);
            break (unknown.contains(&owner) && written.contains(&owner)) || (field && written.contains(&owner)) || sun;
        };
        for p in path {
            alive_memo.insert(p, verdict);
        }
    }
    // New handles for the written kept objects, above every written handle and the seed.
    let mut next = seed(tags).max(max_handle(tags).saturating_add(1));
    let mut alive: HashMap<String, String> = HashMap::new();
    for o in f.objects.iter().filter(|o| alive_memo.get(&o.handle.hex()) == Some(&true)) {
        if let Some(written) = merged.get(&o.handle.hex()) {
            alive.insert(o.handle.hex(), written.clone());
        } else {
            alive.insert(o.handle.hex(), format!("{next:X}"));
            next = next.saturating_add(1);
        }
    }
    let table_aliases = allocate_table_records(f, &roles, &tables, &mut alive, &mut next);
    let entities: HashSet<String> = entity_types(d).into_keys().filter(|h| written.contains(h)).collect();
    let merged_into: HashMap<_, _> = merged.iter().filter_map(|(old, new)| Some((new.clone(), *objs.get(old)?))).collect();
    let unnamed = names_for_unnamed(f, &tables, dwg_bridge);
    let version = declared_version(tags, dwg_bridge);
    let pages = d.layouts.iter().map(|layout| (up(&layout.name), &layout.page)).collect();
    let block_attributes = d.blocks.iter().map(|(name, block)| (up(name), nonconstant_attributes(block))).collect();
    let map =
        Map { f, alive, objects: objs, owners, roles, entities, raw: opaque, geometry, version, target, pages, block_attributes, table_aliases };
    let mut out: Vec<Tag> = Vec::with_capacity(tags.len() + f.objects.iter().chain(&f.table_records).map(|o| o.tags.len() + 1).sum::<usize>());
    let mut table = String::new();
    // The heavy polyline whose VERTEX records are being written, and how many it has had.
    let mut family: Option<(&cadcraft_doc::GeometrySnapshot, usize)> = None;
    for (n, r) in recs.iter().enumerate() {
        let b = body(r);
        let added = write_table_records(&map, r, b, &mut table, &unnamed, &mut out);
        if r.kind == "ENDSEC" && r.sec == "CLASSES" {
            for c in f.classes.iter().filter(|c| c.iter().find(|t| t.code == 1).is_some_and(|n| !class_names.contains(&up(&n.value)))) {
                out.push(Tag::s(0, "CLASS"));
                out.extend(c.iter().map(tag));
            }
        }
        if r.kind == "ENDSEC" && r.sec == "OBJECTS" {
            for o in f.objects.iter().filter(|o| map.alive.contains_key(&o.handle.hex()) && !merged.contains_key(&o.handle.hex())) {
                let number = is_proxy(&o.kind).then(|| renumber(class_number(o.tags.iter().map(|r| (r.code, r.value.as_str()))))).flatten();
                match number {
                    Some(n) => {
                        let mut tags: Vec<Tag> = o.tags.iter().map(tag).collect();
                        set_class_number(&mut tags, n);
                        let o = ForeignObject { tags: tags.iter().map(raw).collect(), ..o.clone() };
                        map.object(&o, &mut out);
                    }
                    None => map.object(o, &mut out),
                }
            }
        }
        for (sec, t) in planned.get(&r.start).into_iter().flatten() {
            let (Some(zero), Some(ub)) = (t.first(), t.get(1..)) else { continue };
            let ur = Rec { sec: sec.clone(), kind: zero.str(), start: 0, end: 0 };
            let x = head(ub).handle.and_then(|h| handle(&h)).and_then(|h| entity_extra(f, &xd, h));
            out.push(zero.clone());
            record(&map, &ur, ub, x.as_deref(), None, true, false, &mut out);
        }
        if let Some(tags) = map.replayed_record(r, b, &merged_into) {
            out.extend(tags);
            continue;
        }
        let start = out.len();
        out.extend(tags.get(r.start).cloned());
        let x = extras.of(r, b);
        let geometry = record_geometry(&map, &recs, n, b, &mut family);
        map.write_record(r, b, x.as_deref(), geometry, Some(r.start) == root, &mut out);
        if added > 0
            && let Some(count) = out.get_mut(start..).and_then(|t| t.iter_mut().find(|t| t.code == 70))
        {
            *count = Tag::i(70, count.i64().saturating_add(i64::try_from(added).unwrap_or(0)));
        }
    }
    add_appids(&mut out, std::iter::empty());
    let top = max_handle(&out).saturating_add(1);
    if let Some(t) = seed_index(&out).and_then(|i| out.get_mut(i))
        && Handle::parse_hex(&t.str()).is_none_or(|h| h.0 < top)
    {
        *t = Tag::s(5, format!("{top:X}"));
    }
    remap_header_pointers(&mut out, |h| map.get(h).unwrap_or_else(|| "0".into()));
    remap_table_fields(&mut out, |h| map.get(h).filter(|n| n != "0"));
    *tags = out;
}

/// Where the entities of types CADCraft doesn't model go: each before the next written entity of
/// its space or block (else at the end of it), as `(section, record)` lists by the index of the
/// record they precede. Owners come from the written block records.
fn plan_unknown(d: &Drawing, recs: &[Rec], tags: &[Tag], roles: &HashMap<ForeignKey, String>) -> HashMap<usize, Vec<(String, Vec<Tag>)>> {
    let mut out: HashMap<usize, Vec<(String, Vec<Tag>)>> = HashMap::new();
    let body = |r: &Rec| tags.get(r.start + 1..r.end).unwrap_or(&[]);
    let mut at: HashMap<String, usize> = HashMap::new();
    for r in recs.iter().filter(|r| matches!(r.sec.as_str(), "ENTITIES" | "BLOCKS") && !matches!(r.kind.as_str(), "BLOCK" | "ENDBLK")) {
        if let Some(h) = head(body(r)).handle {
            at.entry(h).or_insert(r.start);
        }
    }
    let ents_end = recs.iter().find(|r| r.sec == "ENTITIES" && r.kind == "ENDSEC").map(|r| r.start);
    let endblk = |name: &str| {
        let i = recs.iter().position(|r| r.sec == "BLOCKS" && r.kind == "BLOCK" && val(body(r), 2).is_some_and(|n| up(&n) == up(name)))?;
        recs.iter().skip(i).find(|r| r.kind == "ENDBLK").map(|r| r.start)
    };
    let br = |name: &str| roles.get(&("BLOCK_RECORD".to_string(), up(name))).cloned();
    let first_at = |s: &cadcraft_doc::EntityStore| s.iter().find_map(|e| at.get(&e.handle.hex()).copied());
    let layouts = d.paper_space_blocks();
    // (entities, owner, paper space, end of the space, section)
    let mut stores: Vec<(&cadcraft_doc::EntityStore, Option<String>, bool, Option<usize>, &str)> = Vec::new();
    let model_end = layouts.first().and_then(|(_, l)| first_at(&l.entities)).or(ents_end);
    stores.push((&d.model, br("*Model_Space"), false, model_end, "ENTITIES"));
    for (i, (name, l)) in layouts.iter().enumerate() {
        if i == 0 {
            stores.push((&l.entities, br(name), true, ents_end, "ENTITIES"));
        } else {
            stores.push((&l.entities, br(name), true, endblk(name), "BLOCKS"));
        }
    }
    for b in d.blocks.values() {
        stores.push((&b.entities, br(&b.name), false, endblk(&b.name), "BLOCKS"));
    }
    for (store, owner, paper, end, sec) in stores {
        let (Some(owner), Some(end)) = (owner, end) else { continue };
        let mut pending: Vec<(String, Vec<Tag>)> = Vec::new();
        for e in store.iter() {
            match &e.kind {
                EntityKind::Unknown(u) => {
                    let t = unknown_entity(u, e, &owner, paper);
                    if !t.is_empty() {
                        pending.push((sec.to_string(), t));
                    }
                }
                _ => {
                    if let Some(&p) = at.get(&e.handle.hex())
                        && !pending.is_empty()
                    {
                        out.entry(p).or_default().append(&mut pending);
                    }
                }
            }
        }
        if !pending.is_empty() {
            out.entry(end).or_default().append(&mut pending);
        }
    }
    out
}

/// The source geometry of record `n` of `recs` (its groups `b`): its own snapshot, or the next VERTEX
/// record of a polyline that has one (`family`: that snapshot and the VERTEX records handed out).
fn record_geometry<'a>(
    map: &'a Map,
    recs: &[Rec],
    n: usize,
    b: &[Tag],
    family: &mut Option<(&'a cadcraft_doc::GeometrySnapshot, usize)>,
) -> Option<&'a cadcraft_doc::GeometryRecord> {
    let kind = recs.get(n)?.kind.as_str();
    if kind == "VERTEX" {
        return family.as_mut().and_then(|(g, i)| {
            *i += 1;
            g.records.get(*i)
        });
    }
    let written = recs.iter().skip(n + 1).take_while(|v| v.kind == "VERTEX").count();
    // A polyline replays its VERTEX records only if they are as many as the source's.
    let g = handle_of_record(b).and_then(|h| map.geometry.get(&h)).filter(|g| g.records.len() == 1 || g.records.len() == written + 1);
    *family = if kind == "POLYLINE" { g.map(|g| (g.as_ref(), 0)) } else { None };
    g.and_then(|g| g.records.first())
}

/// Keep source indexed fallbacks only while their corresponding RGB color is unchanged.
fn patch_color_index(kind: &str, body: &[Tag], source: &(String, i64)) -> Vec<Tag> {
    let start = body.iter().position(|t| t.code == 100 && t.str() == "AcDbEntity").map_or(body.len(), |i| i + 1);
    let end = body.get(start..).unwrap_or(&[]).iter().position(|t| t.code == 100 || t.code >= 1000).map_or(body.len(), |i| start + i);
    let common = if kind == "LAYER" { body } else { body.get(start..end).unwrap_or(&[]) };
    let same_rgb = common.iter().find(|t| t.code == 420).is_some_and(|t| t.str() == source.0);
    body.iter()
        .enumerate()
        .map(|(i, t)| {
            if same_rgb && t.code == 62 && (kind == "LAYER" || (start..end).contains(&i)) {
                let aci = if kind == "LAYER" { if t.i64() < 0 { -source.1.saturating_abs() } else { source.1.saturating_abs() } } else { source.1 };
                Tag::i(62, aci)
            } else {
                t.clone()
            }
        })
        .collect()
}

/// A written record with its kept extras, remapped references (unknown entities) and kept
/// dictionary entries (the named object dictionary).
#[allow(clippy::too_many_arguments)]
fn record(
    map: &Map,
    r: &Rec,
    b: &[Tag],
    x: Option<&ForeignExtra>,
    geometry: Option<&cadcraft_doc::GeometryRecord>,
    unknown: bool,
    root: bool,
    out: &mut Vec<Tag>,
) {
    let patched;
    let b = match x.filter(|x| r.kind == "LAYOUT" && !x.layout.is_empty()) {
        Some(x) => {
            let name = b.iter().skip_while(|t| !(t.code == 100 && t.str() == "AcDbLayout")).find(|t| t.code == 1).map(Tag::str);
            let page = name.as_ref().and_then(|name| map.pages.get(&up(name))).copied();
            patched = patch_layout(b, &x.layout, page, |h| map.get(h));
            patched.as_slice()
        }
        None => b,
    };
    // The source geometry of an entity whose geometry is as it was read.
    let replayed;
    let b = match geometry.and_then(|g| crate::dxf_geometry::restore(&r.kind, b, &g.groups, map.version, |t| map.group(t))) {
        Some(tags) => {
            replayed = tags;
            replayed.as_slice()
        }
        None => b,
    };
    let colors;
    let b = if let Some(source) = x.and_then(|x| x.color_index.as_ref()) {
        colors = patch_color_index(&r.kind, b, source);
        colors.as_slice()
    } else {
        b
    };
    let h = head(b);
    // Header snap/grid edits belong only to the first active VPORT tile.
    let active_snap = r.kind == "VPORT" && h.handle.as_ref().is_some_and(|h| Some(h) == map.roles.get(&("VPORT".into(), "*ACTIVE".into())));
    let flag_mask = symbol_flag_mask(&r.kind);
    let dictionary = is_dictionary(&r.kind);
    let boundary = matches!(r.kind.as_str(), "BLOCK" | "ENDBLK");
    let attributes = (r.kind == "BLOCK").then(|| val(b, 2).and_then(|name| map.block_attributes.get(&up(&name)).copied())).flatten();
    let replaces_gradient = r.kind == "HATCH" && (map.target < cadcraft_dxf::Version::R2004 || b.iter().any(|t| t.code == 450 && t.i64() != 0));
    let replay = |p: &RawTag| {
        let dimension_field_supported = r.kind != "DIMENSION"
            || match p.code {
                73..=75 => map.target >= cadcraft_dxf::Version::R2007,
                280 => map.target >= cadcraft_dxf::Version::R2010,
                _ => true,
            };
        dimension_field_supported
            && (!replaces_gradient || !matches!(p.code, 450..=470 | 63 | 421))
            && (!boundary || p.code != 8)
            && (flag_mask == 0 || p.code != 70)
            && (!dictionary || p.code != 281)
            && (!active_snap || !matches!(p.code, 13 | 23 | 14 | 24 | 15 | 25 | 50))
    };
    let entity = matches!(r.sec.as_str(), "ENTITIES" | "BLOCKS");
    let xdata_at = b.iter().position(|t| t.code == 1001).unwrap_or(b.len());
    let own_apps: HashSet<String> = b.iter().filter(|t| t.code == 1001).map(|t| up(&t.str())).collect();
    // Where unmodelled properties go: the end of AcDbEntity, else before the xdata. The groups of
    // the record's own subclass (a viewport's sun link, a leader's annotation) follow the groups
    // the writer wrote in it, which is the end of the record before its xdata.
    let props_at = if entity {
        b.iter().position(|t| t.code == 100 && t.str().trim() == "AcDbEntity").and_then(|s| {
            let rest = b.get(s + 1..xdata_at)?;
            Some(s + 1 + rest.iter().position(|t| t.code == 100).unwrap_or(rest.len()))
        })
    } else {
        None
    }
    .unwrap_or(xdata_at);
    // Keep presentation fields in their own subclass, before subtype or embedded-object groups.
    let subclass_at = match r.kind.as_str() {
        "MTEXT" => b.iter().position(|t| t.code == 101).unwrap_or(xdata_at),
        "DIMENSION" => b
            .iter()
            .position(|t| t.code == 100 && t.str() == "AcDbDimension")
            .map_or(xdata_at, |start| start + 1 + b.iter().skip(start + 1).take_while(|t| t.code != 100 && t.code < 1000).count()),
        _ => xdata_at,
    };
    let rgb = b.iter().take(xdata_at).find(|t| t.code == 420).map(Tag::str);
    let has_book = b.iter().take(xdata_at).any(|t| t.code == 430);
    let handle_at = b.iter().position(|t| matches!(t.code, 5 | 105));
    let mut seen_owner = false;
    let mut listed: Option<HashSet<String>> = None;
    for (i, t) in b.iter().enumerate() {
        if i == props_at
            && let Some(x) = x
        {
            if let Some((want, book)) = &x.color_book
                && !has_book
                && rgb.as_deref() == Some(want.as_str())
            {
                out.push(Tag::s(430, book.clone()));
            }
            out.extend(x.props.iter().filter(|p| replay(p)).map(|p| map.group(p)));
        }
        if i == subclass_at
            && let Some(x) = x
        {
            out.extend(x.subclass.iter().filter(|p| replay(p)).map(|p| map.group(p)));
        }
        if i == xdata_at
            && let Some(x) = x
        {
            xdata(map, x, &own_apps, out);
        }
        // Kept fields replace the defaults the writer wrote.
        if i < xdata_at && replaced_by_kept(&r.kind, entity, t, x, &replay) {
            continue;
        }
        let mut t = source_record_group(t, x, &r.kind, attributes);
        if i < xdata_at {
            add_kept_reactors(&t, x, map, &mut listed, out);
        }
        if unknown && i < xdata_at && is_pointer(t.code) && t.code != 102 {
            if t.code == 330 && !seen_owner {
                seen_owner = true;
            } else {
                t = Tag::s(t.code, map.get(&t.str()).unwrap_or_else(|| "0".into()));
            }
        }
        out.push(t);
        if Some(i) == handle_at
            && let Some(x) = x
        {
            if !h.has_reactors {
                let rs: Vec<String> = x.reactors.iter().map(|r| r.hex()).collect();
                map.handle_group(out, "ACAD_REACTORS", 330, &rs);
            }
            if h.xdict.is_none()
                && let Some(xd) = x.xdict
                && h.handle.as_ref().is_some_and(|owner| map.owns_xdict(&xd.hex(), owner))
            {
                map.handle_group(out, "ACAD_XDICTIONARY", 360, &[xd.hex()]);
            }
        }
    }
    if let Some(x) = x {
        if props_at == b.len() {
            if let Some((want, book)) = &x.color_book
                && !has_book
                && rgb.as_deref() == Some(want.as_str())
            {
                out.push(Tag::s(430, book.clone()));
            }
            out.extend(x.props.iter().filter(|p| replay(p)).map(|p| map.group(p)));
        }
        if subclass_at == b.len() {
            out.extend(x.subclass.iter().filter(|p| replay(p)).map(|p| map.group(p)));
        }
        if xdata_at == b.len() {
            xdata(map, x, &own_apps, out);
        }
    }
    if root {
        for (key, h) in &map.f.dictionary {
            if let Some(nh) = map.alive.get(&h.hex())
                && !b.iter().any(|t| t.code == 3 && up(&t.str()) == up(key))
            {
                out.push(Tag::s(3, key.clone()));
                out.push(Tag::s(350, nh.clone()));
            }
        }
    }
}

/// Header variables that point at an object ($CMATERIAL, $INTERFEREOBJVS, $CEPSNID 390…) were written with
/// the handle read (see `header::preserved_header_vars`): point them at the saved object, or at
/// the null handle when it is gone (`saved`). The writer generates no pointer-valued header
/// variable.
/// The FIELD handle (344) of each table cell, as the written FIELD object has it; a cell whose FIELD is not written shows its
/// cached text.
fn remap_table_fields(tags: &mut Vec<Tag>, saved: impl Fn(&str) -> Option<String>) {
    let mut table = false;
    let mut out = Vec::with_capacity(tags.len());
    for t in tags.drain(..) {
        if t.code == 0 {
            table = t.str().trim().eq_ignore_ascii_case("ACAD_TABLE");
        } else if table && t.code == 344 {
            if let Some(new) = saved(&norm(&t.str())) {
                out.push(Tag::s(344, new));
            }
            continue;
        }
        out.push(t);
    }
    *tags = out;
}

fn remap_header_pointers(out: &mut [Tag], saved: impl Fn(&str) -> String) {
    let Some(start) = out.windows(2).position(|w| w[0].code == 0 && w[1].code == 2 && w[1].str() == "HEADER") else { return };
    let mut variable = String::new();
    for t in out.iter_mut().skip(start + 2).take_while(|t| t.code != 0) {
        if t.code == 9 {
            variable = t.str();
        // $CEPSNID's 390 names a placeholder the writer wrote in this file: already saved.
        } else if cadcraft_doc::foreign::is_pointer(t.code) && !variable.eq_ignore_ascii_case("$CEPSNID") {
            *t = Tag::s(t.code, saved(&t.str()));
        }
    }
}

/// Kept xdata of applications the record doesn't already carry, handles remapped.
fn xdata(map: &Map, x: &ForeignExtra, own: &HashSet<String>, out: &mut Vec<Tag>) {
    let mut skip = false;
    let empty_record = |i: usize| {
        let marker = |at: usize, name: &str| x.xdata.get(at).is_some_and(|r| r.code == 1000 && r.value.trim() == name);
        (marker(i, "ACAD_MTEXT_COLUMN_INFO_BEGIN") && marker(i + 1, "ACAD_MTEXT_COLUMN_INFO_END")).then_some(i)
    };
    for (i, r) in x.xdata.iter().enumerate() {
        if r.code == 1001 {
            skip = own.contains(&up(&r.value));
        }
        // The place the model's column record takes in an MTEXT's ACAD data (see `foreign_xdata`) is not data.
        let place = empty_record(i).is_some() || i.checked_sub(1).and_then(empty_record).is_some();
        if !skip && !place {
            out.push(map.group(r));
        }
    }
}

/// APPID records for every application with xdata in the file, added to the APPID table.
fn add_appids(out: &mut Vec<Tag>, extra: impl IntoIterator<Item = String>) {
    let mut used: BTreeMap<String, String> = out.iter().filter(|t| t.code == 1001).map(|t| (up(&t.str()), t.str().trim().to_string())).collect();
    for name in extra {
        let name = name.trim();
        if !name.is_empty() {
            used.entry(up(name)).or_insert_with(|| name.to_string());
        }
    }
    let recs = written_records(out);
    let Some(table) = recs.iter().position(|r| {
        r.sec == "TABLES" && r.kind == "TABLE" && val(out.get(r.start + 1..r.end).unwrap_or(&[]), 2).is_some_and(|n| up(&n) == "APPID")
    }) else {
        return;
    };
    let Some(t) = recs.get(table) else { return };
    let table_h = head(out.get(t.start + 1..t.end).unwrap_or(&[])).handle.unwrap_or_else(|| "0".into());
    let Some(end) = recs.iter().skip(table).find(|r| r.kind == "ENDTAB") else { return };
    let have: HashSet<String> = recs
        .iter()
        .skip(table + 1)
        .take_while(|r| r.kind == "APPID")
        .filter_map(|r| val(out.get(r.start + 1..r.end).unwrap_or(&[]), 2))
        .map(|n| up(&n))
        .collect();
    let missing: Vec<&String> = used.iter().filter(|(k, _)| !have.contains(*k)).map(|(_, v)| v).collect();
    if missing.is_empty() {
        return;
    }
    // New handles above the seed, which is then moved past them.
    let mut next = seed(out).max(max_handle(out).saturating_add(1));
    let mut add = Vec::new();
    for app in &missing {
        add.extend([
            Tag::s(0, "APPID"),
            Tag::s(5, format!("{next:X}")),
            Tag::s(330, table_h.clone()),
            Tag::s(100, "AcDbSymbolTableRecord"),
            Tag::s(100, "AcDbRegAppTableRecord"),
            Tag::s(2, (*app).clone()),
            Tag::i(70, 0),
        ]);
        next = next.saturating_add(1);
    }
    let end_at = end.start;
    if let Some(count) = out.get_mut(t.start + 1..t.end).and_then(|b| b.iter_mut().find(|x| x.code == 70)) {
        *count = Tag::i(70, count.i64().saturating_add(i64::try_from(missing.len()).unwrap_or(0)));
    }
    out.splice(end_at..end_at, add);
}

/// The index of the `$HANDSEED` value in the header.
fn seed_index(tags: &[Tag]) -> Option<usize> {
    tags.windows(2).position(|w| matches!(w, [a, b] if a.code == 9 && a.str() == "$HANDSEED" && b.code == 5)).map(|i| i + 1)
}

/// The header's next free handle.
fn seed(tags: &[Tag]) -> u64 {
    seed_index(tags).and_then(|i| tags.get(i)).and_then(|t| Handle::parse_hex(&t.str())).map_or(0, |h| h.0)
}

/// The largest handle of a record (groups 5 and 105).
fn max_handle(tags: &[Tag]) -> u64 {
    tags.iter().filter(|t| t.code == 5 || t.code == 105).filter_map(|t| Handle::parse_hex(&t.str())).map(|h| h.0).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_constraint_group_form_is_told_by_its_groups_not_by_the_release() {
        let record = |tags: &[(i32, &str)]| tags.iter().map(|(c, v)| Tag::s(*c, *v)).collect::<Vec<_>>();
        // AutoCAD below 2013: each node's class name is followed by its owner.
        let inline = record(&[(0, "ACDBASSOC2DCONSTRAINTGROUP"), (90, "40"), (90, "1"), (1, "AcFixedConstraint"), (330, "5"), (90, "1")]);
        // The DWG bridge, and AutoCAD from 2013: the class names are listed with their ids.
        let registry = record(&[(0, "ACDBASSOC2DCONSTRAINTGROUP"), (90, "40"), (90, "1"), (1, "AcFixedConstraint"), (90, "1"), (70, "0")]);
        assert_eq!((constraint_group_form(&inline), constraint_group_form(&registry)), (0, 1));
        assert_eq!((release_form("ACDBASSOC2DCONSTRAINTGROUP", 1024), release_form("ACDBASSOC2DCONSTRAINTGROUP", 1027)), (Some(0), Some(1)));
        assert_eq!(release_form("LINE", 1032), None);
    }
}

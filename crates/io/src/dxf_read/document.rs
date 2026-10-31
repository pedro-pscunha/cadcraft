use super::common::T;
use super::entities::{entity_plot_style, entity_with_table_budget, insert_attribs, polyline_entity};
use super::extras::{collect_source_block_references, entity_extras};
use super::objects::{MAX_OBJECTS, Objects, SortentsTable, page_setup};
use super::{BTreeMap, Block, BlockDef, Drawing, Entity, EntityKind, Handle, HashMap, Layout, LayoutDef, Rx, Tag, records};
use crate::activation::records::Container;

pub(super) fn parse_entities(
    recs: &[(String, Vec<Tag>)],
    container: &Container,
    d: &mut Drawing,
    rx: &mut Rx,
    skipped: &mut usize,
    table_cells_left: &mut usize,
) -> crate::Result<Vec<(Option<String>, bool, Entity)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((kind, tags)) = recs.get(i) {
        let t = T(tags);
        let owner = crate::dxf_ext::owner(tags);
        let paper = t.i(67) == Some(1);
        // The place of a top-level record, counted as the activation adapter counts it (followers are not records of their own).
        let place = (!matches!(kind.as_str(), "ATTRIB" | "VERTEX" | "SEQEND")).then(|| rx.ordinals.next(container));
        let raw_reason = place.and_then(|place| rx.raw_reason(container, place));
        collect_source_block_references(kind, tags, raw_reason.is_some(), rx);
        if let Some(reason) = raw_reason {
            let end = unit_end(recs, i);
            for (kind, tags) in recs.iter().take(end).skip(i + 1) {
                collect_source_block_references(kind, tags, true, rx);
            }
            match raw_unit(&recs[i..end], &reason, d, rx) {
                Ok(unit) => {
                    d.bump_handseed(unit.handle);
                    out.push((owner, paper, unit));
                    i = end;
                    continue;
                }
                // Plan v3, 4.1: no Legacy entity is built for a needs node. The read fails closed and names the record.
                Err(unraw) => {
                    return Err(crate::IoError::NotKeptRaw {
                        record: kind.clone(),
                        handle: t.s(5).and_then(|h| Handle::parse_hex(&h)),
                        reason: unraw,
                    });
                }
            }
        }
        if kind == "VIEWPORT"
            && crate::dxf_ext::is_paper_view(tags)
            && let Some(block) = owner.as_ref().and_then(|o| rx.brs.get(&o.trim().to_ascii_uppercase()))
        {
            rx.marked_sheets.insert(block.to_ascii_uppercase());
        }
        let handle = t.s(5).and_then(|h| Handle::parse_hex(&h));
        let mut kindent = entity_with_table_budget(kind, tags, table_cells_left);
        if let Some((c, _)) = kindent.as_mut()
            && let Some(n) = entity_plot_style(tags, rx)
        {
            c.plot_style = n;
        }
        i += 1;
        match kind.as_str() {
            "POLYLINE" => kindent = Some(polyline_entity(recs, &mut i, &t, d, rx)),
            "INSERT" if t.i(66) == Some(1) => {
                let attribs = insert_attribs(recs, &mut i, rx);
                if let Some((_, EntityKind::Insert(ins))) = kindent.as_mut() {
                    ins.attribs = attribs;
                }
            }
            // A VERTEX or ATTRIB with no POLYLINE or INSERT before it; a stray SEQEND holds nothing.
            "VERTEX" | "ATTRIB" => {
                *skipped = skipped.saturating_add(1);
                continue;
            }
            "SEQEND" => continue,
            _ => {}
        }
        if let Some((c, mut k)) = kindent {
            let h = match handle {
                Some(h) => {
                    d.bump_handseed(h);
                    h
                }
                None => d.new_handle(),
            };
            let geometry_revision = cadcraft_doc::GeometryRevision::LegacyProjectedV1;
            entity_extras(kind, tags, h, &mut k, rx, geometry_revision);
            out.push((owner, paper, Entity { geometry_revision, handle: h, common: c, kind: k }));
        } else if kind == "HATCH" {
            // Without boundary data. The other records `entity` returns nothing for are read
            // elsewhere (the writer's paper viewport, the loops of a region).
            *skipped = skipped.saturating_add(1);
        }
    }
    // Assign fresh handles to duplicates (hostile files).
    let mut seen = std::collections::HashSet::new();
    for (_, _, e) in &mut out {
        // A raw unit keeps the identity of its source records.
        if !seen.insert(e.handle) && !matches!(e.kind, EntityKind::Raw(_)) {
            e.handle = d.new_handle();
            seen.insert(e.handle);
        }
    }
    Ok(out)
}

/// Read the BLOCKS section: the xref paths of external blocks, then each block's entities.
pub(super) fn read_blocks(
    tags: &[Tag],
    d: &mut Drawing,
    rx: &mut Rx,
    table_cells_left: &mut usize,
    skipped: &mut std::collections::BTreeMap<String, usize>,
) -> crate::Result<Vec<BlockDef>> {
    let recs = records(tags);
    for (k, tg) in &recs {
        let t = T(tg);
        if k == "BLOCK" && t.i(70).unwrap_or(0) & 4 != 0 {
            rx.xref_paths.insert(t.s(2).unwrap_or_default().to_ascii_uppercase(), t.s(1).unwrap_or_default());
        }
    }
    let mut blocks = Vec::new();
    let mut n = 0;
    let mut i = 0;
    while let Some((k, tg)) = recs.get(i) {
        if k == "BLOCK" {
            let t = T(tg);
            let name = t.s(2).unwrap_or_default();
            let base = t.p(10);
            let description = t.s(4).unwrap_or_default();
            // Block flags: 4 xref, 8 overlay.
            let xref = t.i(70).unwrap_or(0) & 12;
            let mut j = i + 1;
            while recs.get(j).is_some_and(|(k, _)| k != "ENDBLK") {
                j += 1;
            }
            let body = recs.get(i + 1..j).unwrap_or(&[]);
            let ents = parse_entities(body, &Container::block(&name), d, rx, &mut n, table_cells_left)?;
            blocks.push((name, base, description, ents, xref));
            i = j + 1;
        } else {
            // A record outside any BLOCK ... ENDBLK pair.
            if k != "ENDBLK" {
                n = n.saturating_add(1);
            }
            i += 1;
        }
    }
    count_skipped(skipped, "BLOCKS", n);
    Ok(blocks)
}

/// Read the OBJECTS section into `objs`, and each LAYOUT into `layout_objs`.
pub(super) fn read_objects(tags: &[Tag], objs: &mut Objects, layout_objs: &mut Vec<LayoutDef>) {
    for (k, tg) in records(tags) {
        objs.add(&k, &tg);
        if k == "LAYOUT" {
            // Fields after the AcDbLayout marker (AcDbPlotSettings also has a code 1).
            let start = tg.iter().position(|x| x.code == 100 && x.str() == "AcDbLayout").unwrap_or(0);
            let tail = tg.get(start..).unwrap_or(&[]);
            let t = T(tail);
            // LAYOUT has two 330s: owner dict and the block record; take the last.
            let br = tg.iter().rfind(|x| x.code == 330).map(Tag::str).unwrap_or_default();
            let plot = T(tg.get(..start).unwrap_or(&[]));
            layout_objs.push((t.s(1).unwrap_or_default(), t.i(71).unwrap_or(0) as u32, br.to_ascii_uppercase(), page_setup(&plot)));
        }
    }
}

/// Add the drawing's layouts, and return the layout each paper-space block belongs to, and the
/// layout that takes paper-space entities without an owner.
pub(super) fn setup_layouts(d: &mut Drawing, layout_objs: &mut [LayoutDef], rx: &Rx) -> (HashMap<String, String>, String) {
    let mut paper_block_for: HashMap<String, String> = HashMap::new();
    layout_objs.sort_by_key(|l| l.1);
    for (name, order, br, page) in layout_objs.iter() {
        if name.eq_ignore_ascii_case("Model") {
            continue;
        }
        d.layouts.push(Layout { page: page.clone(), ..Layout::new(name, *order) });
        if let Some(bname) = rx.brs.get(br) {
            paper_block_for.insert(bname.to_ascii_uppercase(), name.clone());
            std::sync::Arc::make_mut(&mut d.source_symbols).layout_blocks.insert(name.to_ascii_uppercase(), bname.clone());
        }
    }
    if d.layouts.is_empty() {
        d.layouts.push(Layout::new("Layout1", 1));
        paper_block_for.insert("*PAPER_SPACE".into(), "Layout1".into());
    }
    // The layout in `*Paper_Space` is the active one, kept when it isn't the first tab (the first
    // is active by default); paper-space entities without an owner go there.
    let active = paper_block_for.get("*PAPER_SPACE").cloned();
    let first_tab = d.layouts.iter().min_by_key(|l| l.tab_order).map(|l| l.name.clone());
    d.active_layout = active.clone().filter(|a| Some(a) != first_tab.as_ref());
    let first_layout = active.or_else(|| d.layouts.first().map(|l| l.name.clone())).unwrap_or_else(|| "Layout1".into());
    (paper_block_for, first_layout)
}

/// Place the BLOCKS entities: model and paper space blocks go to their space, the rest become
/// block definitions.
pub(super) fn place_blocks(d: &mut Drawing, blocks: Vec<BlockDef>, rx: &mut Rx, paper_block_for: &HashMap<String, String>, first_layout: &str) {
    let source = std::sync::Arc::make_mut(&mut d.source_symbols);
    for (place, (name, ..)) in blocks.iter().enumerate() {
        source.block_order.entry(name.to_ascii_uppercase()).or_insert(place);
    }
    for (name, base, description, ents, xref) in blocks {
        let up = name.to_ascii_uppercase();
        if up == "*MODEL_SPACE" {
            for (_, _, e) in ents {
                d.ensure_layer(&e.common.layer);
                d.model.push(e);
            }
            continue;
        }
        if up.starts_with("*PAPER_SPACE") {
            let lname = paper_block_for.get(&up).cloned().unwrap_or_else(|| first_layout.to_string());
            if let Some(l) = d.layouts.iter_mut().find(|l| l.name == lname) {
                for (_, _, e) in ents {
                    l.entities.push(e);
                }
            }
            continue;
        }
        // Generated geometry we rebuild on save: table blocks, and standard arrowhead blocks
        // nothing inserts or names in xdata whose content is the one CADCraft writes. An
        // arrowhead block with other content (AutoCAD's `_Dot`, a user's redefinition) is kept, so
        // dimensions keep it.
        let generated_arrow = || {
            let own: Vec<&Entity> = ents.iter().map(|(_, _, e)| e).collect();
            crate::dxf_ext::is_standard_arrow(&name)
                && !rx.inserted.contains(&up)
                && !rx.xdata_blocks.contains(&up)
                && crate::dxf_write::is_generated_arrow(&name, &own)
        };
        if rx.table_blocks.contains_key(&up) {
            // The block a table drew itself with: kept with the table, which writes it as read until it is edited.
            let mut block = Block::new(&name);
            block.base = base;
            block.description = description;
            for (_, paper, mut e) in ents {
                e.common.paper_space = paper;
                block.entities.push(e);
            }
            rx.table_graphics.insert(up, std::sync::Arc::new(block));
            continue;
        }
        if generated_arrow() {
            continue;
        }
        let mut b = Block::new(&name);
        b.base = base;
        b.description = description;
        if let Some(&(units, explodable, scale_uniformly)) = rx.block_props.get(&up) {
            (b.units, b.explodable, b.scale_uniformly) = (units, explodable, scale_uniformly);
        }
        if xref & 4 != 0 {
            b.xref_path = Some(String::new());
            b.xref_overlay = xref & 8 != 0;
        } else {
            for (_, paper, mut e) in ents {
                e.common.paper_space = paper;
                b.entities.push(e);
            }
        }
        // Array parameters, with the block names the items use (other programs may rename
        // anonymous blocks); dropped when the items no longer match them.
        b.array = rx.block_arrays.get(&up).cloned().and_then(|mut a| a.sync_names(b.entities.iter().map(|e| e.as_ref())).then_some(a));
        if let Some(path) = rx.xref_paths.remove(&up) {
            b.xref_path = Some(path);
        }
        d.blocks.insert(name, std::sync::Arc::new(b));
    }
}

/// Place the ENTITIES section: model space, or paper space when flagged (67=1).
pub(super) fn place_entities(
    d: &mut Drawing,
    entities: Vec<(Option<String>, bool, Entity)>,
    rx: &Rx,
    paper_block_for: &HashMap<String, String>,
    first_layout: &str,
) {
    for (owner, paper, e) in entities {
        d.ensure_layer(&e.common.layer);
        let owner_name = owner.and_then(|o| rx.brs.get(&o.trim().to_ascii_uppercase()).cloned()).map(|n| n.to_ascii_uppercase());
        let to_paper = paper || owner_name.as_deref().is_some_and(|n| n.starts_with("*PAPER_SPACE"));
        if to_paper {
            let lname = owner_name.and_then(|n| paper_block_for.get(&n).cloned()).unwrap_or_else(|| first_layout.to_string());
            if let Some(l) = d.layouts.iter_mut().find(|l| l.name == lname) {
                l.entities.push(e);
            }
        } else {
            d.model.push(e);
        }
    }
}

pub(super) fn count_skipped(skipped: &mut std::collections::BTreeMap<String, usize>, section: &str, n: usize) {
    if n > 0 {
        let c = skipped.entry(section.to_string()).or_insert(0);
        *c = c.saturating_add(n);
    }
}

/// Records between sections, which [`sections`] passes over: everything after an ENDSEC up to the
/// next SECTION. Stray ENDSECs and the EOF hold nothing and don't count.
pub(super) fn outside_sections(tags: &[Tag]) -> usize {
    let mut inside = false;
    let mut n = 0usize;
    for t in tags.iter().filter(|t| t.code == 0) {
        match t.str().as_str() {
            "SECTION" => inside = true,
            "ENDSEC" => inside = false,
            "EOF" => {}
            _ if !inside => n = n.saturating_add(1),
            _ => {}
        }
    }
    n
}

/// A SORTENTSTABLE (DXF Reference, OBJECTS section): after its AcDbSortentsTable marker, the
/// owning block record (330), then pairs of an entity handle (331) and the sort handle (5) it is
/// drawn by.
pub(super) fn sortents_table(tags: &[Tag]) -> Option<SortentsTable> {
    let start = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbSortentsTable")?;
    let tail = tags.get(start + 1..)?;
    let block = Handle::parse_hex(&tail.iter().find(|t| t.code == 330)?.str())?;
    let handle = Handle::parse_hex(&tags.iter().find(|t| t.code == 5)?.str())?;
    let dictionary = Handle::parse_hex(&tags.get(..start)?.iter().find(|t| t.code == 330)?.str())?;
    let mut entries = Vec::new();
    let mut ent: Option<Handle> = None;
    for t in tail {
        match t.code {
            331 => ent = Handle::parse_hex(&t.str()),
            5 => {
                if let (Some(e), Some(k)) = (ent.take(), Handle::parse_hex(&t.str()))
                    && entries.len() < MAX_OBJECTS
                {
                    entries.push((e, k.0));
                }
            }
            _ => {}
        }
    }
    Some(SortentsTable { handle, dictionary, block, entries })
}

/// Draw each space with a SORTENTSTABLE in sort-handle order: an entity listed in the table by
/// its sort handle, any other by its own handle.
/// Returns the table of each space, for the save to write back ([`cadcraft_doc::foreign::Foreign::sort_tables`]).
pub(super) fn apply_sortents(
    d: &mut Drawing,
    objects: &Objects,
    rx: &Rx,
    paper_for: &HashMap<String, String>,
    first_layout: &str,
) -> BTreeMap<cadcraft_doc::foreign::ForeignKey, cadcraft_doc::foreign::SortTable> {
    let mut tables = BTreeMap::new();
    // Record order is not draw order. Unlisted entities (including a whole space with no
    // table) draw by their own handles; establish that default before explicit overrides.
    d.model.sort_by_key(|e| e.handle);
    for layout in &mut d.layouts {
        layout.entities.sort_by_key(|e| e.handle);
    }
    for block in d.blocks.values_mut() {
        std::sync::Arc::make_mut(block).entities.sort_by_key(|e| e.handle);
    }
    let mut notes = Vec::new();
    for linked in objects.sortents(rx) {
        let Some(name) = rx.brs.get(&linked.block.hex()) else { continue };
        let up = name.to_ascii_uppercase();
        let (key, store) = if up == "*MODEL_SPACE" {
            (("MODEL".to_string(), String::new()), &mut d.model)
        } else if up.starts_with("*PAPER_SPACE") {
            let lname = paper_for.get(&up).map(String::as_str).unwrap_or(first_layout);
            let Some(layout) = d.layouts.iter_mut().find(|l| l.name == lname) else { continue };
            (("LAYOUT".to_string(), layout.name.clone()), &mut layout.entities)
        } else {
            let Some(block) = d.blocks.get_mut(name.as_str()) else { continue };
            (("BLOCK".to_string(), up), &mut std::sync::Arc::make_mut(block).entities)
        };
        let members = store.iter().map(|entity| entity.handle).collect();
        let table = objects.recover_sortents(linked, &members);
        let keys = table.keys(&members);
        let entries = table.valid_entries(&members).into_iter().collect();
        tables.insert(key, cadcraft_doc::foreign::SortTable { entries, place: objects.sortents_place(linked) });
        store.sort_by_key(|entity| keys.get(&entity.handle).copied().unwrap_or(entity.handle.0));
        if linked.handle != table.handle || keys.len() != table.entries.len() {
            notes.push(format!(
                "SORTENTSTABLE {} for block {} repaired using {}: {} dead/duplicate entries dropped",
                linked.handle.hex(),
                name,
                table.handle.hex(),
                table.entries.len().saturating_sub(keys.len()),
            ));
        }
    }
    d.read_notes.extend(notes);
    tables
}

/// The end of the unit that starts at `recs[start]`: the record and the followers the activation adapter gives it (the ATTRIBs of an
/// INSERT, the VERTEX records of a POLYLINE, and their closing SEQEND).
fn unit_end(recs: &[(String, Vec<Tag>)], start: usize) -> usize {
    let follower = match recs[start].0.as_str() {
        "INSERT" => "ATTRIB",
        "POLYLINE" => "VERTEX",
        _ => return start + 1,
    };
    let mut end = start + 1;
    while recs.get(end).is_some_and(|(kind, _)| kind == follower) {
        end += 1;
    }
    if recs.get(end).is_some_and(|(kind, _)| kind == "SEQEND") {
        end += 1;
    }
    end
}

/// The record as raw tags, with an identity if the file gave it none (an old DXF has no handles).
fn raw_record(kind: &str, tags: &[Tag], d: &mut Drawing) -> Option<cadcraft_doc::RawRecord> {
    let mut raw: Vec<cadcraft_doc::RawTag> = tags.iter().map(|t| cadcraft_doc::RawTag { code: t.code, value: t.str() }).collect();
    if !raw.iter().any(|t| t.code == 5) {
        raw.insert(0, cadcraft_doc::RawTag { code: 5, value: d.new_handle().hex() });
    }
    cadcraft_doc::RawRecord::new(kind.to_string(), raw).ok()
}

/// The records of one unit as the raw entity that keeps them verbatim and read-only; the reason as an `Err` when the records
/// cannot be kept raw (no closing SEQEND, a repeated identity): the caller then refuses the read (plan v3, 4.1: no Legacy entity is built for a needs node).
fn raw_unit(unit: &[(String, Vec<Tag>)], reason: &str, d: &mut Drawing, rx: &Rx) -> Result<Entity, String> {
    let (kind, tags) = unit.first().ok_or_else(|| reason.to_string())?;
    let parent = raw_record(kind, tags, d);
    let followers: Option<Vec<_>> = unit[1..].iter().map(|(kind, tags)| raw_record(kind, tags, d)).collect();
    let mut source = super::common::common(&T(tags));
    if let Some(style) = entity_plot_style(tags, rx) {
        source.plot_style = style;
    }
    let compound =
        parent.zip(followers).and_then(|(parent, followers)| cadcraft_doc::RawCompound::new(parent, followers, source, reason.to_string()).ok());
    match compound {
        Some(compound) => {
            let handle = compound.parent().handle();
            d.read_notes.push(format!("{kind} {} is kept as read and cannot be edited: {reason}", handle.hex()));
            Ok(Entity::from_raw(compound))
        }
        None => Err(format!("its records have no usable identity or closing SEQEND: {reason}")),
    }
}

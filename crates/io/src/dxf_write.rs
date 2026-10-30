//! Drawing → DXF (AC1015 / R2000 structure, readable by every DXF consumer since 2000). Later
//! versions keep that structure (the groups they add are optional, DXF Reference) and differ in
//! `$ACADVER`, from 2004 in the newer DIMSTYLE groups, and from 2007 in UTF-8 text (the caller
//! picks the text encoding) and MULTILEADER entities (older files get LEADER + MTEXT).

use std::collections::{HashMap, HashSet};

use cadcraft_color::Color;
use cadcraft_doc::{
    AssocSnap, Attrib, Block, Common, DimAssoc, DimKind, DimStyle, Dimension, Drawing, Entity, EntityKind, EntityStore, Gradient, Group, HAlign,
    HVal, Handle, Hatch, Insert, Layout, Leader, Line, Lineweight, LwPolyline, MLeader, MLeaderStyle, MText, PageSetup, Point, Solid, Table,
    TableStyle, Text, Transparency, VAlign, VportConfig, VportTile,
};
use cadcraft_dxf::{Tag, Version};
use cadcraft_geom::{Bounds2, Vec2, Vec3};

use crate::dxf_ext::{self, DimVal, K};

mod assoc;
mod blocks;
mod common;
mod ctx;
mod entity;
mod header;
mod layout;
mod objects;
mod raw;
mod sections;
mod sortents;
#[cfg(test)]
mod tests;

pub(crate) use assoc::dimassoc_reproducible;
#[cfg(test)]
pub(crate) use assoc::polyline_vertex_ref;
pub(crate) use blocks::is_generated_arrow;
#[cfg(test)]
pub(crate) use blocks::table_block_entities;
use ctx::Ctx;
use entity::entity;
pub(crate) use raw::remap_raw_references;
use sections::tags_for;

#[derive(Default)]
struct W {
    t: Vec<Tag>,
    dwg_bridge: bool,
    next: u64,
    first: u64,
    used: HashSet<Handle>,
    /// Source entity → its emitted top-level records (not ATTRIB/VERTEX/SEQEND children).
    emitted: HashMap<Handle, Vec<Handle>>,
    /// Upper-case plot style name → its `ACDBPLACEHOLDER` handle (group 390 of layers and objects).
    pstyles: HashMap<String, String>,
    /// Upper-case layer name → its LAYER table record handle (group 331 of viewports).
    layers: HashMap<String, String>,
    /// Persistent reactors of boundary objects: each source object of an associative hatch →
    /// the hatches (groups `{ACAD_REACTORS 330 …}`), from [`hatch_reactors`].
    reactors: HashMap<Handle, Vec<String>>,
}

impl W {
    fn new(d: &Drawing) -> Self {
        let first = d.handseed.max(0x100).checked_add(0x1000).unwrap_or(0x100);
        let used: HashSet<_> = std::iter::once(&d.model)
            .chain(d.layouts.iter().map(|l| &l.entities))
            .chain(d.blocks.values().map(|b| &b.entities))
            .flat_map(|s| s.iter())
            .flat_map(|e| {
                let followers = if let EntityKind::Raw(raw) = &e.kind { Some(raw.followers()) } else { None };
                std::iter::once(e.handle).chain(followers.into_iter().flatten().map(|record| record.handle()))
            })
            .collect();
        #[cfg(test)]
        tests::SCANNED.with(|count| count.set(count.get().saturating_add(used.len())));
        Self {
            t: Vec::new(),
            dwg_bridge: false,
            next: first,
            first,
            used,
            emitted: HashMap::new(),
            pstyles: HashMap::new(),
            layers: HashMap::new(),
            reactors: HashMap::new(),
        }
    }

    fn instance_handle(&mut self, wanted: Option<Handle>) -> Handle {
        if let Some(h) = wanted.filter(|h| h.0 != 0 && h.0 < self.first)
            && self.used.insert(h)
        {
            return h;
        }
        self.allocate_handle()
    }

    fn s(&mut self, c: i32, v: impl Into<String>) {
        self.t.push(Tag::s(c, v));
    }
    fn f(&mut self, c: i32, v: f64) {
        self.t.push(Tag::f(c, v));
    }
    fn i(&mut self, c: i32, v: i64) {
        self.t.push(Tag::i(c, v));
    }
    fn p(&mut self, c: i32, p: Vec3) {
        self.f(c, p.x);
        self.f(c + 10, p.y);
        self.f(c + 20, p.z);
    }
    fn p2(&mut self, c: i32, p: Vec2) {
        self.f(c, p.x);
        self.f(c + 10, p.y);
    }
    fn h(&mut self) -> String {
        self.allocate_handle().hex()
    }
    fn allocate_handle(&mut self) -> Handle {
        while self.used.contains(&Handle(self.next)) {
            self.next = self.next.checked_add(1).unwrap_or(1);
        }
        let h = Handle(self.next);
        self.used.insert(h);
        self.next = self.next.checked_add(1).unwrap_or(1);
        h
    }
    /// A `102 {NAME` … `102 }` group of handles.
    fn group(&mut self, name: &str, code: i32, handles: &[&str]) {
        self.s(102, format!("{{{name}"));
        for h in handles {
            self.s(code, *h);
        }
        self.s(102, "}");
    }
    /// A dimension variable as a group (DIMSTYLE records) or as an xdata value (DSTYLE).
    fn dimval(&mut self, code: i32, v: DimVal, xdata: bool) {
        match v {
            DimVal::Real(x) => self.f(if xdata { 1040 } else { code }, x),
            DimVal::Int(i) => self.i(
                if !xdata {
                    code
                } else if i16::try_from(i).is_ok() {
                    1070
                } else {
                    1071
                },
                i,
            ),
            DimVal::Str(t) => {
                if xdata || !t.is_empty() {
                    self.s(if xdata { 1000 } else { code }, t);
                }
            }
            DimVal::Handle(h) => {
                if xdata {
                    self.s(1005, h);
                } else if h != "0" {
                    self.s(code, h);
                }
            }
        }
    }
    fn xdata(&mut self, tags: Vec<Tag>) {
        self.t.extend(tags);
    }
}

/// Write a drawing as ASCII DXF.
/// The drawing as R2000 ASCII DXF.
pub fn write(d: &Drawing) -> String {
    cadcraft_dxf::write_ascii(&tags(d, Version::R2000))
}

/// The drawing's DXF tags, declaring `version` in `$ACADVER`.
pub fn tags(d: &Drawing, version: Version) -> Vec<Tag> {
    tags_for(d, version, version, false)
}

struct SaveExtents {
    min: Option<Vec3>,
    max: Option<Vec3>,
    refusal: Option<(Handle, cadcraft_doc::DocError)>,
}

fn save_extents(d: &Drawing) -> SaveExtents {
    let mut result = SaveExtents { min: d.header.point("EXTMIN"), max: d.header.point("EXTMAX"), refusal: None };
    let saved = match (result.min, result.max) {
        (Some(_), Some(_)) => true,
        (None, None) => false,
        _ => return result,
    };
    let mut bounds = Bounds2::EMPTY;
    for entity in d.model.iter() {
        if saved
            && let Some(source) = &entity.common.geometry
            && cadcraft_doc::geometry_key(&entity.kind, &entity.common).as_deref() == Some(source.model.as_str())
        {
            continue;
        }
        match cadcraft_doc::legacy_entity_bounds(d, entity, 0) {
            Ok(entity_bounds) => bounds = bounds.union(&entity_bounds),
            Err(error) if result.refusal.is_none() => {
                let handle = if let cadcraft_doc::DocError::CanonicalExtent(handle) = &error { *handle } else { entity.handle };
                result.refusal = Some((handle, error));
            }
            Err(_) => {}
        }
    }
    if !bounds.is_empty() {
        if let (Some(min), Some(max)) = (&mut result.min, &mut result.max) {
            // Legacy bounds are XY only. Keep the source Z and grow only beyond its saved limits.
            if bounds.min.x < min.x {
                min.x = bounds.min.x;
            }
            if bounds.min.y < min.y {
                min.y = bounds.min.y;
            }
            if bounds.max.x > max.x {
                max.x = bounds.max.x;
            }
            if bounds.max.y > max.y {
                max.y = bounds.max.y;
            }
        } else if result.refusal.is_none() {
            result.min = Some(bounds.min.to3(0.0));
            result.max = Some(bounds.max.to3(0.0));
        }
    }
    result
}

/// Save notes for repaired handles, refused extents, and block attributes lost when canonical multileaders split
/// before R2007: a non-uniform block scale places no attribute text exactly.
pub(crate) fn omitted_notes(d: &Drawing, target: Version) -> Vec<String> {
    let mut notes: Vec<String> = d
        .committed()
        .with_unique_handles()
        .map(|(_, repairs)| repairs)
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            format!(
                "Handle {} was held by two objects ({} and {}): the one in {} now has handle {}; a reference to {} names the one in {}.",
                r.old.hex(),
                r.kept_in,
                r.moved_in,
                r.moved_in,
                r.new.hex(),
                r.old.hex(),
                r.kept_in
            )
        })
        .collect();
    let extents = save_extents(d);
    if let Some((handle, error)) = extents.refusal {
        let action = if extents.min.is_some() { "growth incomplete" } else { "omitted" };
        notes.push(format!("EXTMIN/EXTMAX {action}: the legacy extent query refused object {} ({error}).", handle.hex()));
    }
    if target >= Version::R2007 {
        return notes;
    }
    let style = d.header.str("DIMSTYLE", "Standard");
    let spaces = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    let attributes = spaces
        .flat_map(|s| s.iter())
        .filter(|e| e.geometry_revision == cadcraft_doc::GeometryRevision::CanonicalV2)
        .filter_map(|e| {
            let EntityKind::MLeader(m) = &e.kind else { return None };
            let base = m.block.as_ref().and_then(|b| d.block(&b.block)).map_or(cadcraft_geom::Vec3::ZERO, |b| b.base);
            let parts = m.split_canonical(&style, base).ok()?;
            let kept = parts.block.as_ref().map_or(0, |(i, _)| i.attribs.len());
            let left_out = m.block.as_ref().map_or(0, |b| b.attribs.len()).saturating_sub(kept);
            (left_out > 0).then(|| {
                format!(
                    "MULTILEADER {}: {left_out} block attribute(s) left out of the file: its block scale is not uniform, so no attribute text is exact before AutoCAD 2007.",
                    e.handle.hex()
                )
            })
        });
    notes.extend(attributes);
    notes
}

/// The R2000 DXF the DWG bridge converts to a `dwg` version DWG: its entity types are the DWG
/// version's (MULTILEADER from 2007). Its text is in the Windows single-byte page it declares,
/// so the bridge stores representable characters rather than their literal Unicode escapes.
pub(crate) fn dwg_bridge(d: &Drawing, dwg: Version) -> Vec<u8> {
    let mut tags = tags_for(d, Version::R2000, dwg, true);
    let page = cadcraft_dxf::single_byte_code_page(&d.header.str("DWGCODEPAGE", "ANSI_1252"));
    bridge_code_page(&mut tags, page);
    cadcraft_dxf::ascii_to_code_page(&cadcraft_dxf::write_ascii(&tags), page)
}

/// Declare the page used for the bridge bytes, including the explicit 1252 fallback when the
/// source uses a page outside the supported Windows single-byte set.
fn bridge_code_page(tags: &mut [Tag], page: &str) {
    let Some(at) = tags.iter().position(|t| t.code == 9 && t.str().trim().eq_ignore_ascii_case("$DWGCODEPAGE")) else { return };
    if let Some(t) = tags.get_mut(at + 1).filter(|t| t.code == 3) {
        *t = Tag::s(3, page);
    }
}

/// One entity's DXF groups, as the writer stores them, without its `0` group: its type name
/// and the groups of its record (an entity written as several records, a POLYLINE and its
/// VERTEXes, gives its first). Handles of objects outside the entity (owner, styles) are left
/// out. AutoLISP's `entget` is built on it.
pub fn entity_groups(d: &Drawing, e: &Entity) -> (String, Vec<Tag>) {
    // One record is not a file: no global handle reservation is needed for entget.
    let mut w = W::default();
    let cx = Ctx { target: Version::R2018, entget: true, ..Ctx::default() };
    let paper = d.space_of(e.handle).is_some_and(|s| !matches!(s, cadcraft_doc::Space::Model));
    entity(&mut w, d, e, "0", paper, &cx);
    let mut tags = w.t.into_iter();
    let kind = tags.next().filter(|t| t.code == 0).map(|t| t.value.as_str()).unwrap_or_default();
    (kind, tags.take_while(|t| t.code != 0).filter(|t| t.code != 330).collect())
}

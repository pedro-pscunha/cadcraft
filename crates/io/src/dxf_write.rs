//! Drawing → DXF (AC1015 / R2000 structure, readable by every DXF consumer since 2000). Later
//! versions keep that structure (the groups they add are optional, DXF Reference) and differ in
//! `$ACADVER`, from 2004 in the newer DIMSTYLE groups, and from 2007 in UTF-8 text (the caller
//! picks the text encoding) and MULTILEADER entities (older files get LEADER + MTEXT).

use std::collections::{HashMap, HashSet};

use cadcraft_color::Color;
use cadcraft_doc::{
    AssocSnap, Attrib, Block, Common, DimAssoc, DimKind, DimStyle, Dimension, Drawing, Entity, EntityKind, EntityStore, Gradient, Group, HAlign,
    HVal, Handle, Hatch, Insert, Layout, Leader, Line, Lineweight, LwPolyline, MLeader, MLeaderStyle, MText, PageSetup, Point, Solid, Space, Table,
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
use sections::tags_for;

#[derive(Default)]
struct W {
    t: Vec<Tag>,
    next: u64,
    first: u64,
    used: HashSet<Handle>,
    /// Source entity → its emitted top-level records (not ATTRIB/VERTEX/SEQEND children).
    emitted: HashMap<Handle, Vec<Handle>>,
    /// Upper-case plot style name → its `ACDBPLACEHOLDER` handle (group 390 of layers and objects).
    pstyles: HashMap<String, String>,
    /// Upper-case layer name → its LAYER table record handle (group 341 of viewports).
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
            .flat_map(|s| s.iter().map(|e| e.handle))
            .collect();
        #[cfg(test)]
        tests::SCANNED.with(|count| count.set(count.get().saturating_add(used.len())));
        Self {
            t: Vec::new(),
            next: first,
            first,
            used,
            emitted: HashMap::new(),
            pstyles: HashMap::new(),
            layers: HashMap::new(),
            reactors: HashMap::new(),
        }
    }

    fn attribute_handle(&mut self, wanted: Option<Handle>) -> String {
        if let Some(h) = wanted.filter(|h| h.0 != 0 && h.0 < self.first)
            && self.used.insert(h)
        {
            return h.hex();
        }
        self.h()
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
        while self.used.contains(&Handle(self.next)) {
            self.next = self.next.checked_add(1).unwrap_or(1);
        }
        let h = Handle(self.next);
        self.used.insert(h);
        self.next = self.next.checked_add(1).unwrap_or(1);
        h.hex()
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

/// The R2000 DXF the DWG bridge converts to a `dwg` version DWG: its entity types are the DWG
/// version's (MULTILEADER from 2007). Its text is in the Windows single-byte page it declares,
/// so the bridge stores representable characters rather than their literal Unicode escapes.
pub(crate) fn dwg_bridge(d: &Drawing, dwg: Version) -> Vec<u8> {
    let mut tags = tags_for(d, Version::R2000, dwg, true);
    let page = cadcraft_dxf::single_byte_code_page(&d.header.str("DWGCODEPAGE", "ANSI_1252"));
    bridge_markers(&mut tags);
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

/// Subclass markers the DWG bridge knows by another spelling (it matches them exactly):
/// AutoCAD writes a dynamic block lookup parameter's as `AcDbBlockLookUpParameter`, the bridge
/// reads `AcDbBlockLookupParameter`; otherwise the parameter loses its name and its lookup
/// action in the DWG, and AutoCAD's lookups stop working.
fn bridge_markers(tags: &mut [Tag]) {
    for t in tags.iter_mut().filter(|t| t.code == 100) {
        if t.str().trim().eq_ignore_ascii_case("AcDbBlockLookupParameter") {
            *t = Tag::s(100, "AcDbBlockLookupParameter");
        }
    }
}

/// One entity's DXF groups, as the writer stores them, without its `0` group: its type name
/// and the groups of its record (an entity written as several records, a POLYLINE and its
/// VERTEXes, gives its first). Handles of objects outside the entity (owner, styles) are left
/// out. AutoLISP's `entget` is built on it.
pub fn entity_groups(d: &Drawing, e: &Entity) -> (String, Vec<Tag>) {
    // One record is not a file: no global handle reservation is needed for entget.
    let mut w = W::default();
    let cx = Ctx { target: Version::R2018, ..Ctx::default() };
    let paper = d.space_of(e.handle).is_some_and(|s| !matches!(s, cadcraft_doc::Space::Model));
    entity(&mut w, d, e, "0", paper, &cx);
    let mut tags = w.t.into_iter();
    let kind = tags.next().filter(|t| t.code == 0).map(|t| t.value.as_str()).unwrap_or_default();
    (kind, tags.take_while(|t| t.code != 0).filter(|t| t.code != 330).collect())
}

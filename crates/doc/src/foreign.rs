//! Data from a drawing file that CADCraft does not model, kept so that saving the drawing writes
//! it back: other programs' objects, extension dictionaries, xdata and properties (#528).
//!
//! It is kept as the file's own DXF groups, with the file's handles. The file reader fills it and
//! the writer puts it back, pointing references at whatever now has the referenced role (the
//! named object dictionary, a table record, a block, an entity); references to objects that are
//! gone are dropped. Copies clone owned graphs. Opaque associations are retired when their guard
//! snapshot changes or a resolved target goes; other foreign data stays with its owner.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Entity, EntityKind, Handle, RawTag};

mod association;
mod block_copy;
mod export;
mod merge;
pub use block_copy::BlockContentCopy;
use block_copy::{BlockNamespace, block_role};
#[cfg(test)]
mod tests;
use association::RawDimAssoc;

/// Longest foreign-data ownership chain followed while copying or retiring objects.
pub const MAX_FOREIGN_DEPTH: usize = 64;

/// Whether copied entities belong to this drawing's foreign-data namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicationSource {
    /// Clone owned metadata and remap references inside each copy set.
    SameDrawing,
    /// Assign new identities but do not adopt another drawing's raw metadata.
    OtherDrawing,
}

/// The normalized DIMASSOC subclass position and the dimension referenced by its first 330.
pub fn dimassoc_reference<S: AsRef<str>>(groups: impl IntoIterator<Item = (i32, S)>) -> Option<(usize, Handle)> {
    let mut start = None;
    let mut grouped = false;
    for (i, (code, value)) in groups.into_iter().enumerate() {
        if code == 102 {
            grouped = value.as_ref().trim().starts_with('{');
            continue;
        }
        if grouped {
            continue;
        }
        if start.is_none() && code == 100 && value.as_ref().trim().eq_ignore_ascii_case("AcDbDimAssoc") {
            start = Some(i);
        } else if let Some(start) = start
            && code == 330
        {
            return Handle::parse_hex(value.as_ref()).map(|h| (start, h));
        }
    }
    None
}

/// What a file handle stood for when the object it named is one CADCraft writes itself (under a
/// new handle on save): a kind and an upper-case name, e.g. `("LAYER", "WALLS")`,
/// `("TABLE", "LAYER")`, `("BLOCK", "DOOR")`, `("NOD", "")` for the named object dictionary and
/// `("NOD", "ACAD_GROUP")` for one of its entries. Entity and attribute instances retain their
/// handles and do not need roles.
pub type ForeignKey = (String, String);

/// Types of entity CADCraft keeps as read (it doesn't model them) that the DWG writer also keeps:
/// the DWG bridge reads their DXF groups into entities of its own and writes them, and a round
/// trip through DWG keeps them (`crates/io/tests/it/unknown_entity_block.rs`). Other unmodelled
/// types are left out of a DWG save, and the save says so. The ACIS types (3DSOLID, REGION, BODY and
/// the surfaces) are not listed: their geometry is data the writer has only from the data store.
/// acadrust has no list of what its DWG writer writes: `dwg_keeps_unknown.rs` saves a record of
/// each type to DWG at every version and checks this list against what comes back.
pub const DWG_KEEPS_UNKNOWN: &[&str] = &[
    "ARC_DIMENSION",
    "LARGE_RADIAL_DIMENSION",
    "MPOLYGON",
    "SHAPE",
    "MESH",
    "LIGHT",
    "SECTIONLINE",
    "DRAWINGVIEW",
    "PDFUNDERLAY",
    "DWFUNDERLAY",
    "DGNUNDERLAY",
    "OLE2FRAME",
    "OLEFRAME",
    "CAMERA",
    "SECTIONOBJECT",
    "ARCALIGNEDTEXT",
    "RTEXT",
    "POSITIONMARKER",
    "GEOPOSITIONMARKER",
    "COORDINATION_MODEL",
    "NAVISWORKSMODEL",
    "ACDBPOINTCLOUD",
    "POINTCLOUD",
    "ACDBPOINTCLOUDEX",
    "POINTCLOUDEX",
    "FORMAT",
    "ALIGNMENTPARAMETERENTITY",
    "BASEPOINTPARAMETERENTITY",
    "FLIPPARAMETERENTITY",
    "LINEARPARAMETERENTITY",
    "POINTPARAMETERENTITY",
    "ROTATIONPARAMETERENTITY",
    "VISIBILITYPARAMETERENTITY",
    "XYPARAMETERENTITY",
    "BLOCKANGULARCONSTRAINTPARAMETERENTITY",
    "FLIPGRIPENTITY",
    "LINEARGRIPENTITY",
    "POLARGRIPENTITY",
    "ROTATIONGRIPENTITY",
    "VISIBILITYGRIPENTITY",
    "XYGRIPENTITY",
    "WALL",
    "MCSDBOBJECT",
    "NOTEPOSITION",
    "SPDSLEVELMARK",
    "SPDSRELATIONMARK",
    "ACAD_PROXY_ENTITY_WRAPPER",
];

/// A non-graphical object of a type or role CADCraft does not model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForeignObject {
    /// Its handle in the file it came from (kept on save).
    pub handle: Handle,
    /// Its DXF type (group 0), e.g. `XRECORD`.
    pub kind: String,
    /// Its groups after the type, as read.
    pub tags: Vec<RawTag>,
}

/// Source proxy graphics, guarded by the parsed geometry and its display dependencies.
#[derive(Clone, Debug, PartialEq)]
pub struct ProxyGraphics {
    pub geometry: String,
    /// No display proof when cache bytes are invalid or dependencies exceed the bounded inspection budget.
    pub display: Option<u64>,
    pub tags: Vec<RawTag>,
    /// The graphics were in the `AcDbMLeader` subclass of the source record, not in its `AcDbEntity` one.
    pub in_mleader: bool,
}

/// What an entity, table record, block or attribute carries beyond what CADCraft models.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForeignExtra {
    /// Its extension dictionary (one of [`Foreign::objects`]).
    pub xdict: Option<Handle>,
    /// Objects of [`Foreign::objects`] that react to it (persistent reactors).
    pub reactors: Vec<Handle>,
    /// Other applications' extended data of a table record, block or attribute: groups from
    /// `1001` on. An entity's is part of the entity ([`crate::Common::xdata`]), so it follows its copies.
    pub xdata: Vec<RawTag>,
    /// The colour book colour (group 430) and the true colour (group 420) it was read with: it is
    /// written back while the colour is unchanged.
    pub color_book: Option<(String, String)>,
    /// Source true color (420) and its independent indexed fallback (62).
    pub color_index: Option<(String, i64)>,
    /// Whether the source block had nonconstant attribute definitions.
    pub block_attributes: Option<bool>,
    /// The own layer, colour and the rest of the common properties of the SEQEND that closes the ATTRIBs of an INSERT, as the
    /// file had them (the SEQEND of a new INSERT takes the INSERT's layer). Its handle follows its owner, as a role.
    pub insert_seqend: Option<crate::Common>,
    /// Other unmodelled property groups of an entity's `AcDbEntity` subclass (material 347, shadow mode 284).
    pub props: Vec<RawTag>,
    /// A MULTILEADER's opaque source graphics. A geometry or display edit invalidates replay.
    pub proxy_graphics: Option<ProxyGraphics>,
    /// The groups of the record's own subclass the model has no field for (a viewport's view, UCS
    /// and sun link, a leader's annotation link, a VPORT record's view), as read.
    pub subclass: Vec<RawTag>,
    /// A layout's plot settings and layout groups as read (each after its `100` subclass marker):
    /// the writer takes from them what it has no model for (limits, extents, UCS, plot origin...).
    pub layout: Vec<RawTag>,
}

impl ForeignExtra {
    pub fn is_empty(&self) -> bool {
        self.xdict.is_none()
            && self.reactors.is_empty()
            && self.xdata.is_empty()
            && self.color_book.is_none()
            && self.color_index.is_none()
            && self.block_attributes.is_none()
            && self.insert_seqend.is_none()
            && self.props.is_empty()
            && self.proxy_graphics.is_none()
            && self.subclass.is_empty()
            && self.layout.is_empty()
    }
}

/// The SORTENTSTABLE a space had in the file: pairs of an entity handle and the sort handle it is
/// drawn by, in the file's order (AutoCAD lists them by ascending entity handle). An empty table
/// is kept too: AutoCAD keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortTable {
    pub entries: Vec<(Handle, u64)>,
    /// How many entries the extension dictionary listed before ACAD_SORTENTS.
    pub place: usize,
}

impl SortTable {
    /// The entries whose entity is in `members`.
    pub fn restricted(&self, members: &BTreeSet<Handle>) -> Vec<(Handle, u64)> {
        self.entries.iter().copied().filter(|(entity, _)| members.contains(entity)).collect()
    }

    /// `members` in the order `entries` draw them: by sort handle, an unlisted one by its own handle.
    pub fn draw_order(entries: &[(Handle, u64)], members: &BTreeSet<Handle>) -> Vec<Handle> {
        let keys: BTreeMap<Handle, u64> = entries.iter().copied().collect();
        let mut order: Vec<Handle> = members.iter().copied().collect();
        order.sort_by_key(|h| (keys.get(h).copied().unwrap_or(h.0), *h));
        order
    }
}

/// Everything of a file CADCraft keeps without modelling it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Foreign {
    /// CLASS records (groups after `0 CLASS`) of the kept objects' and entities' types.
    pub classes: Vec<Vec<RawTag>>,
    /// Kept objects, in file order.
    pub objects: Vec<ForeignObject>,
    /// Unnamed STYLE/DIMSTYLE records, kept outside the selectable, named style lists.
    pub table_records: Vec<ForeignObject>,
    /// Entries of the named object dictionary that CADCraft doesn't write itself: (key, object).
    pub dictionary: Vec<(String, Handle)>,
    /// Extras of entities and attribute instances, by their document handle.
    pub entities: BTreeMap<Handle, ForeignExtra>,
    /// Guard snapshots for retained DIMASSOC records, including semantic evaluability.
    pub(crate) raw_dimassocs: BTreeMap<Handle, RawDimAssoc>,
    /// Extras of table records (`("LAYER", "WALLS")`), tables (`("TABLE", "LAYER")`) and blocks
    /// (`("BLOCK", "DOOR")`), by [`ForeignKey`].
    pub records: BTreeMap<ForeignKey, ForeignExtra>,
    /// The roles of the file handles the kept data refers to, for objects CADCraft rewrites.
    pub targets: BTreeMap<Handle, ForeignKey>,
    /// The upper-case names of the file's classes in its CLASSES order, kept when the file has
    /// proxy objects or entities: a proxy's class number (group 91) is 500 plus its class's place
    /// here, and saving renumbers it to the written CLASSES (#1812).
    pub class_order: Vec<String>,
    /// The SORTENTSTABLE of each space, by the model (`("MODEL", "")`), a layout (`("LAYOUT", name)`)
    /// or a block (`("BLOCK", NAME)`): the save writes it back while the space still draws in the
    /// order it gives.
    pub sort_tables: BTreeMap<ForeignKey, SortTable>,
    /// The file's data store section (ACDSDATA, AutoCAD 2013 and later), as read: its groups
    /// after the section name. It holds the ACIS geometry of 3D solids, regions, bodies and
    /// surfaces, which from 2013 on those entities don't carry themselves: each `ACDSRECORD`
    /// names its entity with a 320 handle (see [`Foreign::data_store_entities`]).
    pub data_store: Vec<RawTag>,
}

impl Foreign {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
            && self.table_records.is_empty()
            && self.dictionary.is_empty()
            && self.entities.is_empty()
            && self.raw_dimassocs.is_empty()
            && self.records.is_empty()
            && self.classes.is_empty()
            && self.targets.is_empty()
            && self.class_order.is_empty()
            && self.sort_tables.is_empty()
            && self.data_store.is_empty()
    }

    /// The entities the data store has records for (their 320 handles).
    pub fn data_store_entities(&self) -> std::collections::BTreeSet<Handle> {
        self.data_store.iter().filter(|t| t.code == 320).filter_map(|t| Handle::parse_hex(t.value.trim())).collect()
    }
}

/// DXF group codes whose values are translated object references (DXF Reference: soft and hard
/// pointers and owners, 330-369, 390-399, 480-481, and the xdata handle 1005). The 320-329
/// "arbitrary handle" groups are taken as is, and the ACDSDATA link is a 320 ([`remap_data_store`]).
pub fn is_pointer(code: i32) -> bool {
    matches!(code,330..=369|390..=399|480..=481|1005)
}

/// Point the data store's links (group 320, the entity a record belongs to) at the saved handles.
pub(crate) fn remap_data_store(tags: &mut [RawTag], map: &BTreeMap<Handle, Handle>) {
    for t in tags.iter_mut().filter(|t| t.code == 320) {
        if let Some(new) = Handle::parse_hex(&t.value).and_then(|h| map.get(&h)) {
            t.value = new.hex();
        }
    }
}

/// DXF dictionary record types, normalized without allocating a name.
pub fn is_dictionary(kind: &str) -> bool {
    let kind = kind.trim();
    kind.eq_ignore_ascii_case("DICTIONARY") || kind.eq_ignore_ascii_case("ACDBDICTIONARYWDFLT")
}

/// An object's owner, excluding reactor/extension/application groups and xdata.
pub fn object_owner<S: AsRef<str>>(groups: impl IntoIterator<Item = (i32, S)>) -> Option<Handle> {
    let mut grouped = false;
    for (code, value) in groups.into_iter().take_while(|(code, _)| *code < 1000) {
        if code == 102 {
            grouped = value.as_ref().trim().starts_with('{');
        } else if code == 330 && !grouped {
            return Handle::parse_hex(value.as_ref());
        }
    }
    None
}

fn owner(tags: &[RawTag]) -> Option<Handle> {
    #[cfg(test)]
    tests::OWNERS.with(|count| count.set(count.get().saturating_add(1)));
    object_owner(tags.iter().map(|t| (t.code, t.value.as_str())))
}

fn owner_index(objects: &[ForeignObject]) -> BTreeMap<Handle, Vec<usize>> {
    let mut index: BTreeMap<Handle, Vec<usize>> = BTreeMap::new();
    for (i, object) in objects.iter().enumerate() {
        if let Some(parent) = owner(&object.tags) {
            index.entry(parent).or_default().push(i);
        }
    }
    index
}

fn owned_indices(objects: &[ForeignObject], index: &BTreeMap<Handle, Vec<usize>>, roots: impl IntoIterator<Item = Handle>) -> BTreeSet<usize> {
    let mut owned = BTreeSet::new();
    let mut pending: Vec<_> = roots.into_iter().map(|h| (h, 0usize)).collect();
    while let Some((parent, depth)) = pending.pop() {
        if depth >= MAX_FOREIGN_DEPTH {
            continue;
        }
        for i in index.get(&parent).into_iter().flatten() {
            if owned.insert(*i)
                && let Some(child) = objects.get(*i)
            {
                pending.push((child.handle, depth + 1));
            }
        }
    }
    owned
}

/// Remove whole dictionary key/pointer pairs selected by their key and target text.
pub fn strip_dictionary_entries(tags: &mut Vec<RawTag>, remove: impl Fn(&str, &str) -> bool) {
    let mut out = Vec::with_capacity(tags.len());
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        if t.code == 3 && tags.get(i + 1).is_some_and(|next| matches!(next.code, 350 | 360) && remove(&t.value, &next.value)) {
            i += 2;
            continue;
        }
        out.push(t.clone());
        i += 1;
    }
    *tags = out;
}

fn drop_references_to(tags: &mut Vec<RawTag>, dictionary: bool, removed: &BTreeSet<Handle>) {
    if dictionary {
        strip_dictionary_entries(tags, |_, target| Handle::parse_hex(target).is_some_and(|h| removed.contains(&h)));
    }
    let gone = |t: &RawTag| Handle::parse_hex(&t.value).is_some_and(|h| removed.contains(&h));
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        if t.code == 102 && t.value.trim() == "{ACAD_REACTORS" {
            let start = i;
            i += 1;
            let mut reactors = Vec::new();
            while let Some(r) = tags.get(i) {
                if r.code == 102 {
                    break;
                }
                if !gone(r) {
                    reactors.push(r.clone());
                }
                i += 1;
            }
            if !reactors.is_empty() {
                out.extend(tags.get(start).cloned());
                out.extend(reactors);
                out.extend(tags.get(i).cloned());
            }
            i += usize::from(tags.get(i).is_some());
            continue;
        }
        out.push(t.clone());
        i += 1;
    }
    *tags = out;
}

fn remap_tags(tags: &mut [RawTag], map: &BTreeMap<Handle, Handle>) {
    for t in tags {
        if (matches!(t.code, 5 | 105) || is_pointer(t.code))
            && let Some(new) = Handle::parse_hex(&t.value).and_then(|h| map.get(&h))
        {
            t.value = new.hex();
        }
    }
}

fn has_entity_metadata(foreign: &Foreign, entities: &[Vec<Entity>]) -> bool {
    entities.iter().flatten().any(|entity| {
        foreign.entities.contains_key(&entity.handle)
            || matches!(entity.kind, EntityKind::Unknown(_))
            || entity
                .kind
                .attributes()
                .iter()
                .filter_map(|a| a.handle)
                .chain(entity.kind.polyline_records().filter_map(|r| r.handle))
                .any(|h| foreign.entities.contains_key(&h))
    })
}

fn remap_extra(extra: &mut ForeignExtra, map: &BTreeMap<Handle, Handle>) {
    extra.xdict = extra.xdict.and_then(|h| map.get(&h).copied());
    extra.reactors = extra.reactors.iter().filter_map(|h| map.get(h).copied()).collect();
    remap_tags(&mut extra.xdata, map);
    remap_tags(&mut extra.props, map);
    remap_tags(&mut extra.subclass, map);
}

fn remap_xdata(tags: &mut [RawTag], map: &BTreeMap<Handle, Handle>, source: DuplicationSource) {
    if source == DuplicationSource::OtherDrawing {
        for t in tags.iter_mut().filter(|t| t.code == 1005) {
            if Handle::parse_hex(&t.value).is_none_or(|h| !map.contains_key(&h)) {
                t.value = "0".into();
            }
        }
    }
    remap_tags(tags, map);
}

fn remap_model(kind: &mut EntityKind, map: &BTreeMap<Handle, Handle>, source: DuplicationSource, discard_association: bool) {
    match kind {
        EntityKind::Dimension(dimension) => {
            if source == DuplicationSource::OtherDrawing || discard_association {
                dimension.assoc.clear();
            }
            dimension.assoc.retain(|link| {
                map.get(&link.handle).is_some_and(|h| h.0 != 0)
                    && match link.snap {
                        crate::AssocSnap::Intersection { other } => map.get(&other).is_some_and(|h| h.0 != 0),
                        _ => true,
                    }
            });
            for link in &mut dimension.assoc {
                link.handle = map.get(&link.handle).copied().unwrap_or(link.handle);
                if let crate::AssocSnap::Intersection { other } = &mut link.snap {
                    *other = map.get(other).copied().unwrap_or(*other);
                }
            }
        }
        EntityKind::Hatch(hatch) => {
            for boundary in &mut hatch.loops {
                boundary.sources = boundary
                    .sources
                    .iter()
                    .filter_map(|h| map.get(h).copied().or_else(|| (source == DuplicationSource::SameDrawing).then_some(*h)))
                    .filter(|h| h.0 != 0)
                    .collect();
            }
        }
        EntityKind::Unknown(unknown) => {
            if source == DuplicationSource::OtherDrawing {
                for t in &mut unknown.tags {
                    if is_pointer(t.code) && Handle::parse_hex(&t.value).is_none_or(|h| !map.contains_key(&h)) {
                        t.value = "0".into();
                    }
                }
            }
            remap_tags(&mut unknown.tags, map);
        }
        _ => {}
    }
}

impl crate::Drawing {
    /// Retire raw associations for one command's set of dimensions. Index ownership once and
    /// rewrite only objects that reference a removed handle; unrelated raw storage stays intact.
    /// Returns the dimensions that actually had a raw association.
    pub fn retire_raw_dimassocs(&mut self, dimensions: &BTreeSet<Handle>) -> BTreeSet<Handle> {
        if dimensions.is_empty() {
            return BTreeSet::new();
        }
        let mut retired = BTreeSet::new();
        let roots: BTreeSet<_> = self
            .foreign
            .objects
            .iter()
            .filter(|o| o.kind == "DIMASSOC")
            .filter_map(|o| {
                let (_, dimension) = dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str())))?;
                if !dimensions.contains(&dimension) {
                    return None;
                }
                retired.insert(dimension);
                Some(o.handle)
            })
            .collect();
        if roots.is_empty() {
            return retired;
        }
        let index = owner_index(&self.foreign.objects);
        let by_handle: BTreeMap<_, _> = self.foreign.objects.iter().enumerate().map(|(i, o)| (o.handle, i)).collect();
        let mut removed: BTreeSet<_> = owned_indices(&self.foreign.objects, &index, roots.iter().copied())
            .into_iter()
            .filter_map(|i| self.foreign.objects.get(i).map(|o| o.handle))
            .collect();
        removed.extend(roots);
        for dictionary in retired.iter().filter_map(|h| self.foreign.entities.get(h).and_then(|x| x.xdict)) {
            let object = by_handle.get(&dictionary).and_then(|i| self.foreign.objects.get(*i));
            let empty = object.is_some_and(|o| {
                is_dictionary(&o.kind)
                    && !o.tags.iter().any(|t| t.code >= 1000)
                    && o.tags.iter().enumerate().filter(|(_, t)| t.code == 3).all(|(i, _)| {
                        o.tags
                            .get(i + 1)
                            .is_some_and(|t| matches!(t.code, 350 | 360) && Handle::parse_hex(&t.value).is_some_and(|h| removed.contains(&h)))
                    })
            });
            let has_children =
                index.get(&dictionary).into_iter().flatten().filter_map(|i| self.foreign.objects.get(*i)).any(|o| !removed.contains(&o.handle));
            if empty && !has_children {
                removed.insert(dictionary);
            }
        }
        let foreign = std::sync::Arc::make_mut(&mut self.foreign);
        foreign.objects.retain(|o| !removed.contains(&o.handle));
        for object in &mut foreign.objects {
            if object.tags.iter().any(|t| is_pointer(t.code) && Handle::parse_hex(&t.value).is_some_and(|h| removed.contains(&h))) {
                drop_references_to(&mut object.tags, is_dictionary(&object.kind), &removed);
            }
        }
        for extra in foreign.entities.values_mut().chain(foreign.records.values_mut()) {
            extra.xdict = extra.xdict.filter(|h| !removed.contains(h));
            extra.reactors.retain(|h| !removed.contains(h));
        }
        foreign.entities.retain(|_, extra| !extra.is_empty());
        foreign.records.retain(|_, extra| !extra.is_empty());
        foreign.raw_dimassocs.retain(|h, _| !retired.contains(h));
        retired
    }

    /// Duplicate entity sets with fresh entity and attribute identities. Each inner vector is
    /// one copy set: references within it follow that copy. Multiple transformed sets batch an
    /// entire command, building the owner index once and mutating Foreign at most once.
    /// External drawings contribute model data only. Raw dimension associations are copied only
    /// when all their geometry is in the same set; dictionaries/reactors never retain old owners.
    pub fn duplicate_entities(&mut self, sets: Vec<Vec<Entity>>, source: DuplicationSource) -> crate::Result<Vec<Entity>> {
        for entity in sets.iter().flatten() {
            entity.require_copyable()?;
        }
        Ok(self.duplicate_entity_sets(sets, source, &BTreeMap::new(), None))
    }

    /// Give merged geometry a new identity and its chosen source's owned metadata. References
    /// to any consumed entity follow the result, without adopting those entities' metadata.
    pub fn merge_entity(&mut self, entity: Entity, consumed: &[Handle]) -> crate::Result<Entity> {
        if let EntityKind::Raw(raw) = &entity.kind {
            return Err(raw.edit_refusal());
        }
        for handle in consumed {
            if let Some(entity) = self.entity(*handle)
                && let EntityKind::Raw(raw) = &entity.kind
            {
                return Err(raw.edit_refusal());
            }
        }
        let aliases = consumed.iter().map(|h| (*h, entity.handle)).collect();
        let merged = self
            .duplicate_entity_sets(vec![vec![entity]], DuplicationSource::SameDrawing, &aliases, None)
            .pop()
            .ok_or_else(|| crate::DocError::Invalid("merged entity was not created".into()))?;
        self.remap_merged_references(consumed, merged.handle);
        Ok(merged)
    }

    fn allocate_copy_roots(&mut self, entities: &mut [Entity]) -> BTreeMap<Handle, Handle> {
        let mut roots = BTreeMap::new();
        for entity in entities {
            let old = entity.handle;
            entity.handle = self.new_handle();
            if old.0 != 0 {
                roots.insert(old, entity.handle);
            }
            if let EntityKind::AttDef(definition) = &mut entity.kind {
                definition.handle = None;
            }
            for attribute in entity.kind.attributes_mut() {
                let old = attribute.handle;
                let new = self.new_handle();
                attribute.handle = Some(new);
                if let Some(old) = old {
                    roots.insert(old, new);
                }
            }
            entity.kind.edit_polyline_records(|record| {
                let handle = match record.handle {
                    Some(old) => *roots.entry(old).or_insert_with(|| self.new_handle()),
                    None => self.new_handle(),
                };
                record.handle = Some(handle);
            });
        }
        roots
    }

    fn duplicate_entity_sets(
        &mut self,
        sets: Vec<Vec<Entity>>,
        source: DuplicationSource,
        aliases: &BTreeMap<Handle, Handle>,
        mut namespace: Option<&mut BlockNamespace>,
    ) -> Vec<Entity> {
        let named_roles: Vec<_> = namespace
            .as_ref()
            .map(|copy| {
                self.foreign.targets.iter().filter(|(_, key)| block_role(key, &copy.source)).map(|(h, key)| (*h, copy.target_key(key))).collect()
            })
            .unwrap_or_default();
        let named_records: Vec<_> = namespace
            .as_ref()
            .map(|copy| {
                self.foreign
                    .records
                    .iter()
                    .filter(|(key, _)| block_role(key, &copy.source))
                    .map(|(key, extra)| (copy.target_key(key), extra.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let has_metadata = source == DuplicationSource::SameDrawing
            && !self.foreign.is_empty()
            && (!named_roles.is_empty() || !named_records.is_empty() || has_entity_metadata(&self.foreign, &sets));
        let index = if has_metadata { owner_index(&self.foreign.objects) } else { BTreeMap::new() };
        let mut added_objects = Vec::new();
        let mut added_extras = Vec::new();
        let mut added_snapshots = Vec::new();
        let mut added_records = Vec::new();
        let mut added_targets = Vec::new();
        let mut out = Vec::new();
        for mut entities in sets {
            let mut roots = self.allocate_copy_roots(&mut entities);
            for (old, key) in &named_roles {
                let new = self.new_handle();
                roots.insert(*old, new);
                added_targets.push((new, key.clone()));
            }
            // Reference aliases are many-to-one; only real copied roots own cloned graphs.
            let mut map = std::borrow::Cow::Borrowed(&roots);
            if !aliases.is_empty() {
                let map = map.to_mut();
                for (old, source) in aliases {
                    if let Some(new) = roots.get(source) {
                        map.insert(*old, *new);
                    }
                }
            }
            // A deleted member of the source definition is not a legitimate external
            // reference just because BSAVEAS restores that member in ORIGINAL.
            BlockNamespace::close_map(namespace.as_deref(), &mut map);
            if !has_metadata {
                for entity in &mut entities {
                    remap_model(&mut entity.kind, &map, source, false);
                    remap_xdata(&mut entity.common.xdata, &map, source);
                }
                BlockNamespace::capture(namespace.as_deref_mut(), &map);
                out.extend(entities);
                continue;
            }
            let owned = owned_indices(&self.foreign.objects, &index, roots.keys().copied());
            let mut dropped = BTreeSet::new();
            let mut discarded_dimensions = BTreeSet::new();
            for i in &owned {
                let Some(object) = self.foreign.objects.get(*i).filter(|o| o.kind == "DIMASSOC") else { continue };
                let dimension = dimassoc_reference(object.tags.iter().map(|t| (t.code, t.value.as_str()))).map(|(_, h)| h);
                let refs: Vec<_> = object.tags.iter().filter(|t| matches!(t.code, 331 | 332)).collect();
                if refs.is_empty()
                    || dimension.is_none_or(|h| !roots.contains_key(&h))
                    || refs.iter().any(|t| Handle::parse_hex(&t.value).is_none_or(|h| !roots.contains_key(&h)))
                {
                    dropped.insert(object.handle);
                    discarded_dimensions.extend(dimension);
                }
            }
            let descendants = owned_indices(&self.foreign.objects, &index, dropped.iter().copied());
            dropped.extend(descendants.iter().filter_map(|i| self.foreign.objects.get(*i).map(|o| o.handle)));
            let mut objects: Vec<_> =
                owned.iter().filter_map(|i| self.foreign.objects.get(*i)).filter(|o| !dropped.contains(&o.handle)).cloned().collect();
            for object in &mut objects {
                drop_references_to(&mut object.tags, is_dictionary(&object.kind), &dropped);
            }
            let dictionaries: Vec<_> = roots.keys().filter_map(|h| self.foreign.entities.get(h).and_then(|e| e.xdict)).collect();
            let by_handle: BTreeMap<_, _> = objects.iter().map(|o| (o.handle, o)).collect();
            let parents: BTreeSet<_> = objects.iter().filter_map(|o| owner(&o.tags)).collect();
            for dictionary in dictionaries {
                let empty =
                    by_handle.get(&dictionary).is_some_and(|o| is_dictionary(&o.kind) && !o.tags.iter().any(|t| t.code == 3 || t.code >= 1000));
                if empty && !parents.contains(&dictionary) {
                    dropped.insert(dictionary);
                }
            }
            objects.retain(|o| !dropped.contains(&o.handle));
            let mut map = map.into_owned();
            for object in &objects {
                map.insert(object.handle, self.new_handle());
            }
            for object in &mut objects {
                if let Some(h) = map.get(&object.handle) {
                    object.handle = *h;
                }
                remap_tags(&mut object.tags, &map);
            }
            let opaque: BTreeSet<_> = objects
                .iter()
                .filter(|o| o.kind == "DIMASSOC")
                .filter_map(|o| dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str()))).map(|(_, h)| h))
                .collect();
            if !opaque.is_empty() {
                let attribute_owners =
                    entities.iter().flat_map(|e| e.kind.attributes().iter().filter_map(move |a| a.handle.map(|h| (h, e.handle)))).collect();
                for (old, new) in &roots {
                    if opaque.contains(new)
                        && let Some(snapshot) = self.foreign.raw_dimassocs.get(old)
                    {
                        added_snapshots.push((*new, snapshot.remapped(&map, &attribute_owners)));
                    }
                }
            }
            added_objects.extend(objects);
            for (old, new) in &roots {
                if let Some(mut extra) = self.foreign.entities.get(old).cloned() {
                    remap_extra(&mut extra, &map);
                    if !extra.is_empty() {
                        added_extras.push((*new, extra));
                    }
                }
            }
            for (key, extra) in &named_records {
                let mut extra = extra.clone();
                remap_extra(&mut extra, &map);
                added_records.push((key.clone(), extra));
            }
            BlockNamespace::capture(namespace.as_deref_mut(), &map);
            let discarded: BTreeSet<_> = discarded_dimensions.iter().filter_map(|h| roots.get(h).copied()).collect();
            for entity in &mut entities {
                remap_model(&mut entity.kind, &map, source, discarded.contains(&entity.handle));
                remap_xdata(&mut entity.common.xdata, &map, source);
            }
            out.extend(entities);
        }
        if !added_objects.is_empty() || !added_extras.is_empty() || !added_records.is_empty() || !added_targets.is_empty() {
            let foreign = std::sync::Arc::make_mut(&mut self.foreign);
            foreign.objects.extend(added_objects);
            foreign.entities.extend(added_extras);
            foreign.raw_dimassocs.extend(added_snapshots);
            foreign.records.extend(added_records);
            foreign.targets.extend(added_targets);
        }
        out
    }
}

//! Data from a drawing file that CADCraft does not model, kept so that saving the drawing writes
//! it back: other programs' objects, extension dictionaries, xdata and properties (#528).
//!
//! It is kept as the file's own DXF groups, with the file's handles. The file reader fills it and
//! the writer puts it back, pointing references at whatever now has the referenced role (the
//! named object dictionary, a table record, a block, an entity); references to objects that are
//! gone are dropped. Copies clone owned graphs; explicit dimension association commands retire
//! raw associations they replace. Other commands leave the kept data alone.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Handle, RawTag};

/// What a file handle stood for when the object it named is one CADCraft writes itself (under a
/// new handle on save): a kind and an upper-case name, e.g. `("LAYER", "WALLS")`,
/// `("TABLE", "LAYER")`, `("BLOCK", "DOOR")`, `("NOD", "")` for the named object dictionary and
/// `("NOD", "ACAD_GROUP")` for one of its entries. Entity and attribute instances retain their
/// handles and do not need roles.
pub type ForeignKey = (String, String);

/// Types of entity CADCraft keeps as read (it doesn't model them) that the DWG writer also keeps:
/// the DWG bridge reads their DXF groups into entities of its own and writes them, and a round
/// trip through DWG keeps them (`crates/io/tests/it/unknown_entity_block.rs`). Other unmodelled types are
/// left out of a DWG save, and the save says so.
pub const DWG_KEEPS_UNKNOWN: &[&str] = &["ARC_DIMENSION", "OLE2FRAME"];

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
    /// Other unmodelled property groups (material 347, shadow mode 284).
    pub props: Vec<RawTag>,
}

impl ForeignExtra {
    pub fn is_empty(&self) -> bool {
        self.xdict.is_none() && self.reactors.is_empty() && self.xdata.is_empty() && self.color_book.is_none() && self.props.is_empty()
    }
}

/// Everything of a file CADCraft keeps without modelling it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Foreign {
    /// CLASS records (groups after `0 CLASS`) of the kept objects' and entities' types.
    pub classes: Vec<Vec<RawTag>>,
    /// Kept objects, in file order.
    pub objects: Vec<ForeignObject>,
    /// Entries of the named object dictionary that CADCraft doesn't write itself: (key, object).
    pub dictionary: Vec<(String, Handle)>,
    /// Extras of entities and attribute instances, by their document handle.
    pub entities: BTreeMap<Handle, ForeignExtra>,
    /// Extras of table records (`("LAYER", "WALLS")`), tables (`("TABLE", "LAYER")`) and blocks
    /// (`("BLOCK", "DOOR")`), by [`ForeignKey`].
    pub records: BTreeMap<ForeignKey, ForeignExtra>,
    /// The roles of the file handles the kept data refers to, for objects CADCraft rewrites.
    pub targets: BTreeMap<Handle, ForeignKey>,
    /// The upper-case names of the file's classes in its CLASSES order, kept when the file has
    /// proxy objects or entities: a proxy's class number (group 91) is 500 plus its class's place
    /// here, and saving renumbers it to the written CLASSES (#1812).
    pub class_order: Vec<String>,
    /// The file's data store section (ACDSDATA, AutoCAD 2013 and later), as read: its groups
    /// after the section name. It holds the ACIS geometry of 3D solids, regions, bodies and
    /// surfaces, which from 2013 on those entities don't carry themselves: each `ACDSRECORD`
    /// names its entity with a 320 handle (see [`Foreign::data_store_entities`]).
    pub data_store: Vec<RawTag>,
}

impl Foreign {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
            && self.dictionary.is_empty()
            && self.entities.is_empty()
            && self.records.is_empty()
            && self.classes.is_empty()
            && self.targets.is_empty()
            && self.class_order.is_empty()
            && self.data_store.is_empty()
    }

    /// The entities the data store has records for (their 320 handles).
    pub fn data_store_entities(&self) -> std::collections::BTreeSet<Handle> {
        self.data_store.iter().filter(|t| t.code == 320).filter_map(|t| Handle::parse_hex(t.value.trim())).collect()
    }
}

/// An object's owner, excluding the handles in its reactor and extension-dictionary groups.
fn owner(tags: &[RawTag]) -> Option<Handle> {
    let mut grouped = false;
    for t in tags.iter().take_while(|t| t.code < 1000) {
        if t.code == 102 {
            grouped = t.value.trim().starts_with('{');
        } else if t.code == 330 && !grouped {
            return Handle::parse_hex(&t.value);
        }
    }
    None
}

fn owned_objects(objects: &[ForeignObject], roots: impl IntoIterator<Item = Handle>) -> BTreeSet<Handle> {
    let mut children: BTreeMap<Handle, Vec<Handle>> = BTreeMap::new();
    for object in objects {
        if let Some(parent) = owner(&object.tags) {
            children.entry(parent).or_default().push(object.handle);
        }
    }
    let mut owned = BTreeSet::new();
    let mut pending: Vec<_> = roots.into_iter().map(|h| (h, 0usize)).collect();
    while let Some((parent, depth)) = pending.pop() {
        if depth >= 64 {
            continue;
        }
        for child in children.get(&parent).into_iter().flatten() {
            if owned.insert(*child) {
                pending.push((*child, depth + 1));
            }
        }
    }
    owned
}

fn strip_edges(tags: &mut Vec<RawTag>, dictionary: bool, removed: &BTreeSet<Handle>) {
    let gone = |t: &RawTag| Handle::parse_hex(&t.value).is_some_and(|h| removed.contains(&h));
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        if dictionary && t.code == 3 && tags.get(i + 1).is_some_and(|n| matches!(n.code, 350 | 360) && gone(n)) {
            i += 2;
            continue;
        }
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
        if matches!(t.code, 5 | 105 | 320..=369 | 390..=399 | 480..=481 | 1005)
            && let Some(new) = Handle::parse_hex(&t.value).and_then(|h| map.get(&h))
        {
            t.value = new.hex();
        }
    }
}

fn remap_extra(extra: &mut ForeignExtra, map: &BTreeMap<Handle, Handle>) {
    extra.xdict = extra.xdict.map(|h| map.get(&h).copied().unwrap_or(h));
    for h in &mut extra.reactors {
        *h = map.get(h).copied().unwrap_or(*h);
    }
    remap_tags(&mut extra.xdata, map);
    remap_tags(&mut extra.props, map);
}

impl crate::Drawing {
    /// Retire a dimension's raw association after an explicit association command. Remove the
    /// owned graph and its dictionary/reactor edges, but keep unrelated extension-dictionary data.
    /// Returns whether a raw association was present.
    pub fn retire_raw_dimassoc(&mut self, dimension: Handle) -> bool {
        let roots: BTreeSet<_> = self
            .foreign
            .objects
            .iter()
            .filter(|o| o.kind == "DIMASSOC")
            .filter_map(|o| {
                let target = o
                    .tags
                    .iter()
                    .skip_while(|t| t.code != 100 || t.value.trim() != "AcDbDimAssoc")
                    .find(|t| t.code == 330)
                    .and_then(|t| Handle::parse_hex(&t.value));
                (target == Some(dimension)).then_some(o.handle)
            })
            .collect();
        if roots.is_empty() {
            return false;
        }
        let mut removed = owned_objects(&self.foreign.objects, roots.iter().copied());
        removed.extend(roots);
        let dictionary = self.foreign.entities.get(&dimension).and_then(|x| x.xdict);
        let foreign = std::sync::Arc::make_mut(&mut self.foreign);
        for object in &mut foreign.objects {
            strip_edges(&mut object.tags, matches!(object.kind.as_str(), "DICTIONARY" | "ACDBDICTIONARYWDFLT"), &removed);
        }
        if let Some(dictionary) = dictionary {
            let empty = foreign
                .objects
                .iter()
                .find(|o| o.handle == dictionary)
                .is_some_and(|o| o.kind == "DICTIONARY" && !o.tags.iter().any(|t| t.code == 3 || t.code >= 1000));
            let has_other_children = foreign.objects.iter().any(|o| owner(&o.tags) == Some(dictionary) && !removed.contains(&o.handle));
            if empty && !has_other_children {
                removed.insert(dictionary);
            }
        }
        foreign.objects.retain(|o| !removed.contains(&o.handle));
        for object in &mut foreign.objects {
            strip_edges(&mut object.tags, matches!(object.kind.as_str(), "DICTIONARY" | "ACDBDICTIONARYWDFLT"), &removed);
        }
        for extra in foreign.entities.values_mut().chain(foreign.records.values_mut()) {
            extra.xdict = extra.xdict.filter(|h| !removed.contains(h));
            extra.reactors.retain(|h| !removed.contains(h));
        }
        foreign.entities.retain(|_, extra| !extra.is_empty());
        foreign.records.retain(|_, extra| !extra.is_empty());
        true
    }

    /// Give a copied entity and its attributes independent foreign data. Owned object graphs
    /// are cloned; internal references follow the copy, while references to other geometry stay.
    pub fn copy_foreign_data(&mut self, source: Handle, copy: &mut crate::Entity) {
        let mut roots = BTreeMap::from([(source, copy.handle)]);
        if let crate::EntityKind::Insert(insert) = &mut copy.kind {
            for attribute in &mut insert.attribs {
                if let Some(old) = attribute.handle {
                    let new = self.new_handle();
                    attribute.handle = Some(new);
                    roots.insert(old, new);
                }
            }
        }
        let owned = owned_objects(&self.foreign.objects, roots.keys().copied());
        let mut map = roots.clone();
        for old in &owned {
            map.insert(*old, self.new_handle());
        }
        remap_tags(&mut copy.common.xdata, &map);
        let original = self.foreign.clone();
        let foreign = std::sync::Arc::make_mut(&mut self.foreign);
        for object in original.objects.iter().filter(|o| owned.contains(&o.handle)) {
            let Some(handle) = map.get(&object.handle).copied() else { continue };
            let mut cloned = ForeignObject { handle, ..object.clone() };
            remap_tags(&mut cloned.tags, &map);
            foreign.objects.push(cloned);
        }
        for (old, new) in roots {
            if let Some(mut extra) = original.entities.get(&old).cloned() {
                remap_extra(&mut extra, &map);
                foreign.entities.insert(new, extra);
            }
        }
    }
}

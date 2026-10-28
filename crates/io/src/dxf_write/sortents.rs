//! Explicit draw order: AutoCAD falls back to handle order, not DXF record order.

use super::{Entity, Handle, HashMap, W};

pub(super) struct Table {
    owner: String,
    dictionary: String,
    handle: String,
    order: Vec<Handle>,
}

/// Allocate only for spaces whose current draw order needs an override. Sort handles reuse
/// the space's existing entity handles as keys; they are not new object identities.
pub(super) fn plan<'a>(w: &mut W, tables: &mut Vec<Table>, owner: &str, entities: impl Iterator<Item = &'a Entity>) {
    let order: Vec<_> = entities.map(|e| e.handle).collect();
    if order.is_sorted() {
        return;
    }
    tables.push(Table { owner: owner.to_string(), dictionary: w.h(), handle: w.h(), order });
}

/// Declare the class and attach before raw passthrough, which merges any kept entries into
/// this same dictionary. No class is needed when every space is already in handle order.
pub(super) fn attach(w: &mut W, tables: &[Table]) {
    if tables.is_empty() {
        return;
    }
    let dictionaries: HashMap<_, _> = tables.iter().map(|t| (t.owner.as_str(), t.dictionary.as_str())).collect();
    let mut block_record = false;
    let mut section_start = false;
    let mut classes = false;
    for tag in std::mem::take(&mut w.t) {
        if section_start && tag.code == 2 {
            classes = tag.str() == "CLASSES";
            section_start = false;
        }
        if tag.code == 0 {
            block_record = tag.str() == "BLOCK_RECORD";
            section_start = tag.str() == "SECTION";
            if classes && tag.str() == "ENDSEC" {
                w.s(0, "CLASS");
                w.s(1, "SORTENTSTABLE");
                w.s(2, "AcDbSortentsTable");
                w.s(3, "ObjectDBX Classes");
                w.i(90, 0);
                w.i(280, 0);
                w.i(281, 0);
                classes = false;
            }
        }
        let dictionary = (block_record && tag.code == 5).then(|| tag.str()).and_then(|h| dictionaries.get(h.as_str()).copied());
        w.t.push(tag);
        if let Some(dictionary) = dictionary {
            w.group("ACAD_XDICTIONARY", 360, &[dictionary]);
        }
    }
}

pub(super) fn objects(w: &mut W, tables: &[Table]) {
    for table in tables {
        w.s(0, "DICTIONARY");
        w.s(5, &table.dictionary);
        w.group("ACAD_REACTORS", 330, &[&table.owner]);
        w.s(330, &table.owner);
        w.s(100, "AcDbDictionary");
        w.i(280, 1);
        w.i(281, 1);
        w.s(3, "ACAD_SORTENTS");
        w.s(360, &table.handle);
        w.s(0, "SORTENTSTABLE");
        w.s(5, &table.handle);
        w.group("ACAD_REACTORS", 330, &[&table.dictionary]);
        w.s(330, &table.dictionary);
        w.s(100, "AcDbSortentsTable");
        w.s(330, &table.owner);
        let mut sorted = table.order.clone();
        sorted.sort_unstable();
        for (entity, sort) in table.order.iter().zip(sorted) {
            w.s(331, entity.hex());
            w.s(5, sort.hex());
        }
    }
}

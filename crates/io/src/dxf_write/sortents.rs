//! Explicit draw order uses the final emitted records, not pre-lowering source identities.

use std::collections::BTreeSet;

use cadcraft_doc::foreign::SortTable;

use super::{Entity, Handle, HashMap, HashSet, Tag, W};

pub(super) struct Table {
    owner: Handle,
    dictionary: Handle,
    new_dictionary: bool,
    handle: String,
    /// Entity and sort handle, in the order the table lists them.
    entries: Vec<(Handle, u64)>,
}

/// Final records after foreign passthrough: dictionaries can be reused, and unknown entities
/// participate only when their raw record really survived.
pub(super) struct Written {
    entities: HashSet<Handle>,
    dictionaries: HashMap<Handle, Handle>,
}

fn records(tags: &[Tag]) -> impl Iterator<Item = (usize, &Tag, &[Tag])> {
    let mut rest = tags;
    let mut offset = 0;
    std::iter::from_fn(move || {
        let (head, tail) = rest.split_first()?;
        let length = tail.iter().position(|t| t.code == 0).unwrap_or(tail.len());
        let (body, next) = tail.split_at(length);
        let start = offset;
        offset += length + 1;
        rest = next;
        Some((start, head, body))
    })
}

fn handle(body: &[Tag]) -> Option<Handle> {
    body.iter().find(|t| matches!(t.code, 5 | 105)).and_then(|t| Handle::parse_hex(&t.str()))
}

fn extension_dictionary(body: &[Tag]) -> Option<Handle> {
    let mut extension = false;
    for tag in body.iter().take_while(|t| t.code < 1000) {
        if tag.code == 102 {
            extension = tag.str().trim() == "{ACAD_XDICTIONARY";
        } else if extension && tag.code == 360 {
            return Handle::parse_hex(&tag.str());
        }
    }
    None
}

pub(super) fn written(w: &mut W) -> Written {
    let mut out = Written { entities: HashSet::new(), dictionaries: HashMap::new() };
    let mut section = String::new();
    // Raw passthrough allocated above the writer's old seed. Continue from its final seed and
    // reserve every actual identity before allocating dictionaries/tables through W::h.
    if let Some(seed) = w.t.windows(2).find(|p| p[0].code == 9 && p[0].str() == "$HANDSEED").and_then(|p| Handle::parse_hex(&p[1].str())) {
        w.next = w.next.max(seed.0);
    }
    for (_, head, body) in records(&w.t) {
        let kind = head.str();
        if kind == "SECTION" {
            section = body.iter().find(|t| t.code == 2).map(Tag::str).unwrap_or_default();
        } else if kind == "ENDSEC" {
            section.clear();
        } else if section != "HEADER"
            && let Some(h) = handle(body)
        {
            w.used.insert(h);
            if matches!(section.as_str(), "ENTITIES" | "BLOCKS") && !matches!(kind.as_str(), "BLOCK" | "ENDBLK" | "VERTEX" | "ATTRIB" | "SEQEND") {
                out.entities.insert(h);
            } else if kind == "BLOCK_RECORD"
                && let Some(dictionary) = extension_dictionary(body)
            {
                out.dictionaries.insert(h, dictionary);
            }
        }
    }
    out
}

/// Plan the SORTENTSTABLE of one space. While the space draws in the order its source table gave,
/// that table is written back as it was, an empty one included (AutoCAD keeps every table). Any
/// other order that is not ascending by handle gets a table of its own, listed by entity handle.
pub(super) fn plan<'a>(
    w: &mut W,
    tables: &mut Vec<Table>,
    written: &Written,
    owner: &str,
    source: Option<&SortTable>,
    entities: impl Iterator<Item = &'a Entity>,
) {
    let mut order = Vec::new();
    for entity in entities {
        match w.emitted.get(&entity.handle).filter(|handles| !handles.is_empty()) {
            Some(handles) => order.extend(handles.iter().copied().filter(|h| written.entities.contains(h))),
            // Unknown entities are restored by raw passthrough, not the modelled entity writer.
            None if written.entities.contains(&entity.handle) => order.push(entity.handle),
            None => {}
        }
    }
    let kept = source.and_then(|table| {
        let members: BTreeSet<Handle> = order.iter().copied().collect();
        let entries = table.restricted(&members);
        (SortTable::draw_order(&entries, &members) == order).then_some(entries)
    });
    let entries = match kept {
        Some(entries) => entries,
        None if order.is_sorted() => return,
        None => {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            let mut entries: Vec<(Handle, u64)> = order.iter().copied().zip(sorted.into_iter().map(|h| h.0)).collect();
            entries.sort_unstable();
            entries
        }
    };
    let Some(owner) = Handle::parse_hex(owner) else { return };
    let existing = written.dictionaries.get(&owner).copied();
    let dictionary = existing.map(Handle::hex).unwrap_or_else(|| w.h());
    let Some(dictionary) = Handle::parse_hex(&dictionary) else { return };
    tables.push(Table { owner, dictionary, new_dictionary: existing.is_none(), handle: w.h(), entries });
}

fn objects(w: &mut W, tables: &[Table]) {
    for table in tables {
        if table.new_dictionary {
            w.s(0, "DICTIONARY");
            w.s(5, table.dictionary.hex());
            // AutoCAD's extension dictionaries of a block record have no reactor.
            w.s(330, table.owner.hex());
            w.s(100, "AcDbDictionary");
            w.i(280, 1);
            w.i(281, 1);
            w.s(3, "ACAD_SORTENTS");
            w.s(360, &table.handle);
        }
        w.s(0, "SORTENTSTABLE");
        w.s(5, &table.handle);
        w.group("ACAD_REACTORS", 330, &[&table.dictionary.hex()]);
        w.s(330, table.dictionary.hex());
        w.s(100, "AcDbSortentsTable");
        w.s(330, table.owner.hex());
        for (entity, sort) in &table.entries {
            w.s(331, entity.hex());
            w.s(5, Handle(*sort).hex());
        }
    }
}

/// Reuse an already restored extension dictionary, or attach exactly one new dictionary.
/// Insert by tag position in one pass, keeping all raw keys and moving rather than cloning tags.
pub(super) fn attach(w: &mut W, tables: &[Table]) {
    if tables.is_empty() {
        return;
    }
    let owners: HashMap<_, _> = tables.iter().map(|t| (t.owner, t)).collect();
    let dictionaries: HashMap<_, _> = tables.iter().filter(|t| !t.new_dictionary).map(|t| (t.dictionary, t)).collect();
    let has_class = records(&w.t).any(|(_, head, body)| head.str() == "CLASS" && body.iter().any(|t| t.code == 1 && t.str() == "SORTENTSTABLE"));
    let mut inserts: HashMap<usize, Vec<Tag>> = HashMap::new();
    let mut section = String::new();
    for (start, head, body) in records(&w.t) {
        let kind = head.str();
        if kind == "SECTION" {
            section = body.iter().find(|t| t.code == 2).map(Tag::str).unwrap_or_default();
        } else if kind == "BLOCK_RECORD"
            && let Some(table) = handle(body).and_then(|h| owners.get(&h)).filter(|t| t.new_dictionary)
        {
            if let Some(index) = body.iter().position(|t| t.code == 5) {
                inserts.entry(start + index + 2).or_default().extend([
                    Tag::s(102, "{ACAD_XDICTIONARY"),
                    Tag::s(360, table.dictionary.hex()),
                    Tag::s(102, "}"),
                ]);
            }
        } else if cadcraft_doc::foreign::is_dictionary(&kind)
            && let Some(table) = handle(body).and_then(|h| dictionaries.get(&h))
        {
            let at = body.iter().position(|t| t.code == 1001).unwrap_or(body.len());
            inserts.entry(start + at + 1).or_default().extend([Tag::s(3, "ACAD_SORTENTS"), Tag::s(360, &table.handle)]);
        } else if kind == "ENDSEC" {
            if section == "CLASSES" && !has_class {
                inserts.entry(start).or_default().extend([
                    Tag::s(0, "CLASS"),
                    Tag::s(1, "SORTENTSTABLE"),
                    Tag::s(2, "AcDbSortentsTable"),
                    Tag::s(3, "ObjectDBX Classes"),
                    Tag::i(90, 0),
                    Tag::i(280, 0),
                    Tag::i(281, 0),
                ]);
            } else if section == "OBJECTS" {
                let mut extra = W::default();
                objects(&mut extra, tables);
                inserts.entry(start).or_default().extend(extra.t);
            }
            section.clear();
        }
    }
    for (index, tag) in std::mem::take(&mut w.t).into_iter().enumerate() {
        if let Some(extra) = inserts.remove(&index) {
            w.t.extend(extra);
        }
        w.t.push(tag);
    }
}

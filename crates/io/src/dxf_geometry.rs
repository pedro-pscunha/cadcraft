//! The geometry groups of a source entity record, and their replacement in a written record (see
//! `cadcraft_doc::foreign::GeometrySnapshot`).
//!
//! Each entity type has the groups that make up its geometry in the subclasses the DXF Reference
//! puts them in: points, sizes, angles, the entity plane (210/220/230), thickness. A subclass is
//! named by its `100` marker and, when a type has the same marker twice (TEXT), by its place among
//! them. The groups are kept in file order, each subclass after its marker, and replace the groups
//! the writer made from the model for that subclass.

use std::collections::{HashMap, HashSet};

use cadcraft_doc::foreign::{Foreign, GeometryRecord, GeometrySnapshot, attrib_geometry_key, geometry_key};
use cadcraft_doc::{Drawing, Handle, RawTag};
use cadcraft_dxf::Tag;

type Spec = &'static [(&'static str, &'static [i32])];

/// The geometry groups of the types that keep a snapshot, by subclass in record order.
fn spec(kind: &str) -> Option<Spec> {
    Some(match kind {
        "LINE" => &[("AcDbLine", &[39, 10, 20, 30, 11, 21, 31, 210, 220, 230])],
        "POINT" => &[("AcDbPoint", &[39, 10, 20, 30, 50, 210, 220, 230])],
        "CIRCLE" => &[("AcDbCircle", &[39, 10, 20, 30, 40, 210, 220, 230])],
        "ARC" => &[("AcDbCircle", &[39, 10, 20, 30, 40, 210, 220, 230]), ("AcDbArc", &[50, 51])],
        "SOLID" | "TRACE" => &[("AcDbTrace", &[39, 10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 210, 220, 230])],
        "TEXT" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbText", &[73])],
        "ATTRIB" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbAttribute", &[74])],
        "ATTDEF" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbAttributeDefinition", &[74])],
        "MTEXT" => &[("AcDbMText", &[10, 20, 30, 40, 41, 71, 72, 11, 21, 31, 50, 73, 44, 210, 220, 230])],
        "INSERT" => &[("AcDbBlockReference", &[10, 20, 30, 41, 42, 43, 50, 70, 71, 44, 45, 210, 220, 230])],
        _ => return None,
    })
}

/// The groups of the `n`-th subclass `marker` of `tags` (before the xdata): the range after the
/// marker group up to the next marker.
fn subclass(tags: &[Tag], marker: &str, n: usize) -> Option<std::ops::Range<usize>> {
    let end = tags.iter().position(|t| t.code >= 1000).unwrap_or(tags.len());
    let at = tags.iter().take(end).enumerate().filter(|(_, t)| t.code == 100 && t.str().trim() == marker).map(|(i, _)| i).nth(n)?;
    let stop = tags.iter().take(end).enumerate().skip(at + 1).find(|(_, t)| t.code == 100).map_or(end, |(i, _)| i);
    Some(at + 1..stop)
}

/// The place of the subclass of `spec` entry `i` among the entries with its marker.
fn occurrence(spec: Spec, i: usize) -> usize {
    spec.iter().take(i).filter(|(m, _)| *m == spec[i].0).count()
}

/// The geometry groups of a source record of `kind`, each subclass after a `100` marker group;
/// `None` for a type without a snapshot or a record that lacks one of the subclasses.
pub(crate) fn capture(kind: &str, tags: &[Tag]) -> Option<Vec<RawTag>> {
    let spec = spec(kind)?;
    let mut out = Vec::new();
    for (i, (marker, codes)) in spec.iter().enumerate() {
        let range = subclass(tags, marker, occurrence(spec, i))?;
        out.push(RawTag { code: 100, value: (*marker).to_string() });
        out.extend(tags.get(range).unwrap_or_default().iter().filter(|t| codes.contains(&t.code)).map(|t| RawTag { code: t.code, value: t.str() }));
    }
    Some(out)
}

/// The record `generated` with its geometry groups replaced by `kept` (see [`capture`]); `None`
/// when the record lacks one of the subclasses. `convert` turns a kept group into a tag.
pub(crate) fn restore(kind: &str, generated: &[Tag], kept: &[RawTag], convert: impl Fn(&RawTag) -> Tag) -> Option<Vec<Tag>> {
    let spec = spec(kind)?;
    let mut parts = kept.split(|t| t.code == 100).skip(1);
    // For each entry: the subclass's range, where its replacement goes (the first geometry group
    // the writer made, else the end of the subclass), its geometry codes and the kept groups.
    let mut edits = Vec::new();
    for (i, (marker, codes)) in spec.iter().enumerate() {
        let range = subclass(generated, marker, occurrence(spec, i))?;
        let at = generated.get(range.clone())?.iter().position(|t| codes.contains(&t.code)).map_or(range.end, |p| range.start + p);
        edits.push((range, at, *codes, parts.next()?));
    }
    let mut out = Vec::with_capacity(generated.len() + kept.len());
    for (i, t) in generated.iter().enumerate() {
        for (.., groups) in edits.iter().filter(|e| e.1 == i) {
            out.extend(groups.iter().map(&convert));
        }
        if edits.iter().any(|(range, _, codes, _)| range.contains(&i) && codes.contains(&t.code)) {
            continue;
        }
        out.push(t.clone());
    }
    for (.., groups) in edits.iter().filter(|e| e.1 >= generated.len()) {
        out.extend(groups.iter().map(&convert));
    }
    Some(out)
}

/// The geometry keys of the entities and attributes of `d` whose handle `wanted` accepts.
fn keys(d: &Drawing, wanted: impl Fn(Handle) -> bool) -> HashMap<Handle, String> {
    let mut out = HashMap::new();
    let all = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    for e in all.flat_map(|s| s.iter()) {
        if wanted(e.handle)
            && let Some(key) = geometry_key(&e.kind, &e.common)
        {
            out.insert(e.handle, key);
        }
        for a in e.kind.attributes() {
            if let Some(h) = a.handle.filter(|h| wanted(*h)) {
                out.insert(h, attrib_geometry_key(a));
            }
        }
    }
    out
}

fn handle_of(tags: &[Tag]) -> Option<Handle> {
    tags.iter().find(|t| matches!(t.code, 5 | 105)).and_then(|t| Handle::parse_hex(&t.str()))
}

/// Keep the geometry groups of the source `records` (type and groups after it) of the entities of
/// `d` that keep a snapshot, in the entities' foreign data. A handle that two records share names
/// no entity.
pub(crate) fn capture_all<'a>(f: &mut Foreign, d: &Drawing, records: impl Iterator<Item = (&'a str, &'a [Tag])>) {
    let records: Vec<_> = records.filter_map(|(kind, tags)| Some((kind, tags, handle_of(tags)?))).collect();
    let mut seen: HashMap<Handle, usize> = HashMap::new();
    for (.., h) in &records {
        *seen.entry(*h).or_default() += 1;
    }
    let keys = keys(d, |h| seen.get(&h) == Some(&1));
    for (kind, tags, h) in records {
        if let Some(model) = keys.get(&h)
            && let Some(groups) = capture(kind, tags)
        {
            let record = GeometryRecord { kind: kind.to_string(), groups };
            f.entities.entry(h).or_default().geometry = Some(GeometrySnapshot { records: vec![record], model: model.clone() });
        }
    }
}

/// The entities and attributes whose snapshot (in `f`) still describes their geometry.
pub(crate) fn eligible(f: &Foreign, d: &Drawing) -> HashSet<Handle> {
    let snapshots: HashMap<Handle, &str> = f.entities.iter().filter_map(|(h, x)| Some((*h, x.geometry.as_ref()?.model.as_str()))).collect();
    if snapshots.is_empty() {
        return HashSet::new();
    }
    keys(d, |h| snapshots.contains_key(&h)).into_iter().filter(|(h, key)| snapshots.get(h) == Some(&key.as_str())).map(|(h, _)| h).collect()
}

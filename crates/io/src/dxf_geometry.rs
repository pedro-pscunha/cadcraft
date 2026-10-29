//! The geometry groups of a source entity record, and their replacement in a written record (see
//! `cadcraft_doc::foreign::GeometrySnapshot`).
//!
//! Each entity type has the groups that make up its geometry in the subclasses the DXF Reference
//! puts them in: points, sizes, angles, the entity plane (210/220/230), thickness. A subclass is
//! named by its `100` marker and, when a type has the same marker twice (TEXT), by its place among
//! them. The groups are kept in file order, each subclass after its marker, and replace the groups
//! the writer made from the model for that subclass.

use std::collections::HashMap;
use std::sync::Arc;

use cadcraft_doc::{Drawing, Entity, GeometryRecord, GeometrySnapshot, Handle, RawTag, attrib_geometry_key, geometry_key};
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
        // The whole curve: flags, degree, counts, tolerances, tangents, knots, weights, controls, fits and the plane.
        "SPLINE" => &[("AcDbSpline", &[210, 220, 230, 70, 71, 72, 73, 74, 42, 43, 44, 12, 22, 32, 13, 23, 33, 40, 41, 10, 20, 30, 11, 21, 31])],
        // The vertices and the plane of the leader, and the annotation's size and offsets (the style, the
        // arrow, the path type and the link to the annotation are the model's and `subclass_groups`').
        "LEADER" => &[("AcDbLeader", &[76, 10, 20, 30, 210, 220, 230, 211, 221, 231, 212, 222, 232, 213, 223, 233, 40, 41])],
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
    if kind == "HATCH" {
        return hatch_capture(tags);
    }
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
    if kind == "HATCH" {
        return hatch_restore(generated, kept, convert);
    }
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

/// A HATCH record: the groups before its first 91 (loop count) hold the elevation point and the
/// plane, the groups from the 91 to the 75 (style) are the boundary loops, and the groups from the
/// 75 on are the pattern (angle, scale, definition lines) and the seed points, then the gradient.
/// A loop ends with its source objects (97 count, 330 handles), which are references and stay
/// as the writer made them.
const HATCH_HEAD: &[i32] = &[10, 20, 30, 210, 220, 230];
const HATCH_TAIL: &[i32] = &[75, 76, 52, 41, 77, 78, 53, 43, 44, 45, 46, 79, 49, 47, 98, 10, 20];

/// The place of the 91 and of the 75 after it in a hatch's groups (the end when it has none).
fn hatch_parts(codes: &[i32]) -> Option<(usize, usize)> {
    let loops = codes.iter().position(|c| *c == 91)?;
    let tail = codes.iter().skip(loops).position(|c| *c == 75).map_or(codes.len(), |p| loops + p);
    Some((loops, tail))
}

/// The loops of the groups `codes[loops..tail]` (after the 91): where each starts (a 92), and where its source
/// objects (97) begin.
fn hatch_loops(codes: &[i32], loops: usize, tail: usize) -> Vec<(usize, usize, usize)> {
    let starts: Vec<usize> = (loops + 1..tail).filter(|i| codes.get(*i) == Some(&92)).collect();
    starts
        .iter()
        .enumerate()
        .map(|(n, start)| {
            let end = starts.get(n + 1).copied().unwrap_or(tail);
            let sources = (*start..end).find(|i| codes.get(*i) == Some(&97)).unwrap_or(end);
            (*start, sources, end)
        })
        .collect()
}

fn hatch_capture(tags: &[Tag]) -> Option<Vec<RawTag>> {
    let range = subclass(tags, "AcDbHatch", 0)?;
    let groups = tags.get(range)?;
    let codes: Vec<i32> = groups.iter().map(|t| t.code).collect();
    let (loops, tail) = hatch_parts(&codes)?;
    let raw = |t: &Tag| RawTag { code: t.code, value: t.str() };
    let mut out = vec![RawTag { code: 100, value: "AcDbHatch".into() }];
    out.extend(groups.iter().take(loops).filter(|t| HATCH_HEAD.contains(&t.code)).map(raw));
    out.extend(groups.iter().take(tail).skip(loops).map(raw));
    out.extend(groups.iter().skip(tail).filter(|t| HATCH_TAIL.contains(&t.code)).map(raw));
    Some(out)
}

fn hatch_restore(generated: &[Tag], kept: &[RawTag], convert: impl Fn(&RawTag) -> Tag) -> Option<Vec<Tag>> {
    let range = subclass(generated, "AcDbHatch", 0)?;
    let groups = generated.get(range.clone())?;
    let kept: Vec<RawTag> = kept.iter().skip(1).cloned().collect();
    let codes: Vec<i32> = groups.iter().map(|t| t.code).collect();
    let kept_codes: Vec<i32> = kept.iter().map(|t| t.code).collect();
    let ((loops, tail), (k_loops, k_tail)) = (hatch_parts(&codes)?, hatch_parts(&kept_codes)?);
    let (own, kept_own) = (hatch_loops(&codes, loops, tail), hatch_loops(&kept_codes, k_loops, k_tail));
    if own.len() != kept_own.len() || codes.get(loops) != kept_codes.get(k_loops) {
        return None;
    }
    let mut out: Vec<Tag> = generated.iter().take(range.start).cloned().collect();
    // Before the loops: the generated groups but the elevation point and the plane, which the kept ones replace.
    out.extend(kept.iter().take(k_loops).map(&convert));
    out.extend(groups.iter().take(loops).filter(|t| !HATCH_HEAD.contains(&t.code)).cloned());
    // The loops: the kept boundary data, then the generated source objects of the loop.
    out.push(groups.get(loops)?.clone());
    for ((_, own_sources, own_end), (k_start, k_sources, _)) in own.iter().zip(&kept_own) {
        out.extend(kept.iter().take(*k_sources).skip(*k_start).map(&convert));
        out.extend(groups.iter().take(*own_end).skip(*own_sources).cloned());
    }
    // The pattern and the seed points: the kept ones; then what the writer made after them (the gradient).
    out.extend(kept.iter().skip(k_tail).map(&convert));
    out.extend(groups.iter().skip(tail).filter(|t| !HATCH_TAIL.contains(&t.code)).cloned());
    out.extend(generated.iter().skip(range.end).cloned());
    Some(out)
}

/// The geometry keys of the entities and attributes of `d` whose handle `wanted` accepts.
fn keys(d: &Drawing, wanted: impl Fn(Handle) -> bool) -> HashMap<Handle, String> {
    let mut out = HashMap::new();
    for e in all_entities(d) {
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

/// Every entity of the drawing (model space, layouts, blocks).
fn all_entities(d: &Drawing) -> impl Iterator<Item = &Entity> {
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    stores.flat_map(|s| s.iter().map(|e| e.as_ref()))
}

fn handle_of(tags: &[Tag]) -> Option<Handle> {
    tags.iter().find(|t| matches!(t.code, 5 | 105)).and_then(|t| Handle::parse_hex(&t.str()))
}

/// Give the entities and attributes of `d` the geometry snapshot of their source `records` (type
/// and groups after it). A handle that two records share names no entity.
pub(crate) fn capture_all<'a>(d: &mut Drawing, records: impl Iterator<Item = (&'a str, &'a [Tag])>) {
    let records: Vec<_> = records.filter_map(|(kind, tags)| Some((kind, tags, handle_of(tags)?))).collect();
    let mut seen: HashMap<Handle, usize> = HashMap::new();
    for (.., h) in &records {
        *seen.entry(*h).or_default() += 1;
    }
    let keys = keys(d, |h| seen.get(&h) == Some(&1));
    let mut snapshots: HashMap<Handle, Arc<GeometrySnapshot>> = HashMap::new();
    for (kind, tags, h) in records {
        if let Some(model) = keys.get(&h)
            && let Some(groups) = capture(kind, tags)
        {
            let record = GeometryRecord { kind: kind.to_string(), groups };
            snapshots.insert(h, Arc::new(GeometrySnapshot { records: vec![record], model: model.clone() }));
        }
    }
    if snapshots.is_empty() {
        return;
    }
    let give = |store: &mut cadcraft_doc::EntityStore| {
        let hs: Vec<Handle> = store
            .iter()
            .filter(|e| snapshots.contains_key(&e.handle) || e.kind.attributes().iter().any(|a| a.handle.is_some_and(|h| snapshots.contains_key(&h))))
            .map(|e| e.handle)
            .collect();
        for h in hs {
            store.modify(h, |e| {
                e.common.geometry = snapshots.get(&e.handle).cloned();
                for a in e.kind.attributes_mut() {
                    a.geometry = a.handle.and_then(|h| snapshots.get(&h)).cloned();
                }
            });
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

/// The snapshots of the entities and attributes of `d` that still describe their geometry.
pub(crate) fn eligible(d: &Drawing) -> HashMap<Handle, Arc<GeometrySnapshot>> {
    let mut out = HashMap::new();
    for e in all_entities(d) {
        if let Some(g) = e.common.geometry.as_ref()
            && geometry_key(&e.kind, &e.common).as_deref() == Some(g.model.as_str())
        {
            out.insert(e.handle, g.clone());
        }
        for a in e.kind.attributes() {
            if let (Some(h), Some(g)) = (a.handle, a.geometry.as_ref())
                && attrib_geometry_key(a) == g.model
            {
                out.insert(h, g.clone());
            }
        }
    }
    out
}

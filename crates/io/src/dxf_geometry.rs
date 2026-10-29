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

/// The geometry groups of a DIMENSION's own `AcDbDimension` subclass: definition point, text point,
/// text rotation, plane and the horizontal direction.
const DIMENSION: &[i32] = &[10, 20, 30, 11, 21, 31, 53, 51, 210, 220, 230];
const INSERT: &[i32] = &[10, 20, 30, 41, 42, 43, 50, 70, 71, 44, 45, 210, 220, 230];

/// The geometry groups of the types that keep a snapshot, by subclass in record order.
fn spec(kind: &str, tags: &[Tag]) -> Option<Spec> {
    let has = |marker: &str| tags.iter().take_while(|t| t.code < 1000).any(|t| t.code == 100 && t.str().trim() == marker);
    Some(match kind {
        "LINE" => &[("AcDbLine", &[39, 10, 20, 30, 11, 21, 31, 210, 220, 230])],
        "POINT" => &[("AcDbPoint", &[39, 10, 20, 30, 50, 210, 220, 230])],
        "CIRCLE" => &[("AcDbCircle", &[39, 10, 20, 30, 40, 210, 220, 230])],
        "ARC" => &[("AcDbCircle", &[39, 10, 20, 30, 40, 210, 220, 230]), ("AcDbArc", &[50, 51])],
        "SOLID" | "TRACE" => &[("AcDbTrace", &[39, 10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 210, 220, 230])],
        "TEXT" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbText", &[73])],
        "ATTRIB" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbAttribute", &[74])],
        "ATTDEF" => &[("AcDbText", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230]), ("AcDbAttributeDefinition", &[74])],
        // Only the main groups: a column text has an embedded object (101) with a second set of the same codes,
        // which the generated record owns.
        "MTEXT" => &[("AcDbMText", &[10, 20, 30, 40, 41, 71, 72, 11, 21, 31, 50, 73, 44, 210, 220, 230])],
        "INSERT" if has("AcDbMInsertBlock") => &[("AcDbMInsertBlock", INSERT)],
        "INSERT" => &[("AcDbBlockReference", INSERT)],
        // The whole curve: flags, degree, counts, tolerances, tangents, knots, weights, controls, fits and the plane.
        "SPLINE" => &[("AcDbSpline", &[210, 220, 230, 70, 71, 72, 73, 74, 42, 43, 44, 12, 22, 32, 13, 23, 33, 40, 41, 10, 20, 30, 11, 21, 31])],
        // The vertices and the plane of the leader, and the annotation's size and offsets (the style, the
        // arrow, the path type and the link to the annotation are the model's and `subclass_groups`').
        "LEADER" => &[("AcDbLeader", &[76, 10, 20, 30, 210, 220, 230, 211, 221, 231, 212, 222, 232, 213, 223, 233, 40, 41])],
        // The frame's position, direction and plane; its text and style are the model's.
        "TOLERANCE" => &[("AcDbFcf", &[10, 20, 30, 11, 21, 31, 210, 220, 230])],
        // A helix is a spline through points on it, then the helix data.
        "HELIX" => &[
            ("AcDbSpline", &[210, 220, 230, 70, 71, 72, 73, 74, 42, 43, 44, 12, 22, 32, 13, 23, 33, 40, 41, 10, 20, 30, 11, 21, 31]),
            ("AcDbHelix", &[90, 91, 10, 20, 30, 11, 21, 31, 12, 22, 32, 40, 41, 42, 290, 280]),
        ],
        // The image frame (origin, U and V vectors, size) and the clipping boundary; the image link, the display
        // flags and the brightness are the model's.
        "WIPEOUT" => &[("AcDbWipeout", &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 71, 91, 14, 24])],
        // The definition point, text position and rotation, the plane and the definition points of the kind
        // of dimension. The block, the text, the style and the measurement stay what the writer makes.
        "DIMENSION" if has("AcDbAlignedDimension") && has("AcDbRotatedDimension") => {
            &[("AcDbDimension", DIMENSION), ("AcDbAlignedDimension", &[13, 23, 33, 14, 24, 34, 50, 52]), ("AcDbRotatedDimension", &[])]
        }
        "DIMENSION" if has("AcDbAlignedDimension") => &[("AcDbDimension", DIMENSION), ("AcDbAlignedDimension", &[13, 23, 33, 14, 24, 34, 50, 52])],
        "DIMENSION" if has("AcDbRadialDimension") => &[("AcDbDimension", DIMENSION), ("AcDbRadialDimension", &[15, 25, 35, 40])],
        "DIMENSION" if has("AcDbDiametricDimension") => &[("AcDbDimension", DIMENSION), ("AcDbDiametricDimension", &[15, 25, 35, 40])],
        "DIMENSION" if has("AcDb2LineAngularDimension") => {
            &[("AcDbDimension", DIMENSION), ("AcDb2LineAngularDimension", &[13, 23, 33, 14, 24, 34, 15, 25, 35, 16, 26, 36])]
        }
        "DIMENSION" if has("AcDb3PointAngularDimension") => {
            &[("AcDbDimension", DIMENSION), ("AcDb3PointAngularDimension", &[13, 23, 33, 14, 24, 34, 15, 25, 35])]
        }
        "DIMENSION" if has("AcDbOrdinateDimension") => &[("AcDbDimension", DIMENSION), ("AcDbOrdinateDimension", &[13, 23, 33, 14, 24, 34])],
        // The insertion point and plane, the direction and the row heights and column widths; the cells, their text,
        // merges and rotations stay what the writer makes.
        "ACAD_TABLE" => &[("AcDbBlockReference", &[10, 20, 30, 210, 220, 230]), ("AcDbTable", &[11, 21, 31, 141, 142])],
        "LWPOLYLINE" => &[("AcDbPolyline", &[90, 70, 43, 38, 39, 10, 20, 40, 41, 42, 91, 210, 220, 230])],
        // A POLYLINE is a 2D polyline (with VERTEX children), a 3D polyline or a mesh (their parents).
        "POLYLINE" if has("AcDb2dPolyline") => &[("AcDb2dPolyline", &[10, 20, 30, 39, 70, 40, 41, 71, 72, 73, 74, 75, 210, 220, 230])],
        "POLYLINE" if has("AcDb3dPolyline") => &[("AcDb3dPolyline", &[10, 20, 30, 39, 70, 210, 220, 230])],
        "POLYLINE" if has("AcDbPolyFaceMesh") => &[("AcDbPolyFaceMesh", &[10, 20, 30, 39, 70, 71, 72, 73, 74, 75, 210, 220, 230])],
        "POLYLINE" if has("AcDbPolygonMesh") => &[("AcDbPolygonMesh", &[10, 20, 30, 39, 70, 71, 72, 73, 74, 75, 210, 220, 230])],
        "VERTEX" if has("AcDb2dVertex") => &[("AcDb2dVertex", &[10, 20, 30, 40, 41, 42, 70, 50, 91])],
        _ => return None,
    })
}

/// The groups of the `n`-th subclass `marker` of `tags` (before the xdata): the range after the
/// marker group up to the next marker.
fn subclass(tags: &[Tag], marker: &str, n: usize) -> Option<std::ops::Range<usize>> {
    let end = tags.iter().position(|t| t.code >= 1000).unwrap_or(tags.len());
    let at = tags.iter().take(end).enumerate().filter(|(_, t)| t.code == 100 && t.str().trim() == marker).map(|(i, _)| i).nth(n)?;
    // The subclass ends at the next marker, or at an embedded object (101), which has groups of its own
    // with the same codes (an MTEXT's or an attribute's).
    let stop = tags.iter().take(end).enumerate().skip(at + 1).find(|(_, t)| matches!(t.code, 100 | 101)).map_or(end, |(i, _)| i);
    Some(at + 1..stop)
}

/// The groups of the embedded object of a multiline attribute (`101 Embedded Object` up to the xdata): its MTEXT.
const EMBEDDED: &[i32] = &[10, 20, 30, 40, 41, 46, 71, 72, 11, 21, 31, 50, 73, 44, 210, 220, 230];

/// The range of the groups of the embedded object of `tags`.
fn embedded(tags: &[Tag]) -> Option<std::ops::Range<usize>> {
    let end = tags.iter().position(|t| t.code >= 1000).unwrap_or(tags.len());
    let at = tags.iter().take(end).position(|t| t.code == 101)?;
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
    if matches!(kind, "MULTILEADER" | "MLEADER") {
        return nested_capture(tags, "AcDbMLeader");
    }

    let spec = spec(kind, tags)?;
    let mut out = Vec::new();
    for (i, (marker, codes)) in spec.iter().enumerate() {
        let range = subclass(tags, marker, occurrence(spec, i))?;
        out.push(RawTag { code: 100, value: (*marker).to_string() });
        // AutoCAD writes no oblique angle (52) of a dimension for extension lines perpendicular to its line and refuses a DXF that has one.
        let perpendicular = |t: &Tag| kind == "DIMENSION" && t.code == 52 && (t.f64().rem_euclid(180.0) - 90.0).abs() < 1e-9;
        out.extend(
            tags.get(range)
                .unwrap_or_default()
                .iter()
                .filter(|t| codes.contains(&t.code) && !perpendicular(t))
                .map(|t| RawTag { code: t.code, value: t.str() }),
        );
    }
    // A multiline attribute's embedded MTEXT, after a `101` group.
    if matches!(kind, "ATTRIB" | "ATTDEF")
        && let Some(range) = embedded(tags)
    {
        out.push(RawTag { code: 101, value: "Embedded Object".into() });
        out.extend(
            tags.get(range).unwrap_or_default().iter().filter(|t| EMBEDDED.contains(&t.code)).map(|t| RawTag { code: t.code, value: t.str() }),
        );
    }
    Some(out)
}

/// The record `generated` with its geometry groups replaced by `kept` (see [`capture`]); `None`
/// when the record lacks one of the subclasses. `convert` turns a kept group into a tag.
pub(crate) fn restore(kind: &str, generated: &[Tag], kept: &[RawTag], version: u32, convert: impl Fn(&RawTag) -> Tag) -> Option<Vec<Tag>> {
    if kind == "HATCH" {
        return hatch_restore(generated, kept, version, convert);
    }
    if matches!(kind, "MULTILEADER" | "MLEADER") {
        return mleader_restore(generated, kept, convert);
    }

    let spec = spec(kind, generated)?;
    // The embedded MTEXT of a multiline attribute follows its `101` group in the kept groups.
    let (kept, embedded_kept) = match kept.iter().position(|t| t.code == 101) {
        Some(at) => (kept.get(..at)?, kept.get(at + 1..)),
        None => (kept, None),
    };
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
    // The embedded MTEXT, when the writer made one too.
    if let Some(groups) = embedded_kept
        && let Some(range) = embedded(&out)
    {
        let at = out.get(range.clone())?.iter().position(|t| EMBEDDED.contains(&t.code)).map_or(range.end, |p| range.start + p);
        let mut next = Vec::with_capacity(out.len() + groups.len());
        for (i, t) in out.iter().enumerate() {
            if i == at {
                next.extend(groups.iter().map(&convert));
            }
            if !(range.contains(&i) && EMBEDDED.contains(&t.code)) {
                next.push(t.clone());
            }
        }
        if at >= out.len() {
            next.extend(groups.iter().map(&convert));
        }
        out = next;
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
            // The last 97 of the loop is its source object count: a spline edge has a fit data count (97) too.
            let sources = (*start..end).rfind(|i| codes.get(*i) == Some(&97)).unwrap_or(end);
            (*start, sources, end)
        })
        .collect()
}

/// The boundary groups of a hatch with the fit data count (97) every spline edge has after its control
/// points. A DXF made from a DWG by acadrust leaves it out, and AutoCAD takes the loop's source object count (97)
/// that follows for it ("expected group 11"). The fit count is a 97 that is followed by fit points (11), by
/// the next edge (72) or by the loop's own count (97); the loop's own count is followed by its objects (330)
/// or by the next loop (92) or the style (75).
fn with_fit_counts(groups: Vec<RawTag>) -> Vec<RawTag> {
    let code = |i: usize| groups.get(i).map(|g| g.code);
    let int = |i: usize| groups.get(i).and_then(|g| g.value.trim().parse::<usize>().ok()).unwrap_or(0);
    let mut out = Vec::with_capacity(groups.len() + 4);
    let mut i = 0;
    while i < groups.len() {
        if !(groups[i].code == 72 && groups[i].value.trim() == "4" && code(i + 1) == Some(94)) {
            out.push(groups[i].clone());
            i += 1;
            continue;
        }
        // 72 4, 94 degree, 73 rational, 74 periodic, 95 knots, 96 controls, the knots (40), the controls (10, 20, 42 when rational).
        let (rational, knots, controls) = (int(i + 2) != 0, int(i + 4), int(i + 5));
        let head = 6;
        let per_control = if rational { 3 } else { 2 };
        let end = (i + head + knots + controls * per_control).min(groups.len());
        out.extend(groups[i..end].iter().cloned());
        i = end;
        let counted = code(i) == Some(97) && matches!(code(i + 1), Some(11 | 97 | 72));
        if !counted {
            out.push(RawTag { code: 97, value: "0".into() });
        }
    }
    out
}

/// The boundary groups of a hatch for a file before 2010: its spline edges have no fit data count (97). Edges with
/// fit points cannot be written so; `None` then.
fn without_fit_counts(groups: Vec<RawTag>) -> Option<Vec<RawTag>> {
    let code = |i: usize| groups.get(i).map(|g| g.code);
    let int = |i: usize| groups.get(i).and_then(|g| g.value.trim().parse::<usize>().ok()).unwrap_or(0);
    let mut out = Vec::with_capacity(groups.len());
    let mut i = 0;
    while i < groups.len() {
        if !(groups[i].code == 72 && groups[i].value.trim() == "4" && code(i + 1) == Some(94)) {
            out.push(groups[i].clone());
            i += 1;
            continue;
        }
        let (rational, knots, controls) = (int(i + 2) != 0, int(i + 4), int(i + 5));
        let end = (i + 6 + knots + controls * if rational { 3 } else { 2 }).min(groups.len());
        out.extend(groups[i..end].iter().cloned());
        i = end;
        if code(i) == Some(97) && matches!(code(i + 1), Some(11 | 97 | 72)) {
            if int(i) != 0 {
                return None;
            }
            i += 1;
        }
    }
    Some(out)
}

fn hatch_capture(tags: &[Tag]) -> Option<Vec<RawTag>> {
    let range = subclass(tags, "AcDbHatch", 0)?;
    let groups = tags.get(range)?;
    let codes: Vec<i32> = groups.iter().map(|t| t.code).collect();
    let (loops, tail) = hatch_parts(&codes)?;
    let raw = |t: &Tag| RawTag { code: t.code, value: t.str() };
    let mut out = vec![RawTag { code: 100, value: "AcDbHatch".into() }];
    out.extend(groups.iter().take(loops).filter(|t| HATCH_HEAD.contains(&t.code)).map(raw));
    out.extend(with_fit_counts(groups.iter().take(tail).skip(loops).map(raw).collect()));
    out.extend(groups.iter().skip(tail).filter(|t| HATCH_TAIL.contains(&t.code)).map(raw));
    Some(out)
}

fn hatch_restore(generated: &[Tag], kept: &[RawTag], version: u32, convert: impl Fn(&RawTag) -> Tag) -> Option<Vec<Tag>> {
    let range = subclass(generated, "AcDbHatch", 0)?;
    let groups = generated.get(range.clone())?;
    let kept: Vec<RawTag> = kept.iter().skip(1).cloned().collect();
    // The fit data count of a spline edge exists from AutoCAD 2010 (AC1024) on.
    let kept = if version < 1024 { without_fit_counts(kept)? } else { kept };
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

/// Records whose geometry sits in nested sections that reuse group codes (`300 CONTEXT_DATA{` ...
/// `301 }`, ...): the groups of the subclass are kept whole, and a save overwrites the values of the
/// geometry groups of the record the writer made with those of the source, in order, when both
/// have the same geometry groups at the same depths (else the source is not replayed).
fn nested_capture(tags: &[Tag], marker: &str) -> Option<Vec<RawTag>> {
    let range = subclass(tags, marker, 0)?;
    let mut out = vec![RawTag { code: 100, value: marker.to_string() }];
    out.extend(tags.get(range)?.iter().map(|t| RawTag { code: t.code, value: t.str() }));
    Some(out)
}

/// The depth of each group of a nested record, and whether it opens (`{`) or closes (`}`) a section.
fn depths(groups: impl Iterator<Item = (i32, String)>) -> Vec<usize> {
    let mut depth = 0usize;
    groups
        .map(|(code, value)| {
            let value = value.trim();
            if matches!(code, 301 | 303 | 305) && value == "}" {
                depth = depth.saturating_sub(1);
            }
            let here = depth;
            if matches!(code, 300 | 302 | 304) && value.ends_with('{') {
                depth += 1;
            }
            here
        })
        .collect()
}

/// The geometry groups of a MULTILEADER's content context (depth 1); its leader roots are replayed whole
/// by [`mleader_restore`].
fn mleader_geometry(depth: usize, code: i32) -> bool {
    depth == 1 && matches!(code, 10..=16 | 20..=26 | 30..=36 | 42 | 43 | 46 | 47 | 110..=112 | 120..=122 | 130..=132)
}

/// The leader roots of a MULTILEADER (`302 LEADER{` ... `303 }`, one after the other in the content
/// context): the range of their groups, from the first opener to the last closer.
fn roots_region(groups: &[(i32, String)]) -> Option<std::ops::Range<usize>> {
    let depths = depths(groups.iter().cloned());
    let first = (0..groups.len()).find(|i| depths[*i] == 1 && groups[*i].0 == 302 && groups[*i].1.trim().ends_with('{'))?;
    let last = (first..groups.len()).rfind(|i| depths[*i] == 1 && groups[*i].0 == 303 && groups[*i].1.trim() == "}")?;
    Some(first..last + 1)
}

/// A MULTILEADER: its leader roots as the source had them (the model joins the roots of one entity, so
/// their number, the lines of each and the landings are the snapshot's), then the values of the geometry groups
/// of the content context by [`nested_restore`].
fn mleader_restore(generated: &[Tag], kept: &[RawTag], convert: impl Fn(&RawTag) -> Tag) -> Option<Vec<Tag>> {
    let range = subclass(generated, "AcDbMLeader", 0)?;
    let own = generated.get(range.clone())?;
    let kept_groups = kept.get(1..)?;
    let pairs = |t: &[Tag]| t.iter().map(|t| (t.code, t.str())).collect::<Vec<_>>();
    let kept_pairs: Vec<(i32, String)> = kept_groups.iter().map(|t| (t.code, t.value.clone())).collect();
    let (own_pairs, theirs) = (pairs(own), roots_region(&kept_pairs));
    let mut tags: Vec<Tag> = generated.to_vec();
    if let Some(theirs) = theirs {
        // Where the writer put its roots, else before the first `272` of the context.
        let at = match roots_region(&own_pairs) {
            Some(mine) => mine,
            None => {
                let depth = depths(own_pairs.iter().cloned());
                let i = (0..own_pairs.len()).find(|i| depth[*i] == 1 && own_pairs[*i].0 == 272)?;
                i..i
            }
        };
        let roots: Vec<Tag> = kept_groups.get(theirs)?.iter().map(&convert).collect();
        tags.splice(range.start + at.start..range.start + at.end, roots);
    }
    nested_restore(&tags, kept, "AcDbMLeader", mleader_geometry, convert)
}

fn nested_restore(
    generated: &[Tag],
    kept: &[RawTag],
    marker: &str,
    is_geometry: fn(usize, i32) -> bool,
    convert: impl Fn(&RawTag) -> Tag,
) -> Option<Vec<Tag>> {
    let range = subclass(generated, marker, 0)?;
    let own = generated.get(range.clone())?;
    let kept = kept.get(1..)?;
    let (own_depths, kept_depths) = (depths(own.iter().map(|t| (t.code, t.str()))), depths(kept.iter().map(|t| (t.code, t.value.clone()))));
    let pick = |codes: &mut dyn Iterator<Item = (usize, i32)>| {
        codes.enumerate().filter(|(_, (d, c))| is_geometry(*d, *c)).map(|(i, (d, c))| (i, d, c)).collect::<Vec<_>>()
    };
    let own_geometry = pick(&mut own_depths.iter().copied().zip(own.iter().map(|t| t.code)));
    let kept_geometry = pick(&mut kept_depths.iter().copied().zip(kept.iter().map(|t| t.code)));
    if own_geometry.len() != kept_geometry.len() || own_geometry.iter().zip(&kept_geometry).any(|(a, b)| (a.1, a.2) != (b.1, b.2)) {
        return None;
    }
    let mut out: Vec<Tag> = generated.to_vec();
    for (a, b) in own_geometry.iter().zip(&kept_geometry) {
        let value = kept.get(b.0)?;
        *out.get_mut(range.start + a.0)? = convert(value);
    }
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
    let records: Vec<_> = records.map(|(kind, tags)| (kind, tags, handle_of(tags))).collect();
    let mut seen: HashMap<Handle, usize> = HashMap::new();
    for h in records.iter().filter_map(|r| r.2) {
        *seen.entry(h).or_default() += 1;
    }
    let keys = keys(d, |h| seen.get(&h) == Some(&1));
    let mut snapshots: HashMap<Handle, Arc<GeometrySnapshot>> = HashMap::new();
    for (i, (kind, tags, h)) in records.iter().enumerate() {
        let Some(h) = h else { continue };
        if let Some(model) = keys.get(h)
            && let Some(groups) = capture(kind, tags)
        {
            let mut parts = vec![GeometryRecord { kind: kind.to_string(), groups }];
            // A heavy 2D polyline replays its VERTEX records too (the model has them converted).
            if model.starts_with("HPOLY") {
                let children = records.iter().skip(i + 1).take_while(|(k, ..)| *k == "VERTEX");
                let children: Option<Vec<_>> =
                    children.map(|(k, t, _)| Some(GeometryRecord { kind: k.to_string(), groups: capture(k, t)? })).collect();
                let Some(children) = children else { continue };
                parts.extend(children);
            }
            snapshots.insert(*h, Arc::new(GeometrySnapshot { records: parts, model: model.clone() }));
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

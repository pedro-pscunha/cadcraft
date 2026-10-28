use super::ctx::OsnapRef;
use super::{AssocSnap, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, LwPolyline};
use cadcraft_dxf::{Kind, Tag, kind_of};

/// Whether a DIMASSOC point reference to `k` names a point of it without naming a segment:
/// true for objects made of one curve. Polylines are referenced by segment ([`polyline_vertex_ref`]);
/// 3D polylines and multilines can't be yet, and references to them are left out, as AutoCAD
/// otherwise resolved them to the first segment and changed the dimension's value (#1049).
fn whole_object_ref(k: &EntityKind) -> bool {
    !matches!(k, EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) | EntityKind::MLine(_))
}

/// A polyline vertex as AutoCAD references it (#1049): an end of one of its two segments,
/// which are numbered from 1 (segment k runs from vertex k−1 to vertex k; a closed polyline's
/// closing segment is the last). Returns (osnap, parameter, segment): the end of the incoming
/// segment (osnap 1, parameter 1.0), else the start of the outgoing one (osnap 13, 0.0). Only
/// straight segments are used, whose parameters are 0 and 1; a vertex between two arcs gets no
/// reference.
pub(crate) fn polyline_vertex_ref(p: &LwPolyline, index: usize) -> Option<(i64, f64, i64)> {
    let n = p.vertices.len();
    if index >= n || n < 2 {
        return None;
    }
    let segments = if p.closed { n } else { n - 1 };
    // Segment k (1-based) starts at vertex k−1; its bulge is that vertex's.
    let straight = |k: usize| k >= 1 && k <= segments && p.vertices.get(k - 1).is_some_and(|v| v.bulge == 0.0);
    let incoming = if index == 0 { if p.closed { Some(n) } else { None } } else { Some(index) };
    let outgoing = (index < segments).then_some(index + 1);
    // Vertex 0 as the start of segment 1, the others as the end of the segment before, as
    // AutoCAD writes them when it can.
    let preferred = if index == 0 {
        [outgoing.map(|k| (13, 0.0, k)), incoming.map(|k| (1, 1.0, k))]
    } else {
        [incoming.map(|k| (1, 1.0, k)), outgoing.map(|k| (13, 0.0, k))]
    };
    preferred.into_iter().flatten().find(|(_, _, k)| straight(*k)).map(|(o, t, k)| (o, t, i64::try_from(k).unwrap_or(0)))
}

/// Standard DIMASSOC point references of an associative dimension: the extension-line
/// origins of linear/aligned dimensions, both ends of both lines of an angular dimension, the
/// feature point of ordinate dimensions and the circle of a radial/diametric dimension, linked
/// to objects that exist ([`whole_object_ref`]). CADCraft's exact links travel in xdata as well.
pub(super) fn std_assoc_refs(d: &Drawing, dm: &Dimension) -> (i64, Vec<OsnapRef>) {
    if matches!(dm.kind, DimKind::Radius | DimKind::Diameter) {
        return radial_ref(d, &dm.assoc);
    }
    let slots: &[&str] = match dm.kind {
        DimKind::Linear { .. } | DimKind::Aligned => &["p13", "p14"],
        DimKind::Angular => &["p13", "p14", "p15", "p16"],
        DimKind::Angular3P => &["p13", "p14", "p15"],
        // Saved as an aligned dimension (`dim_xdata` marks it): its extension origins are the arc ends.
        DimKind::ArcLength => &["p13", "p14"],
        DimKind::Ordinate { .. } => &["p13"],
        DimKind::Radius | DimKind::Diameter => &[],
    };
    let mut flags = 0;
    let mut refs = Vec::new();
    for (slot, name) in slots.iter().enumerate() {
        let Some(a) = dm.assoc.iter().find(|a| a.point == *name) else { continue };
        let Some(ent) = d.entity(a.handle) else { continue };
        let point = match *name {
            "p13" => dm.p13,
            "p14" => dm.p14,
            "p15" => dm.p15,
            "p16" => dm.p16,
            _ => continue,
        };
        if let EntityKind::LwPolyline(p) = &ent.kind {
            let index = match &a.snap {
                AssocSnap::Vertex { index } => Some(*index),
                AssocSnap::Start => Some(0),
                AssocSnap::End => p.vertices.len().checked_sub(1),
                _ => None,
            };
            let Some((osnap, param, segment)) = index.and_then(|i| polyline_vertex_ref(p, i)) else { continue };
            flags |= 1 << slot;
            refs.push(OsnapRef { osnap, main: a.handle, segment, param, point, other: None });
            continue;
        }
        if !whole_object_ref(&ent.kind) {
            continue;
        }
        let (osnap, param, other) = match &a.snap {
            // AutoCAD marks a curve's start with osnap 13 and its end with osnap 1 (group 72).
            AssocSnap::Start => (if matches!(ent.kind, EntityKind::Point(_)) { 1 } else { 13 }, 0.0, None),
            AssocSnap::End => (1, 1.0, None),
            AssocSnap::Vertex { .. } => (1, 0.0, None),
            AssocSnap::Mid => (2, 0.0, None),
            AssocSnap::Center => (if matches!(ent.kind, EntityKind::Point(_)) { 4 } else { 3 }, 0.0, None),
            AssocSnap::OnCircle { angle } => (10, *angle, None),
            AssocSnap::Intersection { other } => {
                if !d.entity(*other).is_some_and(|o| whole_object_ref(&o.kind)) {
                    continue;
                }
                (6, 0.0, Some(*other))
            }
        };
        flags |= 1 << slot;
        refs.push(OsnapRef { osnap, main: a.handle, segment: 0, param, point, other });
    }
    (flags, refs)
}

/// The complete DIMASSOC record after group 0, including its owner/reactor header. Reading
/// compares against this same grammar so neither header nor payload semantics can diverge.
pub(super) fn dimassoc_tags(handle: &str, dictionary: &str, dimension: Handle, flags: i64, refs: &[OsnapRef]) -> Vec<Tag> {
    let mut tags = vec![
        Tag::s(5, handle),
        Tag::s(102, "{ACAD_REACTORS"),
        Tag::s(330, dictionary),
        Tag::s(102, "}"),
        Tag::s(330, dictionary),
        Tag::s(100, "AcDbDimAssoc"),
        Tag::s(330, dimension.hex()),
        Tag::i(90, flags),
        Tag::i(70, 0),
        Tag::i(71, 0),
    ];
    for r in refs {
        tags.extend([
            Tag::s(1, "AcDbOsnapPointRef"),
            Tag::i(72, r.osnap),
            Tag::s(331, r.main.hex()),
            Tag::i(73, if r.segment > 0 { 2 } else { 0 }),
            Tag::i(91, r.segment),
            Tag::s(301, ""),
            Tag::f(40, r.param),
            Tag::f(10, r.point.x),
            Tag::f(20, r.point.y),
            Tag::f(30, r.point.z),
        ]);
        if let Some(other) = r.other {
            tags.extend([Tag::s(332, other.hex()), Tag::i(74, 0), Tag::i(92, 0), Tag::s(302, "")]);
        }
        tags.push(Tag::i(75, 0));
    }
    tags
}

fn same_tag(a: &Tag, b: &Tag) -> bool {
    if a.code != b.code {
        return false;
    }
    let code = a.code;
    let kind = kind_of(code);
    let (a, b) = (a.str(), b.str());
    // Pointer ranges are strings in some DXF value APIs; compare their normalized handle values.
    if cadcraft_doc::foreign::is_pointer(code) {
        return Handle::parse_hex(&a).zip(Handle::parse_hex(&b)).is_some_and(|(a, b)| a == b);
    }
    if (310..=319).contains(&code) {
        let (a, b) = (a.trim(), b.trim());
        return a.len().is_multiple_of(2) && a.bytes().all(|c| c.is_ascii_hexdigit()) && a.eq_ignore_ascii_case(b);
    }
    match kind {
        Kind::Real => a.trim().parse::<f64>().ok().zip(b.trim().parse::<f64>().ok()).is_some_and(|(a, b)| a.is_finite() && a == b),
        Kind::Int16 | Kind::Int32 | Kind::Int64 | Kind::Bool => {
            a.trim().parse::<i64>().ok().zip(b.trim().parse::<i64>().ok()).is_some_and(|(a, b)| a == b)
        }
        Kind::Hex => Handle::parse_hex(&a).zip(Handle::parse_hex(&b)).is_some_and(|(a, b)| a == b),
        Kind::Str => a == b,
    }
}

/// Whether the complete DIMASSOC can be written unchanged from the model. This deliberately
/// keeps unfamiliar or omitted groups raw rather than guessing equivalent defaults. Block
/// dimensions are not in `Drawing::entity` and are not planned by the association writer.
pub(crate) fn dimassoc_reproducible(d: &Drawing, dimension: Handle, source: &[Tag]) -> bool {
    let Some(EntityKind::Dimension(dm)) = d.entity(dimension).map(|e| &e.kind) else { return false };
    let (flags, refs) = std_assoc_refs(d, dm);
    let Some((_, referenced)) = cadcraft_doc::foreign::dimassoc_reference(source.iter().map(|t| (t.code, t.str()))) else { return false };
    if referenced != dimension {
        return false;
    }
    let Some(dictionary) = cadcraft_doc::foreign::object_owner(source.iter().map(|t| (t.code, t.str()))) else { return false };
    let Some(handle) = source.iter().find(|t| t.code == 5).map(Tag::str) else { return false };
    let expected = dimassoc_tags(&handle, &dictionary.hex(), dimension, flags, &refs);
    !refs.is_empty() && source.len() == expected.len() && source.iter().zip(&expected).all(|(a, b)| same_tag(a, b))
}

/// The single standard reference of a radial or diametric dimension: the circle or arc it
/// measures, by its centre, with associativity flag 2. AutoCAD writes radial and diametric
/// dimensions that way: the one reference names the geometry, whose radius or diameter the
/// dimension then follows. Returns no flags and no references when the association does not name
/// a circle or arc, so such a dimension is saved unassociated rather than pointing at nothing.
fn radial_ref(d: &Drawing, assoc: &[DimAssoc]) -> (i64, Vec<OsnapRef>) {
    for a in assoc {
        let Some(ent) = d.entity(a.handle) else { continue };
        let centre = match &ent.kind {
            EntityKind::Circle(c) => c.center.xy(),
            EntityKind::Arc(arc) => arc.center.xy(),
            _ => continue,
        };
        if !centre.is_finite() {
            continue;
        }
        return (2, vec![OsnapRef { osnap: 3, main: a.handle, segment: 0, param: 0.0, point: centre.to3(0.0), other: None }]);
    }
    (0, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_comparison_uses_handle_values_not_spelling() {
        for code in [330, 331, 332, 1005] {
            assert!(same_tag(&Tag::s(code, "00002a"), &Tag::s(code, "2A")), "pointer group {code} was compared as text");
        }
    }

    #[test]
    fn binary_chunk_comparison_uses_case_insensitive_hex() {
        assert!(
            same_tag(&Tag::s(310, "ab00CF1234567890aa55"), &Tag::s(310, "AB00cf1234567890AA55")),
            "binary chunks must compare as case-insensitive hex"
        );
    }
}

//! The source geometry of an entity, kept so that a save with no geometry edit writes it back
//! exactly (entity planes, Z values, angles and scale signs that the model flattens).
//!
//! The snapshot is part of the entity ([`Common::geometry`], [`Attrib::geometry`]), so a copy of
//! the entity carries it, within a drawing and to another one, and undo restores it with the
//! entity. A [`GeometrySnapshot`] holds the geometry groups of the entity's source record and a key of
//! the model geometry the reader made from them. The writer replays the groups only while the
//! entity's current [`geometry_key`] is the same string; a property edit leaves the key as it was,
//! a geometry edit changes it, and the entity is written from the model as before.
//!
//! The key spells every geometry field with `{:?}`, which prints the shortest text that parses
//! back to the same `f64`, so two keys are equal exactly when every number has the same bits.

use std::fmt::Write;

use super::{Attrib, Common, EntityKind, RawTag, Text};

/// The geometry groups of one source record (see [`GeometrySnapshot::records`]).
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryRecord {
    /// The DXF type of the source record.
    pub kind: String,
    /// Its geometry groups in file order, each subclass after a `100` marker group.
    pub groups: Vec<RawTag>,
}

/// An entity's source geometry and the model geometry it was read as.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometrySnapshot {
    pub records: Vec<GeometryRecord>,
    /// The entity's [`geometry_key`] when it was read.
    pub model: String,
}

fn text_geometry(s: &mut String, t: &Text) {
    let _ = write!(s, "{:?}", (t.insert, t.align_pt, t.height, t.rotation, t.width_factor, t.oblique, t.halign, t.valign));
}

/// The key of the geometry of a top-level entity, or `None` for a kind that keeps no snapshot.
/// It covers the entity's plane (`Common::extrusion`, `Common::thickness`) and the model fields
/// the source geometry groups are replayed for, and nothing else: not the layer, colour, text
/// content, style names or attribute values.
pub fn geometry_key(kind: &EntityKind, common: &Common) -> Option<String> {
    let mut s = String::new();
    let plane = (common.extrusion, common.thickness);
    match kind {
        EntityKind::Line(l) => {
            let _ = write!(s, "LINE{:?}{plane:?}", (l.a, l.b));
        }
        EntityKind::Point(p) => {
            let _ = write!(s, "POINT{:?}{plane:?}", (p.p, p.angle));
        }
        EntityKind::Circle(c) => {
            let _ = write!(s, "CIRCLE{:?}{plane:?}", (c.center, c.radius));
        }
        EntityKind::Arc(a) => {
            let _ = write!(s, "ARC{:?}{plane:?}", (a.center, a.radius, a.start, a.end));
        }
        EntityKind::Solid(c) | EntityKind::Trace(c) => {
            let _ = write!(s, "SOLID{:?}{plane:?}", c.corners);
        }
        EntityKind::Text(t) => {
            s.push_str("TEXT");
            text_geometry(&mut s, t);
            let _ = write!(s, "{plane:?}");
        }
        EntityKind::AttDef(a) => {
            s.push_str("ATTDEF");
            text_geometry(&mut s, &a.text);
            let _ = write!(s, "{plane:?}");
        }
        // A multi-column MTEXT keeps no snapshot.
        EntityKind::MText(t) if t.columns.is_none() => {
            let _ = write!(s, "MTEXT{:?}{plane:?}", (t.insert, t.height, t.width, t.attach, t.rotation, t.line_spacing, t.line_spacing_exact));
        }
        EntityKind::Insert(i) => {
            let _ = write!(s, "INSERT{:?}{plane:?}", (i.insert, i.scale, i.rotation, i.cols, i.rows, i.col_spacing, i.row_spacing));
        }
        EntityKind::Spline(c) => {
            let _ = write!(s, "SPLINE{:?}{plane:?}", (c.degree, &c.knots, &c.control, &c.weights, &c.fit, c.closed, &c.fit_opts));
        }
        EntityKind::Leader(l) => {
            let _ = write!(s, "LEADER{:?}{plane:?}", l.vertices);
        }
        EntityKind::Hatch(h) => {
            // The boundary objects (`sources`) are references, not geometry.
            let loops: Vec<_> = h.loops.iter().map(|l| (&l.vertices, l.outer)).collect();
            let _ = write!(s, "HATCH{:?}{plane:?}", (&h.pattern, h.solid, loops, h.scale, h.angle, h.style, h.elevation, h.origin, &h.pattern_lines));
        }
        _ => return None,
    }
    Some(s)
}

/// The key of the geometry of an attribute of an INSERT, which has a plane of its own that the
/// model does not keep.
pub fn attrib_geometry_key(a: &Attrib) -> String {
    let mut s = String::from("ATTRIB");
    text_geometry(&mut s, &a.text);
    s
}

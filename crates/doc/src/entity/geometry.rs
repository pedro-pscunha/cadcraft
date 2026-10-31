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

use cadcraft_geom::Vec3;

use super::{Attrib, Common, Dimension, EntityKind, HAlign, RawTag, Text, VAlign};

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
    /// A justified text as it was read, to place its baseline start again after the text moved.
    pub text: Option<TextAsRead>,
    /// A dimension as it was read, to keep its text position after the dimension moved.
    pub dimension: Option<DimensionAsRead>,
}

/// The definition points of a dimension when it was read (the four that share one frame in a legacy and a canonical
/// dimension: the dimension line point and the first, second and arc or centre points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionAsRead {
    points: [Vec3; 4],
    /// The OCS insertion point (DXF 12) of this dimension's source graphics block.
    pub clone_point: Vec3,
}

impl DimensionAsRead {
    pub fn of(dm: &Dimension, clone_point: Vec3) -> DimensionAsRead {
        DimensionAsRead { points: [dm.defpt, dm.p13, dm.p14, dm.p15], clone_point }
    }

    /// Whether `dm` is the dimension as it was read, moved, turned or mirrored as a whole: every distance between its
    /// definition points is the one it had. Its text position then moved with it, and is still the file's.
    pub fn is_rigid_image(&self, dm: &Dimension) -> bool {
        let now = [dm.defpt, dm.p13, dm.p14, dm.p15];
        let gap = |p: &[Vec3; 4], i: usize, j: usize| (p[i] - p[j]).len();
        (0..4).all(|i| (i + 1..4).all(|j| (gap(&self.points, i, j) - gap(&now, i, j)).abs() <= 1e-9 * (1.0 + gap(&self.points, i, j))))
    }
}

/// A justified text's placement when it was read: the baseline start (group 10), the alignment point (group 11) and the
/// baseline angle, with the [`Text::layout_key`] they were measured for. While the text's layout is the same, a move or a
/// rotation keeps the baseline start where it was relative to the alignment point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextAsRead {
    pub insert: Vec3,
    pub align_pt: Vec3,
    pub rotation: f64,
    pub layout: u64,
}

impl TextAsRead {
    /// `t` as read, or `None` for text that is not justified (left on the baseline, aligned or fit text are placed by their
    /// start point) or has no alignment point.
    pub fn of(t: &Text) -> Option<TextAsRead> {
        if matches!((t.halign, t.valign), (HAlign::Left, VAlign::Baseline) | (HAlign::Aligned | HAlign::Fit, _)) {
            return None;
        }
        Some(TextAsRead { insert: t.insert, align_pt: t.align_pt?, rotation: t.rotation, layout: t.layout_key() })
    }

    /// The baseline start of `t` after a move or rotation since it was read: its alignment point plus the offset it had,
    /// turned by the rotation since. `None` when the text's layout changed (another value, size, style or justification).
    pub fn insert_after(&self, t: &Text) -> Option<Vec3> {
        let align = t.align_pt.filter(|_| t.layout_key() == self.layout)?;
        let offset = (self.insert.xy() - self.align_pt.xy()).rotate(t.rotation - self.rotation);
        Some(Vec3::new(align.x + offset.x, align.y + offset.y, align.z + self.insert.z - self.align_pt.z))
    }
}

/// The placement of a multiline attribute's embedded MTEXT: its place, size and attachment, its full direction and its
/// own plane.
fn embedded_geometry(s: &mut String, a: &Attrib) {
    if let Some(ml) = a.multiline.as_ref() {
        let m = &ml.mtext;
        let _ = write!(s, "{:?}", (m.insert, m.height, m.width, m.attach, m.rotation, m.line_spacing, m.line_spacing_exact, m.direction, ml.normal));
    }
}

/// The geometry fields of a text: its points and baseline angle, size, slant, alignment and reflection.
fn text_geometry(s: &mut String, t: &Text) {
    let _ = write!(s, "{:?}", (t.insert, t.align_pt, t.height, t.rotation, t.width_factor, t.oblique, t.halign, t.valign, t.backward, t.upside_down));
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
            embedded_geometry(&mut s, a);
            let _ = write!(s, "{plane:?}");
        }
        EntityKind::MText(t) => {
            let _ = write!(
                s,
                "MTEXT{:?}{plane:?}",
                (t.insert, t.height, t.width, t.attach, t.rotation, t.line_spacing, t.line_spacing_exact, &t.columns, t.direction)
            );
        }
        EntityKind::Insert(i) => {
            let _ = write!(s, "INSERT{:?}{plane:?}", (i.insert, i.scale, i.rotation, i.cols, i.rows, i.col_spacing, i.row_spacing));
        }
        EntityKind::LwPolyline(p) => {
            let vertices: Vec<_> = p.vertices.iter().map(|v| (v.p, v.bulge, v.start_width, v.end_width)).collect();
            let base = (vertices, p.closed, p.plinegen, p.const_width, p.elevation);
            if p.heavy {
                // A heavy polyline's VERTEX records follow its parent: the fit data and the vertices are its geometry.
                let fit = p.fit.as_ref().map(|f| {
                    let fitted: Vec<_> = f.vertices.iter().map(|v| (v.position, v.flags, v.start_width, v.end_width, v.bulge, v.tangent)).collect();
                    let display: Vec<_> = f.display_vertices.iter().map(|v| (v.p, v.bulge, v.start_width, v.end_width)).collect();
                    (
                        f.flags,
                        f.smooth_type,
                        f.m_count,
                        f.n_count,
                        f.m_density,
                        f.n_density,
                        f.start_width,
                        f.end_width,
                        f.elevation,
                        f.extrusion,
                        fitted,
                        display,
                    )
                });
                let _ = write!(s, "HPOLY{base:?}{fit:?}{plane:?}");
            } else {
                let _ = write!(s, "LWPOLY{base:?}{plane:?}");
            }
        }
        EntityKind::Ellipse(el) => {
            let _ = write!(s, "ELLIPSE{:?}{plane:?}", (el.center, el.major, el.ratio, el.start, el.end));
        }
        EntityKind::Polyline3d(p) => {
            let _ = write!(s, "POLY3D{:?}{plane:?}", p.closed);
        }
        EntityKind::PolylineMesh(m) => {
            let _ = write!(s, "MESH{:?}{plane:?}", (m.kind as u8, m.flags, m.m_count, m.n_count, m.m_density, m.n_density, m.smooth_type));
        }
        EntityKind::Dimension(dm) => {
            // Not the block, the text, the style, the measurement or the overrides, which are regenerated.
            let _ = write!(
                s,
                "DIMENSION{:?}{plane:?}",
                (dm.kind, dm.defpt, dm.text_mid, dm.p13, dm.p14, dm.p15, dm.p16, dm.text_rotation, dm.user_text_pos, dm.marks.oblique)
            );
        }
        EntityKind::MLeader(m) => {
            // Not the text, its style or the arrowhead: every root (number, landing, dogleg direction and length, and
            // its branches with their numbers and vertices), the context's frame and the content's placement.
            let text = m.text.as_ref().map(|t| (t.insert, t.height, t.width, t.attach, t.rotation, t.line_spacing, t.line_spacing_exact));
            let block = m.block.as_ref().map(|b| (b.insert, b.scale, b.rotation));
            let roots: Vec<_> = m
                .roots
                .iter()
                .map(|r| (r.id, r.landing, r.dogleg_dir, r.dogleg, r.branches.iter().map(|b| (b.id, &b.vertices)).collect::<Vec<_>>()))
                .collect();
            let _ = write!(s, "MLEADER{:?}{plane:?}", (roots, &m.context, m.spline, text, block));
            // A canonical text also holds its full direction, and the arrowhead size is part of what an edit scales.
            if m.context.geometry_revision == crate::GeometryRevision::CanonicalV2 {
                let _ = write!(s, "{:?}", (m.text.as_ref().and_then(|t| t.direction), m.arrow_size));
            }
        }
        EntityKind::Table(t) => {
            // The layout geometry: direction, sizes, the merged spans and the cells' own text heights; not the cell text.
            let cells: Vec<Vec<_>> = t.cells.iter().map(|row| row.iter().map(|c| (c.merged, c.text_height)).collect()).collect();
            let _ = write!(s, "TABLE{:?}{plane:?}", (t.insert, t.dir, &t.row_heights, &t.col_widths, cells));
        }
        EntityKind::Tolerance(t) => {
            let _ = write!(s, "TOLERANCE{:?}{plane:?}", (t.insert, t.dir));
        }
        EntityKind::MLine(m) => {
            // The vertices with their cuts and canonical frames (direction, miter, element parameters), the offset
            // scale, the justification, the closure and the cap switches; not the style's elements and caps.
            let vertices: Vec<_> = m.vertices.iter().map(|v| (v.p, &v.cuts, &v.frame)).collect();
            let _ = write!(s, "MLINE{:?}{plane:?}", (vertices, m.scale, m.justification.to_dxf(), m.closed, m.no_start_caps, m.no_end_caps));
        }
        EntityKind::Helix(h) => {
            let _ = write!(s, "HELIX{h:?}{plane:?}");
        }
        EntityKind::Wipeout(w) => {
            // The legacy outline, or the canonical image frame: origin, U and V, size, clip type and clip vertices.
            let _ = write!(s, "WIPEOUT{:?}{plane:?}", (&w.boundary, &w.frame));
        }
        EntityKind::Spline(c) => {
            let _ = write!(s, "SPLINE{:?}{plane:?}", (c.degree, &c.knots, &c.control, &c.weights, &c.fit, c.closed, &c.fit_opts, &c.space));
        }
        EntityKind::Leader(l) => {
            let _ = write!(s, "LEADER{:?}{plane:?}", (&l.vertices, &l.frame));
        }
        EntityKind::Hatch(h) => {
            // The boundary objects (`sources`) are references, not geometry.
            // A canonical loop also has its record flags and exact edges.
            let loops: Vec<_> = h.loops.iter().map(|l| (&l.vertices, l.outer, l.flags, &l.edges)).collect();
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
    // Its own plane: not its block reference's.
    let _ = write!(s, "{:?}", (a.plane.normal, a.plane.thickness));
    embedded_geometry(&mut s, a);
    s
}

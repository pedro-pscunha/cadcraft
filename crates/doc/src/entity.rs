//! Entities: the graphical objects of a drawing.

use cadcraft_color::Color;
use cadcraft_geom::{
    Arc as GArc, Bounds2, Circle as GCircle, Ellipse as GEllipse, Line as GLine, Mat3, Mat4, PolyVertex, Polyline, Segment, Spline as GSpline, Vec2,
    Vec3,
};
use serde::{Deserialize, Serialize};

mod mline;
pub use mline::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "legacy_recovery::SerializedEntity")]
pub struct Entity {
    #[serde(default, rename = "geometryRevision")]
    pub geometry_revision: GeometryRevision,
    pub handle: Handle,
    #[serde(default)]
    pub common: Common,
    pub kind: EntityKind,
}

impl Entity {
    pub fn new(handle: Handle, kind: EntityKind) -> Self {
        Entity { geometry_revision: Default::default(), handle, common: Common::default(), kind }
    }
}
fn one() -> f64 {
    1.0
}
fn standard() -> String {
    "Standard".into()
}

mod helix;
pub use helix::*;
fn yes() -> bool {
    true
}
mod region;
pub use region::*;
mod revision;
pub use revision::GeometryRevision;

mod affine_canonical;
mod block;
mod common;
mod curves;
mod curves_canonical;
mod dim;
mod dimension_canonical;
mod edit;
mod fill;
mod fill_canonical;
mod geometry;
mod helix_canonical;
mod insert_canonical;
mod layout;
mod layout_canonical;
mod leader_canonical;
mod legacy_recovery;
mod mesh;
mod mleader;
mod mleader_canonical;
mod mline_canonical;
mod spline_canonical;
mod text;
mod text_canonical;
mod world;
mod world_families;
pub use legacy_recovery::{Promotion, promote_legacy_spline_leader};
pub use spline_canonical::spline_transformed_canonical;
pub use world::{WorldFill, WorldFillKind, WorldGeometry, WorldGradient, WorldLayout, WorldPattern, WorldPrimitive, WorldText, WorldTextContent};
pub use world_families::{
    DimensionLayout, DimensionLayoutText, LeaderArrowLayout, MLeaderWorld, TableLayout, TableLayoutText, arc_primitives, attrib_primitives,
    circle_primitives, dimension_primitives, hatch_primitives, helix_primitives, leader_primitives, lwpolyline_primitives, mleader_primitives,
    mline_primitives, mtext_primitives, solid_primitives, spline_primitives, table_primitives, text_primitives, tolerance_primitives,
    wipeout_primitives,
};

pub use block::{Attrib, AttribModes, AttribPlane, AttribProps, Insert, MultilineAttrib, mtext_literal};
pub use common::{Common, Handle, Lineweight, RawTag, Transparency};
pub use curves::{
    Arc, Circle, Ellipse, Face3d, Line, LwPolyline, Point, Polyline3d, Polyline3dFit, PolylineFit, PolylineFitVertex, PolylineRecord,
    PolylineRecords, RayLine, Solid,
};
pub use dim::{AssocSnap, DimAssoc, DimBreak, DimKind, DimMarks, Dimension, Inspection, JoggedRadius, Leader, LeaderFrame};
pub use fill::{Gradient, GradientSource, Hatch, HatchEdge, HatchLoop, HatchSpline, Wipeout, WipeoutDisplay, WipeoutFrame};
pub use geometry::{DimensionAsRead, GeometryRecord, GeometrySnapshot, TextAsRead, attrib_geometry_key, geometry_key};
pub use layout::{Image, ImageDefinition, Table, TableCell, TableCellBlock, TableOverrides, Viewport, ViewportStatus};
pub use mesh::{MeshKind, MeshVertex, PolylineMesh};
pub use mleader::{
    MLeader, MLeaderBranch, MLeaderCodes, MLeaderContext, MLeaderContextGroups, MLeaderEntityGroups, MLeaderGroups, MLeaderRoot, MLeaderRootGroups,
};
pub use mleader_canonical::MLeaderParts;
pub use text::{ColumnExtents, HAlign, MText, MTextColumnType, MTextColumns, Text, VAlign};
pub use text_canonical::{Similarity, TextFrame};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unknown {
    pub dxf_type: String,
    pub tags: Vec<RawTag>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EntityKind {
    Line(Line),
    Point(Point),
    Circle(Circle),
    Arc(Arc),
    Ellipse(Ellipse),
    LwPolyline(LwPolyline),
    Polyline3d(Polyline3d),
    PolylineMesh(PolylineMesh),
    Spline(GSpline),
    Helix(Helix),
    Ray(RayLine),
    XLine(RayLine),
    Text(Text),
    MText(MText),
    AttDef(Attrib),
    Insert(Insert),
    Dimension(Dimension),
    MLine(MLine),
    Leader(Leader),
    MLeader(MLeader),
    Hatch(Hatch),
    Solid(Solid),
    Trace(Solid),
    Face3d(Face3d),
    Region(Region),
    Viewport(Viewport),
    Image(Image),
    Wipeout(Wipeout),
    Table(Table),
    Tolerance(crate::Tolerance),
    Unknown(Unknown),
}

/// A primitive the entity decomposes into, for snapping, selection, trimming and rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Seg(Segment),
    Circle(GCircle),
    Ellipse(GEllipse),
    Spline(GSpline),
    Point(Vec2),
    /// An infinite line (`ray == false`) or a ray from `base` along `dir`.
    Infinite {
        base: Vec2,
        dir: Vec2,
        ray: bool,
    },
    /// A filled polygon (solids, wide polylines, hatch fill) — the outline is also selectable.
    Fill(Vec<Vec2>),
}

impl EntityKind {
    /// AutoCAD's object type name as shown in the Properties palette.
    pub fn type_name(&self) -> &'static str {
        match self {
            EntityKind::Line(_) => "Line",
            EntityKind::Point(_) => "Point",
            EntityKind::Circle(_) => "Circle",
            EntityKind::Arc(_) => "Arc",
            EntityKind::Ellipse(_) => "Ellipse",
            EntityKind::LwPolyline(_) => "Polyline",
            EntityKind::Polyline3d(_) => "3D Polyline",
            EntityKind::PolylineMesh(mesh) => match mesh.kind {
                MeshKind::Polyface => "Polyface Mesh",
                MeshKind::Polygon => "Polygon Mesh",
            },
            EntityKind::Spline(_) => "Spline",
            EntityKind::Helix(_) => "Helix",
            EntityKind::Ray(_) => "Ray",
            EntityKind::XLine(_) => "Xline",
            EntityKind::Text(_) => "Text",
            EntityKind::MText(_) => "MText",
            EntityKind::AttDef(_) => "Attribute Definition",
            EntityKind::Insert(_) => "Block Reference",
            EntityKind::Dimension(_) => "Dimension",
            EntityKind::MLine(_) => "MLine",
            EntityKind::Leader(_) => "Leader",
            EntityKind::MLeader(_) => "Multileader",
            EntityKind::Hatch(_) => "Hatch",
            EntityKind::Solid(_) => "2D Solid",
            EntityKind::Trace(_) => "Trace",
            EntityKind::Face3d(_) => "3D Face",
            EntityKind::Region(_) => "Region",
            EntityKind::Viewport(_) => "Viewport",
            EntityKind::Image(_) => "Raster Image",
            EntityKind::Wipeout(_) => "Wipeout",
            EntityKind::Table(_) => "Table",
            EntityKind::Tolerance(_) => "Tolerance",
            EntityKind::Unknown(_) => "Proxy",
        }
    }
    /// DXF entity type name.
    pub fn dxf_name(&self) -> &str {
        match self {
            EntityKind::Line(_) => "LINE",
            EntityKind::Point(_) => "POINT",
            EntityKind::Circle(_) => "CIRCLE",
            EntityKind::Arc(_) => "ARC",
            EntityKind::Ellipse(_) => "ELLIPSE",
            EntityKind::LwPolyline(_) => "LWPOLYLINE",
            EntityKind::Polyline3d(_) | EntityKind::PolylineMesh(_) => "POLYLINE",
            EntityKind::Spline(_) => "SPLINE",
            EntityKind::Helix(_) => "HELIX",
            EntityKind::Ray(_) => "RAY",
            EntityKind::XLine(_) => "XLINE",
            EntityKind::Text(_) => "TEXT",
            EntityKind::MText(_) => "MTEXT",
            EntityKind::AttDef(_) => "ATTDEF",
            EntityKind::Insert(_) => "INSERT",
            EntityKind::Dimension(_) => "DIMENSION",
            EntityKind::MLine(_) => "MLINE",
            EntityKind::Leader(_) => "LEADER",
            EntityKind::MLeader(_) => "MULTILEADER",
            EntityKind::Hatch(_) => "HATCH",
            EntityKind::Solid(_) => "SOLID",
            EntityKind::Trace(_) => "TRACE",
            EntityKind::Face3d(_) => "3DFACE",
            EntityKind::Region(_) => "REGION",
            EntityKind::Viewport(_) => "VIEWPORT",
            EntityKind::Image(_) => "IMAGE",
            EntityKind::Wipeout(_) => "WIPEOUT",
            EntityKind::Table(_) => "ACAD_TABLE",
            EntityKind::Tolerance(_) => "TOLERANCE",
            EntityKind::Unknown(u) => u.dxf_type.as_str(),
        }
    }

    /// Geometric primitives in WCS (2D projection). Text/blocks/dimensions return their
    /// insertion points only; their full geometry comes from the renderer's expansion.
    pub fn prims(&self) -> Vec<Prim> {
        match self {
            EntityKind::Line(l) => vec![Prim::Seg(Segment::Line(GLine::new(l.a.xy(), l.b.xy())))],
            EntityKind::Point(p) => vec![Prim::Point(p.p.xy())],
            EntityKind::Circle(c) => vec![Prim::Circle(GCircle::new(c.center.xy(), c.radius))],
            EntityKind::Arc(a) => vec![Prim::Seg(Segment::Arc { arc: GArc::new(a.center.xy(), a.radius, a.start, a.end), ccw: true })],
            EntityKind::Ellipse(e) => {
                vec![Prim::Ellipse(e.projected())]
            }
            EntityKind::LwPolyline(p) => {
                let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
                pl.segments().into_iter().map(Prim::Seg).collect()
            }
            EntityKind::Polyline3d(p) => {
                let mut v: Vec<Prim> =
                    p.points.windows(2).filter_map(|w| Some(Prim::Seg(Segment::Line(GLine::new(w.first()?.xy(), w.get(1)?.xy()))))).collect();
                if p.closed
                    && let (Some(a), Some(b)) = (p.points.last(), p.points.first())
                {
                    v.push(Prim::Seg(Segment::Line(GLine::new(a.xy(), b.xy()))));
                }
                v
            }
            EntityKind::PolylineMesh(mesh) => mesh.faces().into_iter().flat_map(|face| EntityKind::Face3d(face).prims()).collect(),
            EntityKind::Spline(s) => vec![Prim::Spline(s.clone())],
            EntityKind::Helix(h) => vec![Prim::Spline(h.spline())],
            EntityKind::Ray(r) => vec![Prim::Infinite { base: r.base.xy(), dir: r.dir.xy().normalized(), ray: true }],
            EntityKind::XLine(r) => vec![Prim::Infinite { base: r.base.xy(), dir: r.dir.xy().normalized(), ray: false }],
            EntityKind::Solid(s) | EntityKind::Trace(s) => {
                let c = &s.corners;
                vec![Prim::Fill(vec![c[0].xy(), c[1].xy(), c[3].xy(), c[2].xy()])]
            }
            EntityKind::Face3d(f) => {
                let c = &f.corners;
                let pts = [c[0].xy(), c[1].xy(), c[2].xy(), c[3].xy(), c[0].xy()];
                pts.windows(2).filter_map(|w| Some(Prim::Seg(Segment::Line(GLine::new(*w.first()?, *w.get(1)?))))).collect()
            }
            EntityKind::Region(r) => r.segments().into_iter().map(Prim::Seg).collect(),
            EntityKind::Hatch(h) => {
                h.loops.iter().flat_map(|l| Polyline { vertices: l.vertices.clone(), closed: true }.segments()).map(Prim::Seg).collect()
            }
            EntityKind::Wipeout(w) => {
                let mut pts = w.boundary.clone();
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                pts.windows(2).filter_map(|w| Some(Prim::Seg(Segment::Line(GLine::new(*w.first()?, *w.get(1)?))))).collect()
            }
            EntityKind::Viewport(v) => {
                let c = v.center.xy();
                let h = Vec2::new(v.width / 2.0, v.height / 2.0);
                let b = Bounds2::new(c - h, c + h).corners();
                (0..4).map(|i| Prim::Seg(Segment::Line(GLine::new(b[i], b[(i + 1) % 4])))).collect()
            }
            EntityKind::Leader(l) => {
                l.vertices.windows(2).filter_map(|w| Some(Prim::Seg(Segment::Line(GLine::new(w.first()?.xy(), w.get(1)?.xy()))))).collect()
            }
            EntityKind::Text(t) => vec![Prim::Point(t.insert.xy())],
            EntityKind::MText(t) => vec![Prim::Point(t.insert.xy())],
            EntityKind::AttDef(a) => vec![Prim::Point(a.text.insert.xy())],
            EntityKind::Insert(i) => vec![Prim::Point(i.insert.xy())],
            EntityKind::Dimension(d) => vec![Prim::Point(d.defpt.xy())],
            EntityKind::MLine(m) => m.segments().into_iter().map(Prim::Seg).collect(),
            EntityKind::MLeader(m) => m
                .lines()
                .flat_map(|(_, b)| b.vertices.windows(2).filter_map(|w| Some(Prim::Seg(Segment::Line(GLine::new(w.first()?.xy(), w.get(1)?.xy()))))))
                .chain(m.block.iter().map(|b| Prim::Point(b.insert.xy())))
                .collect(),
            EntityKind::Image(i) => vec![Prim::Point(i.insert.xy())],
            EntityKind::Table(t) => vec![Prim::Point(t.insert.xy())],
            EntityKind::Tolerance(t) => vec![Prim::Point(t.insert.xy())],
            EntityKind::Unknown(_) => Vec::new(),
        }
    }

    /// Apply a 2D affine transform (move/rotate/uniform scale/mirror). Non-uniform scale turns
    /// circles into ellipses only where the kind supports it; otherwise radii use the mean scale.
    fn transform_legacy(&mut self, m: &Mat3) -> crate::Result<()> {
        let t3 = |p: &mut Vec3| {
            let q = m.apply(p.xy());
            p.x = q.x;
            p.y = q.y;
        };
        let s = m.scale_factor();
        let rot = m.rotation();
        let mirror = m.is_mirroring();
        match self {
            EntityKind::Line(l) => {
                t3(&mut l.a);
                t3(&mut l.b);
            }
            EntityKind::Point(p) => t3(&mut p.p),
            EntityKind::Circle(c) => {
                t3(&mut c.center);
                c.radius *= s;
            }
            EntityKind::Arc(a) => {
                let sp = m.apply(Vec2::polar(a.center.xy(), a.radius, a.start));
                let ep = m.apply(Vec2::polar(a.center.xy(), a.radius, a.end));
                t3(&mut a.center);
                a.radius *= s;
                let c = a.center.xy();
                if mirror {
                    a.start = c.angle_to(ep);
                    a.end = c.angle_to(sp);
                } else {
                    a.start = c.angle_to(sp);
                    a.end = c.angle_to(ep);
                }
            }
            EntityKind::Ellipse(e) => {
                if *m == Mat3::IDENTITY {
                    return Ok(());
                }
                if m.a == 1.0 && m.b == 0.0 && m.c == 0.0 && m.d == 1.0 {
                    t3(&mut e.center);
                    return Ok(());
                }
                if !e.flatten() {
                    return Err(crate::DocError::Invalid("An edge-on or invalid ellipse cannot use the legacy 2D edit.".into()));
                }
                let sp = m.apply(e.center.xy() + e.major.xy() * e.start.cos() + e.major.xy().perp() * (e.ratio * e.start.sin()));
                let ep = m.apply(e.center.xy() + e.major.xy() * e.end.cos() + e.major.xy().perp() * (e.ratio * e.end.sin()));
                t3(&mut e.center);
                let mj = m.apply_vec(e.major.xy());
                e.major = mj.to3(0.0);
                let full = (cadcraft_geom::ccw_sweep(e.start, e.end) - cadcraft_geom::TAU).abs() < 1e-9;
                if !full {
                    let ge = GEllipse { center: e.center.xy(), major: mj, ratio: e.ratio, start: 0.0, end: cadcraft_geom::TAU };
                    let (ps, pe) = (ge.param_of(sp), ge.param_of(ep));
                    if mirror {
                        e.start = pe;
                        e.end = ps;
                    } else {
                        e.start = ps;
                        e.end = pe;
                    }
                }
            }
            EntityKind::LwPolyline(p) => p.transform(m),
            EntityKind::Polyline3d(p) => p.map_points(t3),
            EntityKind::PolylineMesh(mesh) => mesh.transform(m),
            EntityKind::Spline(sp) => sp.transform(m),
            EntityKind::Helix(h) => {
                let sp = m.apply(h.start_point().xy());
                t3(&mut h.center);
                h.base_radius *= s;
                h.top_radius *= s;
                h.height *= s;
                h.start_angle = h.center.xy().angle_to(sp);
                // Mirrored, the twist turns the other way.
                h.ccw ^= mirror;
            }
            EntityKind::Ray(r) | EntityKind::XLine(r) => {
                t3(&mut r.base);
                r.dir = m.apply_vec(r.dir.xy()).normalized().to3(0.0);
            }
            EntityKind::Text(t) => edit::text_payload(t, GeometryRevision::LegacyProjectedV1, &Mat4::from_mat3(m), edit::TextOwner::Entity)?,
            EntityKind::AttDef(a) => {
                edit::text_payload(&mut a.text, a.geometry_revision, &Mat4::from_mat3(m), edit::TextOwner::Entity)?;
                a.try_transform_multiline(&Mat4::from_mat3(m))?;
            }
            EntityKind::MText(t) => edit::mtext_payload(t, GeometryRevision::LegacyProjectedV1, &Mat4::from_mat3(m), edit::TextOwner::Entity)?,
            EntityKind::Insert(i) => {
                t3(&mut i.insert);
                i.scale.x *= s;
                i.scale.y *= s;
                i.scale.z *= s;
                i.rotation = cadcraft_geom::norm_angle(i.rotation + rot);
                if mirror {
                    i.scale.y = -i.scale.y;
                    i.rotation = cadcraft_geom::norm_angle(rot - (i.rotation - rot));
                }
                for a in &mut i.attribs {
                    a.try_transform(&Mat4::from_mat3(m))?;
                }
                i.col_spacing *= s;
                i.row_spacing *= s;
            }
            EntityKind::Dimension(d) => {
                for p in [&mut d.defpt, &mut d.text_mid, &mut d.p13, &mut d.p14, &mut d.p15, &mut d.p16] {
                    t3(p);
                }
                if let DimKind::Linear { rotation } = &mut d.kind {
                    *rotation = cadcraft_geom::norm_angle(*rotation + rot);
                }
                d.leader_length *= s;
                d.marks.transform(&t3, rot, s, mirror);
                d.block = None;
            }
            EntityKind::MLine(ml) => {
                ml.vertices.iter_mut().for_each(|v| t3(&mut v.p));
                ml.vertices.iter_mut().flat_map(|v| v.cuts.iter_mut()).for_each(|c| (c.start, c.end) = (c.start * s, c.end * s));
                // Mirrored, the elements change sides.
                ml.scale *= if mirror { -s } else { s };
            }
            EntityKind::Leader(l) => l.vertices.iter_mut().for_each(t3),
            EntityKind::MLeader(ml) => {
                // Each root keeps its own landing and dogleg: the dogleg turns and scales with the map.
                let unit = |v: Vec3, by: Vec2| {
                    let len = by.len();
                    if len > 0.0 { (Vec3::new(by.x / len, by.y / len, v.z), len) } else { (v, 1.0) }
                };
                for r in &mut ml.roots {
                    r.branches.iter_mut().flat_map(|b| b.vertices.iter_mut()).for_each(t3);
                    t3(&mut r.landing);
                    let (dir, len) = unit(r.dogleg_dir, m.apply_vec(r.dogleg_dir.xy()));
                    (r.dogleg_dir, r.dogleg) = (dir, r.dogleg * len);
                }
                ml.context.text_dir = unit(ml.context.text_dir, m.apply_vec(ml.context.text_dir.xy())).0;
                if let Some(t) = &mut ml.text {
                    edit::mtext_payload(t, ml.context.geometry_revision, &Mat4::from_mat3(m), edit::TextOwner::MLeaderContent)?;
                }
                if let Some(b) = &mut ml.block {
                    // Transformed exactly like a block reference.
                    let mut entity = Entity::new(Handle(0), EntityKind::Insert(b.clone()));
                    entity.try_transform(&Mat4::from_mat3(m))?;
                    if let EntityKind::Insert(nb) = entity.kind {
                        *b = nb;
                    }
                }
            }
            EntityKind::Hatch(h) => {
                for l in &mut h.loops {
                    for v in &mut l.vertices {
                        v.p = m.apply(v.p);
                        if mirror {
                            v.bulge = -v.bulge;
                        }
                    }
                }
                h.scale *= s;
                h.angle = cadcraft_geom::norm_angle(h.angle + rot);
                h.origin = m.apply(h.origin);
                h.seed_points.iter_mut().for_each(|p| *p = m.apply(*p));
            }
            EntityKind::Solid(so) | EntityKind::Trace(so) => so.corners.iter_mut().for_each(t3),
            EntityKind::Face3d(f) => f.corners.iter_mut().for_each(t3),
            EntityKind::Region(r) => {
                for v in r.loops.iter_mut().flat_map(|l| l.vertices.iter_mut()) {
                    v.p = m.apply(v.p);
                    if mirror {
                        v.bulge = -v.bulge;
                    }
                }
            }
            EntityKind::Viewport(v) => {
                t3(&mut v.center);
                v.width *= s;
                v.height *= s;
            }
            EntityKind::Image(i) => {
                t3(&mut i.insert);
                i.u = m.apply_vec(i.u.xy()).to3(0.0);
                i.v = m.apply_vec(i.v.xy()).to3(0.0);
            }
            EntityKind::Wipeout(w) => w.boundary.iter_mut().for_each(|p| *p = m.apply(*p)),
            EntityKind::Table(t) => {
                if let Some(b) = &mut t.breaks {
                    b.height *= s;
                    b.spacing *= s;
                    b.heights.iter_mut().for_each(|h| *h *= s);
                    b.offsets.iter_mut().for_each(|o| *o = m.apply_vec(*o));
                }
                t3(&mut t.insert);
                t.col_widths.iter_mut().for_each(|w| *w *= s);
                t.row_heights.iter_mut().for_each(|h| *h *= s);
                t.text_height *= s;
            }
            EntityKind::Tolerance(t) => {
                t3(&mut t.insert);
                // Like text with MIRRTEXT = 0: a mirrored frame keeps reading left to right.
                let a = if mirror { t.rotation() } else { t.rotation() + rot };
                t.dir = Vec2::from_angle(cadcraft_geom::norm_angle(a)).to3(0.0);
            }
            EntityKind::Unknown(_) => return Err(crate::DocError::Invalid("This entity has no supported geometry transform.".into())),
        }
        Ok(())
    }

    /// Grip points (editable defining points), in display order.
    pub fn grips(&self) -> Vec<Vec2> {
        // A broken table has a grip for each part.
        if let EntityKind::Table(t) = self {
            return t.grip_points();
        }
        match self {
            EntityKind::Line(l) => vec![l.a.xy(), l.a.xy().mid(l.b.xy()), l.b.xy()],
            EntityKind::Point(p) => vec![p.p.xy()],
            EntityKind::Circle(c) => {
                let o = c.center.xy();
                vec![o, o + Vec2::X * c.radius, o + Vec2::Y * c.radius, o - Vec2::X * c.radius, o - Vec2::Y * c.radius]
            }
            EntityKind::Arc(a) => {
                let g = GArc::new(a.center.xy(), a.radius, a.start, a.end);
                vec![g.start_point(), g.mid_point(), g.end_point(), a.center.xy()]
            }
            EntityKind::Ellipse(e) => {
                let projected = e.projected();
                let c = projected.center;
                let mj = projected.major;
                let mn = projected.minor();
                vec![c, c + mj, c + mn, c - mj, c - mn]
            }
            // A spline-fit polyline is edited by its frame control points, not by the vertices of the curve it draws.
            EntityKind::LwPolyline(p) => {
                if let Some(frame) = p.spline_frame() {
                    return frame;
                }
                let mut g: Vec<Vec2> = Vec::new();
                let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
                // Compute the segments once, not once per vertex: rebuilding the whole segment
                // vector inside the loop is quadratic in the vertex count (CWE-407).
                let segs = pl.segments();
                for (i, v) in p.vertices.iter().enumerate() {
                    g.push(v.p);
                    if let Some(s) = segs.get(i) {
                        g.push(s.mid());
                    }
                }
                g
            }
            EntityKind::Polyline3d(p) => p.grip_points(),
            EntityKind::PolylineMesh(mesh) => mesh.vertices.iter().filter(|v| !v.is_face()).map(|v| v.vertex.position.xy()).collect(),
            EntityKind::Spline(s) => {
                if s.fit.is_empty() {
                    s.control.clone()
                } else {
                    s.fit.clone()
                }
            }
            EntityKind::Helix(h) => vec![h.center.xy(), h.start_point().xy(), h.end_point().xy()],
            EntityKind::Ray(r) | EntityKind::XLine(r) => vec![r.base.xy(), r.base.xy() + r.dir.xy()],
            EntityKind::Text(t) => vec![t.justify_point()],
            EntityKind::MText(t) => vec![t.insert.xy()],
            EntityKind::AttDef(a) => vec![a.text.justify_point()],
            EntityKind::Insert(i) => std::iter::once(i.insert.xy()).chain(i.attribs.iter().map(|a| a.text.justify_point())).collect(),
            EntityKind::Dimension(d) => vec![d.p13.xy(), d.p14.xy(), d.defpt.xy(), d.text_mid.xy()],
            EntityKind::MLine(m) => m.points(),
            EntityKind::Leader(l) => l.vertices.iter().map(|v| v.xy()).collect(),
            // The vertices of every line in file order, then the landing of each root.
            EntityKind::MLeader(m) => {
                m.lines().flat_map(|(_, b)| b.vertices.iter()).chain(m.roots.iter().map(|r| &r.landing)).map(|v| v.xy()).collect()
            }
            EntityKind::Hatch(h) => {
                let b = Bounds2::from_points(h.loops.iter().flat_map(|l| l.vertices.iter().map(|v| v.p)));
                vec![b.center()]
            }
            EntityKind::Solid(s) | EntityKind::Trace(s) => s.corners.iter().map(|c| c.xy()).collect(),
            EntityKind::Face3d(f) => f.corners.iter().map(|c| c.xy()).collect(),
            EntityKind::Region(r) => vec![r.bounds().center()],
            EntityKind::Viewport(v) => {
                let c = v.center.xy();
                let h = Vec2::new(v.width / 2.0, v.height / 2.0);
                Bounds2::new(c - h, c + h).corners().to_vec()
            }
            EntityKind::Image(i) => vec![i.insert.xy()],
            EntityKind::Wipeout(w) => w.boundary.clone(),
            EntityKind::Table(t) => vec![t.insert.xy()],
            EntityKind::Tolerance(t) => vec![t.insert.xy()],
            EntityKind::Unknown(_) => Vec::new(),
        }
    }
}

fn transform_text(t: &mut Text, m: &Mat3, s: f64, rot: f64, mirror: bool) {
    let q = m.apply(t.insert.xy());
    t.insert.x = q.x;
    t.insert.y = q.y;
    if let Some(a) = &mut t.align_pt {
        let q = m.apply(a.xy());
        a.x = q.x;
        a.y = q.y;
    }
    t.height *= s;
    // MIRRTEXT = 0 (the default): mirrored text keeps reading left to right.
    t.rotation = cadcraft_geom::norm_angle(t.rotation + if mirror { 0.0 } else { rot });
}

#[cfg(test)]
mod grip_tests {
    use super::*;
    use cadcraft_geom::PolyVertex;

    #[test]
    fn handle_parse_hex_accepts_only_hex_digits() {
        assert_eq!(Handle::parse_hex("1a"), Some(Handle(0x1A)));
        assert_eq!(Handle::parse_hex(" 1A "), Some(Handle(0x1A)));
        for bad in ["+1A", "-1A", "", "+", "1 A"] {
            assert_eq!(Handle::parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn polyline_grips_are_vertices_and_midpoints() {
        // Grips are each vertex plus the midpoint of each segment; the segment vector is computed
        // once rather than rebuilt per vertex (CWE-407, row 35).
        let verts: Vec<PolyVertex> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)].iter().map(|&(x, y)| PolyVertex::new(Vec2::new(x, y))).collect();
        let e = EntityKind::LwPolyline(LwPolyline {
            records: Default::default(),
            vertices: verts,
            closed: false,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
            heavy: false,
            fit: None,
        });
        let g = e.grips();
        // 3 vertices + 2 segment midpoints.
        assert_eq!(g.len(), 5, "{g:?}");
        assert!(g.contains(&Vec2::new(0.0, 0.0)) && g.contains(&Vec2::new(5.0, 0.0)));
    }
}

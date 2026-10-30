//! Entity::try_transform on a canonical payload goes through the family's transformed_canonical hook, atomically: the payload,
//! the plane normal and the thickness follow together. Each family is moved and turned about a tilted axis and compared with
//! its world primitives transformed by the same matrix; a mirror refuses with the entity unchanged; the families with no
//! canonical hook keep refusing; a legacy payload keeps the legacy path.

use cadcraft_color::Color;
use cadcraft_doc::{
    Arc, Attrib, Circle, Common, DimKind, Dimension, Ellipse, Entity, EntityKind, Face3d, GeometryRevision, HAlign, Handle, Hatch, HatchEdge,
    HatchLoop, Helix, HelixConstrain, HelixFrame, Leader, LeaderArrowLayout, LeaderFrame, Line, LwPolyline, MLine, MLineJust, MLineStyle,
    MLineVertex, MLineVertexFrame, MText, MeshKind, MeshVertex, Point, Polyline3d, PolylineFitVertex, PolylineMesh, Solid, Table, TableCell,
    TableLayout, TableLayoutText, Text, Tolerance, VAlign, Wipeout, WipeoutFrame, WorldPrimitive, arc_primitives, circle_primitives,
    hatch_primitives, helix_primitives, leader_primitives, lwpolyline_primitives, mleader_primitives, mline_primitives, mtext_primitives,
    solid_primitives, spline_primitives, table_primitives, text_primitives, tolerance_primitives, wipeout_primitives,
};
use cadcraft_doc::{Image, Insert, MLeader, MLeaderContext, RayLine};
use cadcraft_geom::{FitOptions, Mat4, PolyVertex, Spline, SplineSpace, Vec2, Vec3};

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-8
}

/// A rotation about the axis (1, 1, 0) by `angle` followed by a translation.
fn rotation(angle: f64, shift: Vec3) -> Mat4 {
    let (s, c) = angle.sin_cos();
    let k = Vec3::new(1.0, 1.0, 0.0).normalized();
    let t = 1.0 - c;
    Mat4 {
        m: [
            [t * k.x * k.x + c, t * k.x * k.y - s * k.z, t * k.x * k.z + s * k.y, shift.x],
            [t * k.x * k.y + s * k.z, t * k.y * k.y + c, t * k.y * k.z - s * k.x, shift.y],
            [t * k.x * k.z - s * k.y, t * k.y * k.z + s * k.x, t * k.z * k.z + c, shift.z],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

fn translation(v: Vec3) -> Mat4 {
    Mat4 { m: [[1.0, 0.0, 0.0, v.x], [0.0, 1.0, 0.0, v.y], [0.0, 0.0, 1.0, v.z], [0.0, 0.0, 0.0, 1.0]] }
}

fn uniform(k: f64) -> Mat4 {
    Mat4 { m: [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [0.0, 0.0, 0.0, 1.0]] }
}

fn mirror() -> Mat4 {
    Mat4 { m: [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] }
}

fn text(insert: Vec3) -> Text {
    Text {
        insert,
        align_pt: Some(Vec3::new(4.0, 5.0, insert.z)),
        height: 2.0,
        value: "T".into(),
        rotation: 0.3,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Center,
        valign: VAlign::Middle,
        backward: false,
        upside_down: false,
    }
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(1.0, 2.0, 3.0),
        height: 0.5,
        width: 4.0,
        attach: 5,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Body".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(0.6, 0.64, -0.48)),
        drawing_direction: None,
    }
}

fn wipeout() -> Wipeout {
    Wipeout {
        boundary: Vec::new(),
        display: Default::default(),
        frame: Some(WipeoutFrame {
            origin: Vec3::new(10.0, 1.0, 2.0),
            u: Vec3::new(4.0, 0.0, 0.5),
            v: Vec3::new(0.0, 2.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            clip_type: 2,
            clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
        }),
    }
}

fn helix() -> Helix {
    Helix {
        center: Vec3::new(1.0, 2.0, 3.0),
        base_radius: 2.0,
        top_radius: 2.0,
        height: 4.0,
        turns: 2.0,
        start_angle: 0.0,
        ccw: true,
        constrain: HelixConstrain::Height,
        frame: Some(HelixFrame { axis: Vec3::new(2.0, 0.0, 0.0), start: Vec3::new(1.0, 4.0, 3.0) }),
    }
}

fn mline() -> MLine {
    // OCS axes for NORMAL: -X, (0, -0.8, 0.6), (0, 0.6, 0.8).
    let to_world = |p: Vec3| Vec3::new(-p.x, -0.8 * p.y + 0.6 * p.z, 0.6 * p.y + 0.8 * p.z);
    let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 1.0);
    m.def.elements =
        vec![cadcraft_doc::MLineElement { offset: 1.0, ..Default::default() }, cadcraft_doc::MLineElement { offset: -1.0, ..Default::default() }];
    for x in [0.0, 10.0] {
        let mut v = MLineVertex::new(to_world(Vec3::new(x, 0.0, 1.0)));
        v.frame = Some(MLineVertexFrame {
            dir: to_world(Vec3::new(1.0, 0.0, 0.0)),
            miter: to_world(Vec3::new(0.0, 1.0, 0.0)),
            params: vec![vec![1.0, 0.0, 3.0, 5.0], vec![-1.0, 0.0]],
        });
        m.vertices.push(v);
    }
    m
}

fn hatch() -> Hatch {
    let edges = vec![
        HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
        HatchEdge::Arc { center: Vec2::new(4.0, 1.0), radius: 1.0, start: 0.5, end: 1.5, ccw: true },
        HatchEdge::Line { a: Vec2::new(4.0, 2.0), b: Vec2::new(0.0, 2.0) },
    ];
    Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![HatchLoop { flags: 1, edges, outer: true, ..HatchLoop::default() }],
        scale: 1.5,
        angle: 0.25,
        associative: false,
        style: 0,
        elevation: 2.0,
        gradient: None,
        origin: Vec2::new(1.0, 1.0),
        background: None,
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    }
}

fn polyline() -> LwPolyline {
    LwPolyline {
        records: Default::default(),
        vertices: vec![
            PolyVertex { p: Vec2::new(0.0, 0.0), bulge: 0.5, start_width: 0.25, end_width: 0.5 },
            PolyVertex::new(Vec2::new(3.0, 1.0)),
            PolyVertex::new(Vec2::new(4.0, 4.0)),
        ],
        closed: false,
        const_width: 0.0,
        elevation: 2.0,
        plinegen: false,
        heavy: false,
        fit: None,
    }
}

fn spline() -> Spline {
    Spline {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, 0.0)],
        weights: vec![1.0, 0.5, 1.0],
        fit: Vec::new(),
        closed: false,
        fit_opts: FitOptions::default(),
        space: Some(SplineSpace { control: vec![0.0, 4.0, 1.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 }),
        source: Default::default(),
    }
}

fn leader() -> Leader {
    Leader {
        vertices: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 1.0, 2.0), Vec3::new(5.0, 1.0, 2.5)],
        frame: Some(LeaderFrame {
            direction: Vec3::new(1.0, 0.0, 0.0),
            block_offset: Vec3::new(0.0, 0.5, 0.0),
            annotation_offset: Vec3::new(1.0, 0.25, 0.0),
            text_height: 0.2,
            text_width: 3.0,
        }),
        ..Leader::default()
    }
}

fn dimension() -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.4 },
        defpt: Vec3::new(1.0, 2.0, 3.0),
        text_mid: Vec3::new(0.5, 1.5, 3.0),
        p13: Vec3::new(0.0, 0.0, 3.0),
        p14: Vec3::new(4.0, 0.0, 3.0),
        p15: Vec3::new(2.0, 2.0, 3.0),
        p16: Vec3::new(1.0, 1.0, 3.0),
        text: String::new(),
        style: "Standard".into(),
        measurement: 4.0,
        text_rotation: 0.1,
        user_text_pos: true,
        block: Some("*D1".into()),
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn mesh() -> PolylineMesh {
    let vertex = |p: Vec3| MeshVertex {
        vertex: PolylineFitVertex { record: Default::default(), position: p, flags: 64, start_width: 0.0, end_width: 0.0, bulge: 0.0, tangent: 0.0 },
        indices: [0; 4],
    };
    let face = MeshVertex { vertex: PolylineFitVertex { flags: 128, position: Vec3::ZERO, ..vertex(Vec3::ZERO).vertex }, indices: [1, 2, 3, 0] };
    PolylineMesh {
        kind: MeshKind::Polyface,
        flags: 0,
        m_count: 0,
        n_count: 0,
        m_density: 0,
        n_density: 0,
        smooth_type: 0,
        vertices: vec![vertex(Vec3::new(0.0, 0.0, 0.0)), vertex(Vec3::new(1.0, 0.0, 2.0)), vertex(Vec3::new(1.0, 1.0, 3.0)), face],
        seqend: None,
    }
}

/// The plane of every fixture: tilted, with a source magnitude that is not 1.
const NORMAL: Vec3 = Vec3 { x: 0.0, y: 3.0, z: 4.0 };

fn entity(kind: EntityKind, normal: Vec3) -> Entity {
    let mut e = Entity::new(Handle(0x10), kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, thickness: 0.5, layer: "L".into(), color: Color::Index(3), ..Common::default() };
    e
}

fn tolerance() -> Tolerance {
    // The direction lies in the plane of NORMAL: (1, 0, 0).
    Tolerance { insert: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(1.0, 0.0, 0.0), style: "Standard".into(), text: "x".into() }
}

fn table() -> Table {
    let cell = |s: &str| TableCell { text: s.into(), ..TableCell::default() };
    Table {
        insert: Vec3::new(1.0, 2.0, 3.0),
        dir: Vec3::new(1.0, 0.0, 0.0),
        col_widths: vec![2.0, 3.0],
        row_heights: vec![1.0],
        cells: vec![vec![cell("a"), cell("b")]],
        ..Table::default()
    }
}

/// A multileader with two roots: its text in the tilted plane of NORMAL, or a block with an attribute in block space.
fn mleader(block: bool) -> MLeader {
    let mut m =
        MLeader::single(Vec3::new(10.0, 20.0, 3.0), 2.0, false, vec![vec![Vec3::new(0.0, 0.0, 3.0), Vec3::new(4.0, 6.0, 3.0)]], (!block).then(mtext));
    let mut second = cadcraft_doc::MLeaderRoot::new(Vec3::new(-10.0, 18.0, 3.0), 3.0, true, vec![vec![Vec3::new(-2.0, 0.0, 3.0)]]);
    second.id = 1;
    m.roots.push(second);
    m.arrow_size = 0.18;
    m.context = MLeaderContext {
        origin: Vec3::new(0.0, 0.0, 3.0),
        text_normal: NORMAL,
        text_dir: Vec3::new(0.6, 0.64, -0.48),
        block_normal: Vec3::new(1.0, 0.0, 2.0),
        geometry_revision: GeometryRevision::CanonicalV2,
        ..MLeaderContext::default()
    };
    if block {
        let mut attrib = Attrib::new("TAG", text(Vec3::new(1.0, 2.0, 3.0)));
        attrib.geometry_revision = GeometryRevision::CanonicalV2;
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::new(7.0, -3.0, 2.0),
            scale: Vec3::new(1.5, -2.0, 0.5),
            rotation: 0.4,
            attribs: vec![attrib],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
    }
    m
}

/// Every family's fixture, with its normal.
fn fixtures() -> Vec<(&'static str, Entity)> {
    let attdef = {
        let mut a = Attrib::new("TAG", text(Vec3::new(1.0, 2.0, 3.0)));
        a.multiline = Some(cadcraft_doc::MultilineAttrib {
            geometry_revision: GeometryRevision::CanonicalV2,
            normal: Vec3::new(1.0, 0.0, 2.0),
            mtext: mtext(),
            read_as: "Body".into(),
            embedded_46: None,
            type_72: None,
        });
        a
    };
    let mut heavy = polyline();
    heavy.heavy = true;
    let corners = [Vec3::new(0.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 3.0, 2.0), Vec3::new(2.0, 3.0, 2.0)];
    vec![
        ("TEXT", entity(EntityKind::Text(text(Vec3::new(1.0, 2.0, 3.0))), NORMAL)),
        ("MTEXT", entity(EntityKind::MText(mtext()), NORMAL)),
        ("ATTDEF", entity(EntityKind::AttDef(attdef), NORMAL)),
        ("TOLERANCE", entity(EntityKind::Tolerance(tolerance()), NORMAL)),
        ("TABLE", entity(EntityKind::Table(table()), NORMAL)),
        ("WIPEOUT", entity(EntityKind::Wipeout(wipeout()), NORMAL)),
        ("HELIX", entity(EntityKind::Helix(helix()), NORMAL)),
        ("MLINE", entity(EntityKind::MLine(mline()), NORMAL)),
        ("HATCH", entity(EntityKind::Hatch(hatch()), NORMAL)),
        ("CIRCLE", entity(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 }), NORMAL)),
        ("ARC", entity(EntityKind::Arc(Arc { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0, start: 0.3, end: 2.0 }), NORMAL)),
        ("LWPOLYLINE", entity(EntityKind::LwPolyline(polyline()), NORMAL)),
        ("POLYLINE", entity(EntityKind::LwPolyline(heavy), NORMAL)),
        ("SOLID", entity(EntityKind::Solid(Solid { corners }), NORMAL)),
        ("TRACE", entity(EntityKind::Trace(Solid { corners }), NORMAL)),
        ("SPLINE", entity(EntityKind::Spline(spline()), NORMAL)),
        ("LEADER", entity(EntityKind::Leader(leader()), NORMAL)),
        ("DIMENSION", entity(EntityKind::Dimension(dimension()), NORMAL)),
        // The major axis of the ellipse lies in the plane of NORMAL = (0, 3, 4).
        (
            "ELLIPSE",
            entity(
                EntityKind::Ellipse(Ellipse {
                    center: Vec3::new(1.0, 2.0, 3.0),
                    major: Vec3::new(4.0, 0.0, 0.0),
                    normal: NORMAL,
                    ratio: 0.5,
                    start: 0.3,
                    end: 2.0,
                }),
                NORMAL,
            ),
        ),
        ("LINE", entity(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(4.0, 1.0, 2.0) }), NORMAL)),
        ("POINT", entity(EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.5 }), NORMAL)),
        (
            "POLYLINE3D",
            entity(
                EntityKind::Polyline3d(Polyline3d {
                    records: Default::default(),
                    points: vec![Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0), Vec3::new(5.0, 1.0, -2.0)],
                    closed: false,
                    fit: None,
                }),
                NORMAL,
            ),
        ),
        ("3DFACE", entity(EntityKind::Face3d(Face3d { corners, hidden_edges: 2 }), NORMAL)),
        ("MLEADER", entity(EntityKind::MLeader(mleader(false)), NORMAL)),
        ("MLEADER block", entity(EntityKind::MLeader(mleader(true)), NORMAL)),
        ("MESH", entity(EntityKind::PolylineMesh(mesh()), NORMAL)),
        ("RAY", entity(EntityKind::Ray(RayLine { base: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.6, 0.8) }), NORMAL)),
        ("XLINE", entity(EntityKind::XLine(RayLine { base: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.6, 0.8) }), NORMAL)),
        (
            "IMAGE",
            entity(
                EntityKind::Image(Image {
                    insert: Vec3::new(1.0, 2.0, 3.0),
                    u: Vec3::new(0.5, 0.0, 0.0),
                    v: Vec3::new(0.0, 0.5, 0.25),
                    size: Vec2::new(100.0, 50.0),
                    clip: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)],
                    clipping: true,
                    ..Image::default()
                }),
                NORMAL,
            ),
        ),
    ]
}

/// The world primitives of a fixture, from the builders (independent of the hooks), or `None` for the dimension.
fn primitives(e: &Entity) -> Option<Vec<WorldPrimitive>> {
    let (id, n) = (&Mat4::IDENTITY, e.common.extrusion);
    let layout_tolerance = cadcraft_doc::GdtLayout {
        frame: vec![cadcraft_doc::GdtPrim::Line(vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, -2.0)])],
        ..Default::default()
    };
    let layout_table = TableLayout {
        fills: Vec::new(),
        lines: vec![[Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)]],
        texts: vec![TableLayoutText { at: Vec2::new(0.1, -0.1), attach: 1, height: 0.2, style: "Standard".into(), text: "cell".into() }],
    };
    Some(match &e.kind {
        EntityKind::Text(t) => text_primitives(t, n, id).unwrap(),
        EntityKind::MText(t) => mtext_primitives(t, n, id).unwrap(),
        EntityKind::AttDef(a) => {
            let mut out = text_primitives(&a.text, n, id).unwrap();
            if let Some(m) = &a.multiline {
                out.extend(mtext_primitives(&m.mtext, m.normal, id).unwrap());
            }
            out
        }
        EntityKind::Tolerance(t) => tolerance_primitives(t, n, &layout_tolerance, "Standard", id).unwrap(),
        EntityKind::Table(t) => table_primitives(t, n, &layout_table, id).unwrap(),
        EntityKind::Wipeout(w) => wipeout_primitives(w, id).unwrap(),
        EntityKind::Helix(h) => helix_primitives(h, id).unwrap(),
        EntityKind::MLine(m) => mline_primitives(m, n, id).unwrap(),
        EntityKind::Hatch(h) => hatch_primitives(h, n, id).unwrap(),
        EntityKind::Circle(c) => circle_primitives(c, n, id).unwrap(),
        EntityKind::Arc(a) => arc_primitives(a, n, id).unwrap(),
        EntityKind::LwPolyline(p) => lwpolyline_primitives(p, n, id).unwrap(),
        EntityKind::Solid(s) | EntityKind::Trace(s) => solid_primitives(s, n, id).unwrap(),
        EntityKind::Spline(s) => spline_primitives(s, id).unwrap(),
        EntityKind::Leader(l) => leader_primitives(l, n, &LeaderArrowLayout::default(), id).unwrap(),
        EntityKind::MLeader(m) => mleader_primitives(m, Vec3::new(1.0, 2.0, 3.0), &LeaderArrowLayout::default(), id).unwrap().primitives,
        EntityKind::Ellipse(_) | EntityKind::Line(_) | EntityKind::Point(_) => e.world_geometry(id).unwrap().primitives().to_vec(),
        EntityKind::Polyline3d(p) => p.points.windows(2).map(|w| WorldPrimitive::Line { a: w[0], b: w[1] }).collect(),
        EntityKind::Face3d(f) => f.corners.windows(2).map(|w| WorldPrimitive::Line { a: w[0], b: w[1] }).collect(),
        EntityKind::PolylineMesh(m) => m.vertices.iter().filter(|v| !v.is_face()).map(|v| WorldPrimitive::Point(v.vertex.position)).collect(),
        _ => return None,
    })
}

/// A point on a conic at the parameter `t`.
fn conic_at(p: &WorldPrimitive, t: f64) -> Vec3 {
    match p {
        WorldPrimitive::Conic { center, axis_u, axis_v, .. } => *center + *axis_u * t.cos() + *axis_v * t.sin(),
        other => panic!("{other:?}"),
    }
}

/// Whether `p` lies on the (full) conic `c`: `p - center = u cos t + v sin t` for some t.
fn on_conic(c: &WorldPrimitive, p: Vec3) -> bool {
    let WorldPrimitive::Conic { center, axis_u, axis_v, .. } = c else { return false };
    let d = p - *center;
    // Solve d = u x + v y in the least-squares sense, then check the residual and x^2 + y^2 = 1.
    let (uu, uv, vv) = (axis_u.dot(*axis_u), axis_u.dot(*axis_v), axis_v.dot(*axis_v));
    let det = uu * vv - uv * uv;
    if det.abs() < 1e-12 {
        return false;
    }
    let (du, dv) = (d.dot(*axis_u), d.dot(*axis_v));
    let (x, y) = ((du * vv - dv * uv) / det, (dv * uu - du * uv) / det);
    near(*axis_u * x + *axis_v * y, d) && (x * x + y * y - 1.0).abs() < 1e-8
}

/// The same primitive up to the parametrisation of its conics (an arc's angles turn with its OCS).
fn same(a: &WorldPrimitive, b: &WorldPrimitive) -> bool {
    use WorldPrimitive as W;
    match (a, b) {
        (W::Line { a: a0, b: b0 }, W::Line { a: a1, b: b1 }) => near(*a0, *a1) && near(*b0, *b1),
        (W::Point(p), W::Point(q)) => near(*p, *q),
        (W::Conic { start: s0, end: e0, .. }, W::Conic { start: s1, end: e1, .. }) => {
            let (sweep0, sweep1) = (cadcraft_geom::ccw_sweep(*s0, *e0), cadcraft_geom::ccw_sweep(*s1, *e1));
            if sweep0 >= std::f64::consts::TAU - 1e-9 {
                // A full conic has no start: the same curve means the same points on it.
                return sweep1 >= std::f64::consts::TAU - 1e-9 && [0.0, 1.0, 2.5, 4.0].iter().all(|t| on_conic(b, conic_at(a, *t)));
            }
            (sweep0 - sweep1).abs() < 1e-8 && [0.0, 0.5, 1.0].iter().all(|f| near(conic_at(a, s0 + sweep0 * f), conic_at(b, s1 + sweep1 * f)))
        }
        (W::Spline { control: c0, knots: k0, weights: w0, .. }, W::Spline { control: c1, knots: k1, weights: w1, .. }) => {
            k0 == k1 && w0 == w1 && c0.len() == c1.len() && c0.iter().zip(c1).all(|(p, q)| near(*p, *q))
        }
        (W::Text(t0), W::Text(t1)) => {
            near(t0.frame.origin, t1.frame.origin)
                && near(t0.frame.x_axis, t1.frame.x_axis)
                && near(t0.frame.y_axis, t1.frame.y_axis)
                && near(t0.frame.normal, t1.frame.normal)
                && t0.align.zip(t1.align).is_none_or(|(a, b)| near(a, b))
        }
        (W::Fill(f0), W::Fill(f1)) => {
            f0.loops.len() == f1.loops.len()
                && f0.loops.iter().zip(&f1.loops).all(|(l0, l1)| l0.len() == l1.len() && l0.iter().zip(l1).all(|(p, q)| same(p, q)))
                && match (&f0.kind, &f1.kind) {
                    (cadcraft_doc::WorldFillKind::Pattern(p0), cadcraft_doc::WorldFillKind::Pattern(p1)) => {
                        let direction = |p: &cadcraft_doc::WorldPattern| p.x_axis * p.angle.cos() + p.y_axis * p.angle.sin();
                        near(p0.origin, p1.origin) && near(direction(p0), direction(p1)) && (p0.scale - p1.scale).abs() < 1e-9
                    }
                    (x, y) => x == y,
                }
        }
        _ => false,
    }
}

fn moves() -> Vec<(&'static str, Mat4)> {
    vec![("move", translation(Vec3::new(5.0, -3.0, 2.0))), ("rotate about a tilted axis", rotation(0.6, Vec3::new(1.0, 2.0, 3.0)))]
}

#[test]
fn a_canonical_move_and_a_tilted_rotation_give_the_hooks_result_for_every_family() {
    for (name, e) in fixtures() {
        for (what, m) in moves() {
            let moved = e.transformed(&m).unwrap_or_else(|err| panic!("{name} {what}: {err}"));
            assert_eq!(
                (moved.handle, moved.geometry_revision, moved.common.layer.as_str()),
                (e.handle, GeometryRevision::CanonicalV2, "L"),
                "{name}"
            );
            if let (Some(before), Some(after)) = (primitives(&e), primitives(&moved)) {
                let expected: Vec<WorldPrimitive> = before.iter().map(|p| p.transformed(&m).unwrap()).collect();
                assert_eq!(expected.len(), after.len(), "{name} {what}");
                for (i, (x, y)) in expected.iter().zip(&after).enumerate() {
                    assert!(same(x, y), "{name} {what}: primitive {i}: {x:?} vs {y:?}");
                }
            }
            // The plane follows the motion (a rigid one: the normal turns, its magnitude and the thickness stay).
            if !matches!(
                e.kind,
                EntityKind::Wipeout(_)
                    | EntityKind::Helix(_)
                    | EntityKind::Polyline3d(_)
                    | EntityKind::Face3d(_)
                    | EntityKind::PolylineMesh(_)
                    | EntityKind::Ray(_)
                    | EntityKind::XLine(_)
                    | EntityKind::Image(_)
                    | EntityKind::MLeader(_)
            ) {
                assert!(near(moved.common.extrusion, m.apply_vec(e.common.extrusion)), "{name} {what}: normal {:?}", moved.common.extrusion);
            }
            assert_eq!(moved.common.thickness, 0.5, "{name} {what}");
        }
    }
}

#[test]
fn the_dimension_follows_a_motion_point_by_point() {
    let e = fixtures().into_iter().find(|(n, _)| *n == "DIMENSION").unwrap().1;
    let m = rotation(0.6, Vec3::new(1.0, 2.0, 3.0));
    let moved = e.transformed(&m).unwrap();
    let (EntityKind::Dimension(a), EntityKind::Dimension(b)) = (&e.kind, &moved.kind) else { panic!("dimension") };
    assert!(near(b.defpt, m.apply(a.defpt)) && near(b.p13, m.apply(a.p13)) && near(b.p14, m.apply(a.p14)) && near(b.p15, m.apply(a.p15)));
    // The OCS text midpoint is the same world point, moved.
    let (old, new) = (cadcraft_geom::OcsFrame::new(e.common.extrusion).unwrap(), cadcraft_geom::OcsFrame::new(moved.common.extrusion).unwrap());
    assert!(near(new.to_world(b.text_mid), m.apply(old.to_world(a.text_mid))));
    assert_eq!(b.block, None, "the graphics block is regenerated");
}

#[test]
fn a_scale_scales_the_thickness_with_the_similarity_families() {
    for (name, e) in fixtures() {
        let moved = e.transformed(&uniform(2.0)).unwrap_or_else(|err| panic!("{name}: {err}"));
        let unscaled = matches!(
            e.kind,
            EntityKind::Wipeout(_)
                | EntityKind::Helix(_)
                | EntityKind::Spline(_)
                | EntityKind::Ellipse(_)
                | EntityKind::Polyline3d(_)
                | EntityKind::Face3d(_)
                | EntityKind::PolylineMesh(_)
                | EntityKind::Ray(_)
                | EntityKind::XLine(_)
                | EntityKind::Image(_)
                | EntityKind::MLeader(_)
        );
        let expected = if unscaled { 0.5 } else { 1.0 };
        assert_eq!(moved.common.thickness, expected, "{name}");
    }
}

#[test]
fn a_mirror_refuses_with_the_entity_unchanged_where_the_family_is_a_similarity() {
    for (name, e) in fixtures() {
        let before = e.clone();
        let mut candidate = e.clone();
        let result = candidate.try_transform(&mirror());
        // These are exact under any invertible affine map (a frame, a B-spline, an ellipse, WCS positions, a line's extrusion vector).
        if matches!(
            e.kind,
            EntityKind::Wipeout(_)
                | EntityKind::Spline(_)
                | EntityKind::Ellipse(_)
                | EntityKind::Line(_)
                | EntityKind::Point(_)
                | EntityKind::Polyline3d(_)
                | EntityKind::Face3d(_)
                | EntityKind::PolylineMesh(_)
                | EntityKind::Ray(_)
                | EntityKind::XLine(_)
                | EntityKind::Image(_)
        ) {
            assert!(result.is_ok(), "{name}");
        } else {
            assert!(result.is_err(), "{name}: a mirror must refuse");
            assert_eq!(candidate, before, "{name}: a refused edit changes nothing");
        }
    }
}

#[test]
fn families_with_no_canonical_hook_keep_refusing() {
    let cases = vec![EntityKind::Unknown(cadcraft_doc::Unknown { dxf_type: "X".into(), tags: Vec::new() })];
    for kind in cases {
        let e = entity(kind, NORMAL);
        let mut candidate = e.clone();
        assert!(candidate.try_transform(&rotation(0.6, Vec3::ZERO)).is_err(), "{}", e.kind.type_name());
        assert_eq!(candidate, e);
    }
}

/// A canonical block reference in the tilted plane of NORMAL, with a signed scale and a rotation.
fn block_reference() -> Insert {
    Insert {
        block: "B".into(),
        insert: Vec3::new(7.0, -3.0, 2.0),
        scale: Vec3::new(1.5, -2.0, 0.5),
        rotation: 0.4,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    }
}

#[test]
fn a_canonical_block_reference_follows_the_edit_through_its_block_matrix() {
    let e = entity(EntityKind::Insert(block_reference()), NORMAL);
    let base = Vec3::new(1.0, 2.0, 3.0);
    let matrix = |e: &Entity| {
        let EntityKind::Insert(i) = &e.kind else { panic!("insert") };
        i.world_transform(e.common.extrusion, base, 0, 0).unwrap()
    };
    let before = matrix(&e);
    let mut cases = moves();
    cases.push(("scale", uniform(2.0)));
    for (what, m) in cases {
        let moved = e.transformed(&m).unwrap_or_else(|err| panic!("{what}: {err}"));
        let want = m.mul(&before);
        for (row_a, row_b) in matrix(&moved).m.iter().zip(&want.m) {
            assert!(row_a.iter().zip(row_b).all(|(a, b)| (a - b).abs() < 1e-8), "{what}: {:?} vs {:?}", matrix(&moved), want);
        }
        assert_eq!(moved.common.thickness, 0.5, "{what}: the thickness stays");
        assert_eq!(moved.geometry_revision, GeometryRevision::CanonicalV2);
    }
}

#[test]
fn a_legacy_payload_keeps_the_legacy_path() {
    for (name, mut e) in fixtures() {
        e.geometry_revision = GeometryRevision::LegacyProjectedV1;
        e.common.extrusion = Vec3::Z;
        match &mut e.kind {
            EntityKind::AttDef(a) => a.multiline = None,
            // A legacy multileader has a legacy context and legacy attributes.
            EntityKind::MLeader(m) => {
                m.context.geometry_revision = GeometryRevision::LegacyProjectedV1;
                m.block.iter_mut().for_each(|b| b.attribs.clear());
            }
            // The legacy wipeout is a boundary in the XY plane.
            EntityKind::Wipeout(w) => {
                *w = Wipeout {
                    boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.0)],
                    display: Default::default(),
                    frame: None,
                }
            }
            _ => {}
        }
        if let EntityKind::AttDef(a) = &mut e.kind {
            a.multiline = None;
        }
        // A spatial turn has no legacy form; a planar move is as before.
        let before = e.clone();
        assert!(e.clone().try_transform(&rotation(0.6, Vec3::ZERO)).is_err(), "{name}: legacy refuses a spatial turn");
        let mut moved = e.clone();
        moved.try_transform(&translation(Vec3::new(5.0, 0.0, 0.0))).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_ne!(moved, before, "{name}: the legacy move happened");
        assert_eq!(moved.geometry_revision, GeometryRevision::LegacyProjectedV1, "{name}");
    }
}

#[test]
fn rays_construction_lines_and_images_follow_the_map() {
    for (name, e) in fixtures().into_iter().filter(|(n, _)| matches!(*n, "RAY" | "XLINE" | "IMAGE")) {
        for (what, m) in moves() {
            let moved = e.transformed(&m).unwrap_or_else(|err| panic!("{name} {what}: {err}"));
            match (&e.kind, &moved.kind) {
                (EntityKind::Ray(a) | EntityKind::XLine(a), EntityKind::Ray(b) | EntityKind::XLine(b)) => {
                    assert!(near(b.base, m.apply(a.base)) && near(b.dir, m.apply_vec(a.dir)), "{name} {what}");
                }
                (EntityKind::Image(a), EntityKind::Image(b)) => {
                    assert!(near(b.insert, m.apply(a.insert)) && near(b.u, m.apply_vec(a.u)) && near(b.v, m.apply_vec(a.v)), "{name} {what}");
                    assert_eq!((&b.clip, b.size, b.clipping), (&a.clip, a.size, true), "the clip stays in pixels");
                }
                _ => panic!("{name}: kind changed"),
            }
        }
        // A collapse refuses with the entity unchanged.
        let fold = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        let mut candidate = e.clone();
        assert!(candidate.try_transform(&fold).is_err(), "{name}");
        assert_eq!(candidate, e);
    }
}

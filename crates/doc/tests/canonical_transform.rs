//! Entity::try_transform on a canonical payload goes through the family's transformed_canonical hook, atomically: the payload,
//! the plane normal and the thickness follow together. Each family is moved and turned about a tilted axis and compared with
//! its world primitives transformed by the same matrix; a mirror refuses with the entity unchanged; the families with no
//! canonical hook keep refusing; a legacy payload keeps the legacy path.

use cadcraft_color::Color;
use cadcraft_doc::{
    Arc, Attrib, Circle, Common, DimKind, Dimension, Entity, EntityKind, GeometryRevision, HAlign, Handle, Hatch, HatchEdge, HatchLoop, Helix,
    HelixConstrain, HelixFrame, Leader, LeaderArrowLayout, LeaderFrame, LwPolyline, MLine, MLineJust, MLineStyle, MLineVertex, MLineVertexFrame,
    MText, Solid, Table, TableCell, TableLayout, TableLayoutText, Text, Tolerance, VAlign, Wipeout, WipeoutFrame, WorldPrimitive, arc_primitives,
    circle_primitives, hatch_primitives, helix_primitives, leader_primitives, lwpolyline_primitives, mline_primitives, mtext_primitives,
    solid_primitives, spline_primitives, table_primitives, text_primitives, tolerance_primitives, wipeout_primitives,
};
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
    }
}

fn wipeout() -> Wipeout {
    Wipeout {
        boundary: Vec::new(),
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
    let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 1.0);
    m.def.elements =
        vec![cadcraft_doc::MLineElement { offset: 1.0, ..Default::default() }, cadcraft_doc::MLineElement { offset: -1.0, ..Default::default() }];
    for x in [0.0, 10.0] {
        let mut v = MLineVertex::new(Vec3::new(x, 0.0, 1.0));
        v.frame = Some(MLineVertexFrame {
            dir: Vec3::new(1.0, 0.0, 0.0),
            miter: Vec3::new(0.0, 1.0, 0.0),
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

/// Every family's fixture, with its normal.
fn fixtures() -> Vec<(&'static str, Entity)> {
    let attdef = {
        let mut a = Attrib::new("TAG", text(Vec3::new(1.0, 2.0, 3.0)));
        a.multiline = Some(cadcraft_doc::MultilineAttrib {
            geometry_revision: GeometryRevision::CanonicalV2,
            normal: Vec3::new(1.0, 0.0, 2.0),
            mtext: mtext(),
            read_as: "Body".into(),
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
            if !matches!(e.kind, EntityKind::Wipeout(_) | EntityKind::Helix(_)) {
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
        let expected = if matches!(e.kind, EntityKind::Wipeout(_) | EntityKind::Helix(_) | EntityKind::Spline(_)) { 0.5 } else { 1.0 };
        assert_eq!(moved.common.thickness, expected, "{name}");
    }
}

#[test]
fn a_mirror_refuses_with_the_entity_unchanged_where_the_family_is_a_similarity() {
    for (name, e) in fixtures() {
        let before = e.clone();
        let mut candidate = e.clone();
        let result = candidate.try_transform(&mirror());
        // A wipeout frame and a B-spline are affine-invariant: a mirror is exact for them.
        if matches!(e.kind, EntityKind::Wipeout(_) | EntityKind::Spline(_)) {
            assert!(result.is_ok(), "{name}");
        } else {
            assert!(result.is_err(), "{name}: a mirror must refuse");
            assert_eq!(candidate, before, "{name}: a refused edit changes nothing");
        }
    }
}

#[test]
fn families_with_no_canonical_hook_keep_refusing() {
    let cases = vec![
        EntityKind::Line(cadcraft_doc::Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 2.0) }),
        EntityKind::Point(cadcraft_doc::Point { p: Vec3::ZERO, angle: 0.0 }),
        EntityKind::Ellipse(cadcraft_doc::Ellipse {
            center: Vec3::ZERO,
            major: Vec3::new(2.0, 0.0, 0.0),
            normal: Vec3::Z,
            ratio: 0.5,
            start: 0.0,
            end: 1.0,
        }),
        EntityKind::Insert(cadcraft_doc::Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    ];
    for kind in cases {
        let e = entity(kind, NORMAL);
        let mut candidate = e.clone();
        assert!(candidate.try_transform(&rotation(0.6, Vec3::ZERO)).is_err(), "{}", e.kind.type_name());
        assert_eq!(candidate, e);
    }
}

#[test]
fn a_legacy_payload_keeps_the_legacy_path() {
    for (name, mut e) in fixtures() {
        e.geometry_revision = GeometryRevision::LegacyProjectedV1;
        e.common.extrusion = Vec3::Z;
        match &mut e.kind {
            EntityKind::AttDef(a) => a.multiline = None,
            // The legacy wipeout is a boundary in the XY plane.
            EntityKind::Wipeout(w) => *w = Wipeout { boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.0)], frame: None },
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

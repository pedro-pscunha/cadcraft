//! Canonical fills (revision CanonicalV2): a WIPEOUT is written with the image frame and clip its record held. Each
//! field changed alone reaches the user DXF and the DWG, and the canonical reader returns what the writer wrote.

use acadrust::DwgReader;
use acadrust::entities::EntityType;
use cadcraft_doc::{
    Common, Drawing, Entity, EntityKind, GeometryRevision, Hatch, HatchEdge, HatchLoop, HatchSpline, Helix, HelixConstrain, HelixFrame, MLine,
    MLineJust, MLineStyle, MLineVertex, MLineVertexFrame, Wipeout, WipeoutDisplay, WipeoutFrame,
};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{Vec2, Vec3};

fn drawing(kind: EntityKind) -> Drawing {
    drawing_in(kind, Vec3::Z)
}

fn drawing_in(kind: EntityKind, normal: Vec3) -> Drawing {
    let mut d = Drawing::new_metric();
    let handle = d.new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, ..Common::default() };
    d.model.push(e);
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap()
}

fn record(text: &str, kind: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags).into_iter().find(|(k, _)| k == kind).unwrap_or_else(|| panic!("no {kind}")).1
}

fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    record(text, kind).iter().map(|t| (t.code, t.str())).collect()
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn wipeout(frame: WipeoutFrame) -> Wipeout {
    Wipeout { boundary: Vec::new(), display: Default::default(), frame: Some(frame) }
}

fn frame() -> WipeoutFrame {
    WipeoutFrame {
        origin: Vec3::new(10.0, 1.0, 2.0),
        u: Vec3::new(4.0, 0.0, 0.5),
        v: Vec3::new(0.0, 2.0, 0.0),
        size: Vec2::new(1.0, 1.0),
        clip_type: 2,
        clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, 0.5), Vec2::new(-0.5, -0.5)],
    }
}

#[test]
fn each_wipeout_frame_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut WipeoutFrame);
    let cases: Vec<(&str, Mutation)> = vec![
        ("origin z", |f| f.origin.z = 2.5),
        ("u x", |f| f.u.x = 4.5),
        ("u z", |f| f.u.z = 0.75),
        ("v y", |f| f.v.y = 2.5),
        ("v z", |f| f.v.z = 0.25),
        ("clip vertex", |f| f.clip[1].y = -0.25),
        ("clip count", |f| f.clip.truncate(4)),
    ];
    let make = |mutate: Mutation| {
        let mut f = frame();
        mutate(&mut f);
        (drawing(EntityKind::Wipeout(wipeout(f.clone()))), f)
    };
    let base = groups(&dxf(&make(|_| {}).0), "WIPEOUT");
    for (name, mutate) in cases {
        let (d, f) = make(mutate);
        let g = groups(&dxf(&d), "WIPEOUT");
        assert_ne!(g, base, "{name}");
        let p = |code: i32| {
            let n = |c: i32| g.iter().find(|t| t.0 == c).map(|t| t.1.trim().parse::<f64>().unwrap()).unwrap();
            Vec3::new(n(code), n(code + 10), n(code + 20))
        };
        assert!(near(p(10), f.origin) && near(p(11), f.u) && near(p(12), f.v), "{name}: {g:?}");
        let ys: Vec<f64> = g.iter().filter(|t| t.0 == 24).map(|t| t.1.trim().parse().unwrap()).collect();
        assert_eq!(ys, f.clip.iter().map(|q| q.y).collect::<Vec<_>>(), "{name}: the clip is written as held");
        let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(&d, "x.dwg").unwrap())).read().unwrap();
        let s = stored.entities().find_map(|e| if let EntityType::Wipeout(s) = e { Some(s.clone()) } else { None }).expect("a WIPEOUT");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.insertion_point), f.origin), "{name}: DWG origin");
        assert!(near(v(s.u_vector), f.u) && near(v(s.v_vector), f.v), "{name}: DWG basis {:?} {:?}", s.u_vector, s.v_vector);
        assert_eq!(s.clip_boundary_vertices.len(), f.clip.len(), "{name}: DWG clip count");
        assert!(
            s.clip_boundary_vertices.iter().zip(&f.clip).all(|(a, b)| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9),
            "{name}: DWG clip {:?}",
            s.clip_boundary_vertices
        );
    }
}

#[test]
fn the_display_settings_of_a_wipeout_survive_the_dxf_and_the_dwg() {
    let display = WipeoutDisplay { flags: 15, clipping: 0, brightness: 70, contrast: 30, fade: 20 };
    let canonical = Wipeout { display, ..wipeout(frame()) };
    let legacy = Wipeout { boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)], display, frame: None };
    for (name, w, revision) in [("canonical", canonical, GeometryRevision::CanonicalV2), ("legacy", legacy, GeometryRevision::LegacyProjectedV1)] {
        let mut d = Drawing::new_metric();
        let handle = d.new_handle();
        let mut e = Entity::new(handle, EntityKind::Wipeout(w));
        e.geometry_revision = revision;
        d.model.push(e);
        let g = groups(&dxf(&d), "WIPEOUT");
        let at = |code: i32| g.iter().find(|t| t.0 == code).map(|t| t.1.trim().to_string());
        let shown = [70, 280, 281, 282, 283].map(|c| at(c).unwrap_or_default());
        assert_eq!(shown, ["15", "0", "70", "30", "20"], "{name}: DXF groups {g:?}");
        let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
        let read = |d: &Drawing| d.model.iter().find_map(|e| if let EntityKind::Wipeout(w) = &e.kind { Some(w.display) } else { None });
        assert_eq!(read(&back), Some(display), "{name}: DXF round trip");
        let again = cadcraft_io::read(&cadcraft_io::write(&back, "x.dwg").unwrap(), "x.dwg").unwrap();
        assert_eq!(read(&again), Some(display), "{name}: DWG round trip");
    }
}

#[test]
fn the_canonical_reader_returns_what_the_canonical_writer_wrote() {
    let out = dxf(&drawing(EntityKind::Wipeout(wipeout(frame()))));
    let (_, kind) = cadcraft_io::canonical_fill_from_groups("WIPEOUT", &record(&out, "WIPEOUT")).unwrap();
    assert_eq!(kind, EntityKind::Wipeout(wipeout(frame())));
}

#[test]
fn the_canonical_reader_refuses_a_frame_that_spans_no_plane() {
    let tags = vec![
        Tag::f(10, 0.0),
        Tag::f(20, 0.0),
        Tag::f(30, 0.0),
        Tag::f(11, 1.0),
        Tag::f(21, 0.0),
        Tag::f(31, 0.0),
        Tag::f(12, 2.0),
        Tag::f(22, 0.0),
        Tag::f(32, 0.0),
    ];
    assert!(cadcraft_io::canonical_fill_from_groups("WIPEOUT", &tags).is_none());
}

#[test]
fn the_legacy_reader_still_projects_a_wipeout_to_xy() {
    let d = drawing(EntityKind::Wipeout(wipeout(frame())));
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    let e = back.model.iter().find(|e| matches!(e.kind, EntityKind::Wipeout(_))).unwrap();
    assert_eq!(e.geometry_revision, GeometryRevision::LegacyProjectedV1);
    let EntityKind::Wipeout(w) = &e.kind else { unreachable!() };
    assert!(w.frame.is_none() && !w.boundary.is_empty(), "no canonical import is active");
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

#[test]
fn each_helix_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Helix);
    let cases: Vec<(&str, Mutation)> = vec![
        ("center z", |h| h.center.z = 3.5),
        ("axis x", |h| h.frame.as_mut().unwrap().axis.x = 3.0),
        ("axis y", |h| h.frame.as_mut().unwrap().axis.y = 0.5),
        ("axis z", |h| h.frame.as_mut().unwrap().axis.z = 0.5),
        ("start z", |h| h.frame.as_mut().unwrap().start.z = 3.5),
        ("radius", |h| h.base_radius = 2.5),
        ("turns", |h| h.turns = 3.0),
        ("height", |h| h.height = 6.0),
        ("handedness", |h| h.ccw = false),
    ];
    let make = |mutate: Mutation| {
        let mut h = helix();
        mutate(&mut h);
        (drawing(EntityKind::Helix(h.clone())), h)
    };
    let base = groups(&dxf(&make(|_| {}).0), "HELIX");
    for (name, mutate) in cases {
        let (d, h) = make(mutate);
        let g = groups(&dxf(&d), "HELIX");
        assert_ne!(g, base, "{name}");
        // The helix data follow the AcDbHelix marker.
        let at = g.iter().position(|t| t.0 == 100 && t.1 == "AcDbHelix").unwrap();
        let data = &g[at..];
        let p = |code: i32| {
            let n = |c: i32| data.iter().find(|t| t.0 == c).map(|t| t.1.trim().parse::<f64>().unwrap()).unwrap();
            Vec3::new(n(code), n(code + 10), n(code + 20))
        };
        let f = h.frame.unwrap();
        assert!(near(p(10), h.center) && near(p(11), f.start) && near(p(12), f.axis), "{name}: {data:?}");
        let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(&d, "x.dwg").unwrap())).read().unwrap();
        let s = stored.entities().find_map(|e| if let EntityType::Helix(s) = e { Some(s.clone()) } else { None }).expect("a HELIX");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.axis_base_point), h.center) && near(v(s.start_point), f.start) && near(v(s.axis_vector), f.axis), "{name}: DWG frame");
        assert!((s.radius - h.base_radius).abs() < 1e-9 && (s.turns - h.turns).abs() < 1e-9, "{name}: DWG size");
        assert!((s.turn_height - h.turn_height()).abs() < 1e-9 && s.handedness == h.ccw, "{name}: DWG pitch");
        // The spline starts on the cylinder, in the direction of the start point.
        let first = s.spline.control_points.first().expect("a control point");
        let a = f.axis.normalized();
        let radial = f.start - h.center;
        let on_cylinder = h.center + (radial - a * radial.dot(a)).normalized() * h.base_radius;
        assert!(near(v(*first), on_cylinder), "{name}: DWG spline starts at {first:?}");
    }
}

#[test]
fn the_canonical_reader_returns_a_helix_as_the_canonical_writer_wrote_it() {
    let out = dxf(&drawing(EntityKind::Helix(helix())));
    let (_, kind) = cadcraft_io::canonical_fill_from_groups("HELIX", &record(&out, "HELIX")).unwrap();
    let EntityKind::Helix(read) = kind else { panic!("not a helix") };
    let want = helix();
    assert_eq!((read.frame, read.center, read.turns, read.ccw, read.constrain), (want.frame, want.center, want.turns, want.ccw, want.constrain));
    assert!(
        (read.base_radius - want.base_radius).abs() < 1e-9
            && (read.height - want.height).abs() < 1e-9
            && (read.top_radius - want.top_radius).abs() < 1e-6
    );
}

#[test]
fn the_canonical_reader_refuses_a_helix_without_an_axis() {
    let tags = vec![Tag::s(100, "AcDbHelix"), Tag::f(10, 0.0), Tag::f(20, 0.0), Tag::f(30, 0.0), Tag::f(11, 1.0), Tag::f(21, 0.0), Tag::f(31, 0.0)];
    assert!(cadcraft_io::canonical_fill_from_groups("HELIX", &tags).is_none());
}

fn mline() -> MLine {
    let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 2.0);
    for (p, dir) in [(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.6, 0.0, 0.8)), (Vec3::new(3.0, 0.0, 5.0), Vec3::new(0.6, 0.0, 0.8))] {
        let mut v = MLineVertex::new(p);
        v.frame = Some(MLineVertexFrame { dir, miter: Vec3::new(0.0, 1.0, 0.0), params: vec![vec![1.0, 5.0], vec![-1.0, 5.0]] });
        m.vertices.push(v);
    }
    m
}

fn dxf_in(kind: EntityKind, normal: Vec3) -> String {
    dxf(&drawing_in(kind, normal))
}

#[test]
fn each_mline_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut MLine, &mut Vec3);
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let cases: Vec<(&str, Mutation)> = vec![
        ("vertex z", |m, _| m.vertices[1].p.z = 5.5),
        ("direction x", |m, _| m.vertices[0].frame.as_mut().unwrap().dir.x = 0.8),
        ("direction z", |m, _| m.vertices[1].frame.as_mut().unwrap().dir.z = 0.6),
        ("miter x", |m, _| m.vertices[1].frame.as_mut().unwrap().miter.x = 0.25),
        ("miter z", |m, _| m.vertices[0].frame.as_mut().unwrap().miter.z = 0.5),
        ("parameter", |m, _| m.vertices[0].frame.as_mut().unwrap().params[1][1] = 4.0),
        ("break", |m, _| m.vertices[1].frame.as_mut().unwrap().params[0].extend([1.0, 2.0])),
        ("normal x", |_, n| n.x = 0.5),
        ("normal y", |_, n| n.y = 2.5),
    ];
    let make = |mutate: Mutation| {
        let (mut m, mut n) = (mline(), normal);
        mutate(&mut m, &mut n);
        (drawing_in(EntityKind::MLine(m.clone()), n), m, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "MLINE");
    for (name, mutate) in cases {
        let (d, m, n) = make(mutate);
        let g = groups(&dxf(&d), "MLINE");
        assert_ne!(g, base, "{name}");
        let all = |code: i32| -> Vec<f64> { g.iter().filter(|t| t.0 == code).map(|t| t.1.trim().parse().unwrap()).collect() };
        let triples = |code: i32| -> Vec<Vec3> {
            all(code).into_iter().zip(all(code + 10)).zip(all(code + 20)).map(|((x, y), z)| Vec3::new(x, y, z)).collect()
        };
        let frames: Vec<_> = m.vertices.iter().map(|v| v.frame.as_ref().unwrap()).collect();
        assert_eq!(triples(11), m.vertices.iter().map(|v| v.p).collect::<Vec<_>>(), "{name}");
        assert_eq!(triples(12), frames.iter().map(|f| f.dir).collect::<Vec<_>>(), "{name}");
        assert_eq!(triples(13), frames.iter().map(|f| f.miter).collect::<Vec<_>>(), "{name}");
        assert!(near(triples(210)[0], n), "{name}");
        let written: Vec<f64> = all(41);
        let want: Vec<f64> = frames.iter().flat_map(|f| f.params.iter().flatten().copied()).collect();
        assert_eq!(written, want, "{name}: the parameters are written as held");
        let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(&d, "x.dwg").unwrap())).read().unwrap();
        let s = stored.entities().find_map(|e| if let EntityType::MLine(s) = e { Some(s.clone()) } else { None }).expect("an MLINE");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.normal), n), "{name}: DWG normal {:?}", s.normal);
        for (k, sv) in s.vertices.iter().enumerate() {
            let f = frames[k];
            assert!(near(v(sv.position), m.vertices[k].p) && near(v(sv.direction), f.dir) && near(v(sv.miter), f.miter), "{name}: DWG vertex {k}");
            let params: Vec<Vec<f64>> = sv.segments.iter().map(|g| g.parameters.clone()).collect();
            assert_eq!(params, f.params, "{name}: DWG parameters at vertex {k}");
        }
    }
}

#[test]
fn the_canonical_reader_returns_an_mline_as_the_canonical_writer_wrote_it() {
    let normal = Vec3::new(0.0, 2.0, 0.0);
    let out = dxf_in(EntityKind::MLine(mline()), normal);
    let (c, kind) = cadcraft_io::canonical_fill_from_groups("MLINE", &record(&out, "MLINE")).unwrap();
    assert_eq!(c.extrusion, normal);
    let EntityKind::MLine(read) = kind else { panic!("not an MLINE") };
    let want = mline();
    assert_eq!(read.vertices, want.vertices);
    assert_eq!((read.scale, read.justification, read.closed), (want.scale, want.justification, want.closed));
}

#[test]
fn the_canonical_reader_refuses_an_mline_without_a_plane() {
    let out = dxf_in(EntityKind::MLine(mline()), Vec3::Z);
    let mut tags = record(&out, "MLINE");
    tags.retain(|t| t.code != 210 && t.code != 220 && t.code != 230);
    tags.extend([Tag::f(210, 0.0), Tag::f(220, 0.0), Tag::f(230, 0.0)]);
    assert!(cadcraft_io::canonical_fill_from_groups("MLINE", &tags).is_none());
}

fn hatch() -> Hatch {
    let spline = HatchSpline {
        degree: 3,
        rational: true,
        periodic: false,
        knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(2.0, 2.0), Vec2::new(3.0, 0.0)],
        weights: vec![1.0, 0.5, 0.5, 1.0],
        fit: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)],
        start_tangent: Vec2::new(1.0, 1.0),
        end_tangent: Vec2::new(1.0, -1.0),
    };
    let edges = vec![
        HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
        HatchEdge::Arc { center: Vec2::new(1.0, 0.5), radius: 2.0, start: 0.25, end: 1.5, ccw: true },
        HatchEdge::Ellipse { center: Vec2::new(5.0, 5.0), major: Vec2::new(2.0, 0.0), ratio: 0.5, start: 0.0, end: 3.0, ccw: false },
        HatchEdge::Spline(spline),
    ];
    let polygon = vec![cadcraft_geom::PolyVertex::new(Vec2::new(0.0, 0.0)), cadcraft_geom::PolyVertex::with_bulge(Vec2::new(1.0, 0.0), 0.5)];
    Hatch {
        pattern: "SOLID".into(),
        solid: true,
        loops: vec![
            HatchLoop { flags: 1, edges, outer: true, ..HatchLoop::default() },
            HatchLoop { flags: 2, edges: vec![HatchEdge::Polyline { vertices: polygon, closed: true }], ..HatchLoop::default() },
        ],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 2.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    }
}

/// `a` and `b` are the same edges up to the rounding of the degrees in the file.
fn same_edges(a: &HatchEdge, b: &HatchEdge) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() < 1e-9;
    let near2 = |x: Vec2, y: Vec2| near(x.x, y.x) && near(x.y, y.y);
    match (a, b) {
        (HatchEdge::Line { a: a0, b: b0 }, HatchEdge::Line { a: a1, b: b1 }) => near2(*a0, *a1) && near2(*b0, *b1),
        (
            HatchEdge::Arc { center: c0, radius: r0, start: s0, end: e0, ccw: w0 },
            HatchEdge::Arc { center: c1, radius: r1, start: s1, end: e1, ccw: w1 },
        ) => near2(*c0, *c1) && near(*r0, *r1) && near(*s0, *s1) && near(*e0, *e1) && w0 == w1,
        (
            HatchEdge::Ellipse { center: c0, major: m0, ratio: r0, start: s0, end: e0, ccw: w0 },
            HatchEdge::Ellipse { center: c1, major: m1, ratio: r1, start: s1, end: e1, ccw: w1 },
        ) => near2(*c0, *c1) && near2(*m0, *m1) && near(*r0, *r1) && near(*s0, *s1) && near(*e0, *e1) && w0 == w1,
        (HatchEdge::Spline(p), HatchEdge::Spline(q)) => p == q,
        (HatchEdge::Polyline { vertices: v0, closed: c0 }, HatchEdge::Polyline { vertices: v1, closed: c1 }) => v0 == v1 && c0 == c1,
        _ => false,
    }
}

#[test]
fn each_hatch_edge_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    type Mutation = fn(&mut Hatch, &mut Vec3);
    fn edge(h: &mut Hatch, k: usize) -> &mut HatchEdge {
        &mut h.loops[0].edges[k]
    }
    fn spline(h: &mut Hatch) -> &mut HatchSpline {
        match &mut h.loops[0].edges[3] {
            HatchEdge::Spline(s) => s,
            _ => unreachable!(),
        }
    }
    let cases: Vec<(&str, Mutation)> = vec![
        ("line end", |h, _| {
            if let HatchEdge::Line { b, .. } = edge(h, 0) {
                b.y = 0.5;
            }
        }),
        ("arc radius", |h, _| {
            if let HatchEdge::Arc { radius, .. } = edge(h, 1) {
                *radius = 2.5;
            }
        }),
        ("arc end", |h, _| {
            if let HatchEdge::Arc { end, .. } = edge(h, 1) {
                *end = 1.25;
            }
        }),
        ("arc direction", |h, _| {
            if let HatchEdge::Arc { ccw, .. } = edge(h, 1) {
                *ccw = false;
            }
        }),
        ("ellipse major", |h, _| {
            if let HatchEdge::Ellipse { major, .. } = edge(h, 2) {
                major.y = 0.5;
            }
        }),
        ("ellipse ratio", |h, _| {
            if let HatchEdge::Ellipse { ratio, .. } = edge(h, 2) {
                *ratio = 0.25;
            }
        }),
        ("ellipse end", |h, _| {
            if let HatchEdge::Ellipse { end, .. } = edge(h, 2) {
                *end = 2.5;
            }
        }),
        ("spline control", |h, _| spline(h).control[1].y = 2.5),
        ("spline weight", |h, _| spline(h).weights[1] = 0.75),
        ("spline knot", |h, _| spline(h).knots[4] = 0.9),
        ("spline fit point", |h, _| spline(h).fit[1].x = 3.5),
        ("spline tangent", |h, _| spline(h).start_tangent.y = 2.0),
        ("polyline bulge", |h, _| {
            if let HatchEdge::Polyline { vertices, .. } = &mut h.loops[1].edges[0] {
                vertices[1].bulge = 0.25;
            }
        }),
        ("polyline closed", |h, _| {
            if let HatchEdge::Polyline { closed, .. } = &mut h.loops[1].edges[0] {
                *closed = false;
            }
        }),
        ("elevation", |h, _| h.elevation = 2.5),
        ("normal x", |_, n| n.x = 0.25),
        ("normal z", |_, n| n.z = 2.5),
    ];
    let make = |mutate: Mutation| {
        let (mut h, mut n) = (hatch(), Vec3::new(0.0, 0.0, 2.0));
        mutate(&mut h, &mut n);
        (drawing_in(EntityKind::Hatch(h.clone()), n), h, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "HATCH");
    for (name, mutate) in cases {
        let (d, h, n) = make(mutate);
        let out = dxf(&d);
        assert_ne!(groups(&out, "HATCH"), base, "{name}");
        // The canonical reader returns the edited hatch.
        let (c, kind) = cadcraft_io::canonical_fill_from_groups("HATCH", &record(&out, "HATCH")).unwrap_or_else(|| panic!("{name}: unreadable"));
        let EntityKind::Hatch(read) = kind else { panic!("not a hatch") };
        assert_eq!(c.extrusion, n, "{name}");
        assert_eq!(read.elevation, h.elevation, "{name}");
        assert_eq!(read.loops.len(), h.loops.len(), "{name}");
        for (l, (rl, hl)) in read.loops.iter().zip(&h.loops).enumerate() {
            assert!(rl.vertices.is_empty() && rl.flags == hl.flags, "{name}: loop {l} flags");
            assert_eq!(rl.edges.len(), hl.edges.len(), "{name}: loop {l}");
            assert!(rl.edges.iter().zip(&hl.edges).all(|(a, b)| same_edges(a, b)), "{name}: loop {l}: {:?} != {:?}", rl.edges, hl.edges);
        }
        // The DWG holds the same boundary.
        let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(&d, "x.dwg").unwrap())).read().unwrap();
        let s = stored.entities().find_map(|e| if let EntityType::Hatch(s) = e { Some(s.clone()) } else { None }).expect("a HATCH");
        let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
        assert!(near(v(s.normal), n) && (s.elevation - h.elevation).abs() < 1e-9, "{name}: DWG plane");
        assert_eq!(s.paths.len(), 2, "{name}");
        assert_eq!(s.paths[0].edges.len(), 4, "{name}");
        assert_eq!(s.paths[0].flags.bits(), 1, "{name}");
        // The edges come back through the DWG as they went in: the bridge converts the ellipse angles both ways.
        let back = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&cadcraft_io::write(&d, "x.dwg").unwrap()).unwrap()).unwrap();
        let (_, kind) = cadcraft_io::canonical_fill_from_groups("HATCH", &record(&back, "HATCH")).unwrap_or_else(|| panic!("{name}: DWG unreadable"));
        let EntityKind::Hatch(via_dwg) = kind else { panic!("not a hatch") };
        for (l, hl) in h.loops.iter().enumerate() {
            assert_eq!(via_dwg.loops[l].edges.len(), hl.edges.len(), "{name}: DWG loop {l}");
            for (k, (got, want)) in via_dwg.loops[l].edges.iter().zip(&hl.edges).enumerate() {
                assert!(same_edges(got, want), "{name}: DWG loop {l} edge {k}: {got:?} != {want:?}");
            }
        }
    }
}

#[test]
fn the_canonical_hatch_reader_keeps_what_the_legacy_one_flattens() {
    let out = dxf_in(EntityKind::Hatch(hatch()), Vec3::new(0.0, 0.0, 2.0));
    let back = cadcraft_io::read(out.as_bytes(), "x.dxf").unwrap();
    let EntityKind::Hatch(legacy) = &back.model.iter().find(|e| matches!(e.kind, EntityKind::Hatch(_))).unwrap().kind else { panic!("hatch") };
    assert!(legacy.loops[0].edges.is_empty() && !legacy.loops[0].vertices.is_empty(), "no canonical import is active");
    let (_, kind) = cadcraft_io::canonical_fill_from_groups("HATCH", &record(&out, "HATCH")).unwrap();
    let EntityKind::Hatch(exact) = kind else { panic!("hatch") };
    assert!(matches!(exact.loops[0].edges[3], HatchEdge::Spline(_)) && matches!(exact.loops[0].edges[2], HatchEdge::Ellipse { .. }));
}

#[test]
fn the_canonical_hatch_reader_refuses_an_unknown_edge_and_a_zero_normal() {
    let out = dxf_in(EntityKind::Hatch(hatch()), Vec3::Z);
    let mut tags = record(&out, "HATCH");
    let at = tags.iter().position(|t| t.code == 72 && t.i64() == 2).unwrap();
    tags[at] = Tag::i(72, 9);
    assert!(cadcraft_io::canonical_fill_from_groups("HATCH", &tags).is_none());
    let mut zero = record(&out, "HATCH");
    zero.iter_mut().filter(|t| matches!(t.code, 210 | 220 | 230)).for_each(|t| *t = Tag::f(t.code, 0.0));
    assert!(cadcraft_io::canonical_fill_from_groups("HATCH", &zero).is_none());
}

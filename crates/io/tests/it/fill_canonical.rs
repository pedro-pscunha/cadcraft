//! Canonical fills (revision CanonicalV2): a WIPEOUT is written with the image frame and clip its record held. Each
//! field changed alone reaches the user DXF and the DWG, and the canonical reader returns what the writer wrote.

use acadrust::DwgReader;
use acadrust::entities::EntityType;
use cadcraft_doc::{
    Common, Drawing, Entity, EntityKind, GeometryRevision, Helix, HelixConstrain, HelixFrame, MLine, MLineJust, MLineStyle, MLineVertex,
    MLineVertexFrame, Wipeout, WipeoutFrame,
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
    Wipeout { boundary: Vec::new(), frame: Some(frame) }
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

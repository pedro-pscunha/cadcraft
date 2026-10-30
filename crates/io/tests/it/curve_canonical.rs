//! Canonical CIRCLE, ARC and 2D polylines (revision CanonicalV2): the OCS center, angles and vertices, the normal with
//! its source magnitude and the signed thickness are written as held. Each field changed alone reaches the user DXF and
//! the DWG, and the canonical readers return what the writer wrote.

use acadrust::DwgReader;
use acadrust::entities::EntityType;
use cadcraft_doc::{Arc, Circle, Common, Drawing, Entity, EntityKind, GeometryRevision, LwPolyline, Solid};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};

const NORMAL: Vec3 = Vec3 { x: 0.0, y: 3.0, z: 4.0 };

fn drawing(kind: EntityKind, normal: Vec3, thickness: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    let handle = d.new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, thickness, ..Common::default() };
    d.model.push(e);
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap()
}

fn tags_of(text: &str, section: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == section).unwrap();
    records(&section.tags)
}

fn record(text: &str, kind: &str) -> Vec<Tag> {
    tags_of(text, "ENTITIES").into_iter().find(|(k, _)| k == kind).unwrap_or_else(|| panic!("no {kind}")).1
}

fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    record(text, kind).iter().map(|t| (t.code, t.str())).collect()
}

fn num(g: &[(i32, String)], code: i32) -> Option<f64> {
    g.iter().find(|t| t.0 == code).map(|t| t.1.trim().parse().unwrap())
}

fn point(g: &[(i32, String)], code: i32) -> Option<Vec3> {
    Some(Vec3::new(num(g, code)?, num(g, code + 10)?, num(g, code + 20)?))
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn dwg_entities(d: &Drawing) -> Vec<EntityType> {
    DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap().entities().cloned().collect()
}

fn v3(p: acadrust::types::Vector3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

#[test]
fn each_circle_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Circle, &mut Vec3, &mut f64);
    let cases: Vec<(&str, Mutation)> = vec![
        ("center x", |c, _, _| c.center.x = 1.5),
        ("center y", |c, _, _| c.center.y = 2.5),
        ("elevation", |c, _, _| c.center.z = 3.5),
        ("radius", |c, _, _| c.radius = 2.5),
        ("normal x", |_, n, _| n.x = 0.25),
        ("normal z", |_, n, _| n.z = 4.5),
        ("thickness", |_, _, t| *t = -1.25),
    ];
    let make = |mutate: Mutation| {
        let (mut c, mut n, mut t) = (Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 }, NORMAL, 0.5);
        mutate(&mut c, &mut n, &mut t);
        (drawing(EntityKind::Circle(c.clone()), n, t), c, n, t)
    };
    let base = groups(&dxf(&make(|_, _, _| {}).0), "CIRCLE");
    for (name, mutate) in cases {
        let (d, c, n, t) = make(mutate);
        let g = groups(&dxf(&d), "CIRCLE");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), c.center) && num(&g, 40) == Some(c.radius), "{name}: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n) && num(&g, 39) == Some(t), "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Circle(s) = e { Some(s) } else { None }).expect("a CIRCLE");
        assert!(near(v3(s.center), c.center) && (s.radius - c.radius).abs() < 1e-12, "{name}: DWG {:?}", s.center);
        assert!(near(v3(s.normal), n) && s.thickness == t, "{name}: DWG normal {:?} thickness {}", s.normal, s.thickness);
    }
}

#[test]
fn each_arc_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Arc, &mut Vec3);
    let cases: Vec<(&str, Mutation)> = vec![
        ("center z", |a, _| a.center.z = 3.5),
        ("radius", |a, _| a.radius = 2.5),
        ("start", |a, _| a.start = 0.5),
        ("end", |a, _| a.end = 2.5),
        ("normal y", |_, n| n.y = 3.5),
    ];
    let make = |mutate: Mutation| {
        let (mut a, mut n) = (Arc { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0, start: 0.25, end: 2.0 }, NORMAL);
        mutate(&mut a, &mut n);
        (drawing(EntityKind::Arc(a.clone()), n, 0.0), a, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "ARC");
    for (name, mutate) in cases {
        let (d, a, n) = make(mutate);
        let g = groups(&dxf(&d), "ARC");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), a.center) && near(point(&g, 210).unwrap(), n), "{name}");
        assert!((num(&g, 50).unwrap() - a.start.to_degrees()).abs() < 1e-9 && (num(&g, 51).unwrap() - a.end.to_degrees()).abs() < 1e-9, "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Arc(s) = e { Some(s) } else { None }).expect("an ARC");
        assert!(near(v3(s.center), a.center) && near(v3(s.normal), n), "{name}: DWG");
        assert!((s.start_angle - a.start).abs() < 1e-9 && (s.end_angle - a.end).abs() < 1e-9, "{name}: DWG angles {} {}", s.start_angle, s.end_angle);
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

#[test]
fn each_lwpolyline_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut LwPolyline, &mut Vec3, &mut f64);
    let cases: Vec<(&str, Mutation)> = vec![
        ("vertex x", |p, _, _| p.vertices[1].p.x = 3.5),
        ("bulge", |p, _, _| p.vertices[0].bulge = 0.75),
        ("width", |p, _, _| p.vertices[0].end_width = 0.75),
        ("elevation", |p, _, _| p.elevation = 2.5),
        ("closed", |p, _, _| p.closed = true),
        ("normal x", |_, n, _| n.x = 0.25),
        ("thickness", |_, _, t| *t = 1.25),
    ];
    let make = |mutate: Mutation| {
        let (mut p, mut n, mut t) = (polyline(), NORMAL, 0.0);
        mutate(&mut p, &mut n, &mut t);
        (drawing(EntityKind::LwPolyline(p.clone()), n, t), p, n, t)
    };
    let base = groups(&dxf(&make(|_, _, _| {}).0), "LWPOLYLINE");
    for (name, mutate) in cases {
        let (d, p, n, t) = make(mutate);
        let g = groups(&dxf(&d), "LWPOLYLINE");
        assert_ne!(g, base, "{name}");
        let xs: Vec<f64> = g.iter().filter(|t| t.0 == 10).map(|t| t.1.trim().parse().unwrap()).collect();
        assert_eq!(xs, p.vertices.iter().map(|v| v.p.x).collect::<Vec<_>>(), "{name}: the vertices are written as held");
        assert!(near(point(&g, 210).unwrap(), n) && num(&g, 38) == Some(p.elevation) && num(&g, 39).unwrap_or(0.0) == t, "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::LwPolyline(s) = e { Some(s) } else { None }).expect("an LWPOLYLINE");
        assert!(
            near(v3(s.normal), n) && (s.elevation - p.elevation).abs() < 1e-12 && s.is_closed == p.closed && s.thickness == t,
            "{name}: DWG plane"
        );
        assert_eq!(s.vertices.len(), p.vertices.len(), "{name}");
        for (a, b) in s.vertices.iter().zip(&p.vertices) {
            assert!((a.location.x - b.p.x).abs() < 1e-12 && (a.location.y - b.p.y).abs() < 1e-12 && a.bulge == b.bulge, "{name}: DWG vertex {a:?}");
            assert!(a.start_width == b.start_width && a.end_width == b.end_width, "{name}: DWG widths");
        }
    }
}

#[test]
fn a_negative_z_polyline_is_not_mirrored() {
    let p = polyline();
    let g = groups(&dxf(&drawing(EntityKind::LwPolyline(p.clone()), Vec3::new(0.0, 0.0, -1.0), 0.0)), "LWPOLYLINE");
    let xs: Vec<f64> = g.iter().filter(|t| t.0 == 10).map(|t| t.1.trim().parse().unwrap()).collect();
    assert_eq!(xs, vec![0.0, 3.0, 4.0], "the OCS x is written as held");
    assert_eq!(g.iter().filter(|t| t.0 == 42).map(|t| t.1.trim().parse::<f64>().unwrap()).collect::<Vec<_>>(), vec![0.5]);
}

#[test]
fn the_canonical_readers_return_what_the_canonical_writer_wrote() {
    let normal = Vec3::new(0.0, 3.0, -4.0);
    let circle = Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 };
    let out = dxf(&drawing(EntityKind::Circle(circle.clone()), normal, 0.5));
    let (c, kind) = cadcraft_io::canonical_curve_from_groups("CIRCLE", &record(&out, "CIRCLE")).unwrap();
    assert_eq!((c.extrusion, c.thickness, kind), (normal, 0.5, EntityKind::Circle(circle)));

    let arc = Arc { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0, start: 0.25, end: 2.0 };
    let out = dxf(&drawing(EntityKind::Arc(arc.clone()), normal, 0.0));
    let (_, kind) = cadcraft_io::canonical_curve_from_groups("ARC", &record(&out, "ARC")).unwrap();
    let EntityKind::Arc(read) = kind else { panic!("not an arc") };
    assert_eq!((read.center, read.radius), (arc.center, arc.radius));
    assert!((read.start - arc.start).abs() < 1e-12 && (read.end - arc.end).abs() < 1e-12, "no angle reversal under a negative Z");

    let out = dxf(&drawing(EntityKind::LwPolyline(polyline()), normal, 0.0));
    let (c, kind) = cadcraft_io::canonical_curve_from_groups("LWPOLYLINE", &record(&out, "LWPOLYLINE")).unwrap();
    assert_eq!(c.extrusion, normal);
    assert_eq!(kind, EntityKind::LwPolyline(polyline()), "no mirroring under a negative Z");
}

#[test]
fn the_canonical_readers_refuse_a_bad_radius_and_a_zero_normal() {
    let tags = |radius: f64, nz: f64| vec![Tag::f(10, 0.0), Tag::f(20, 0.0), Tag::f(30, 0.0), Tag::f(40, radius), Tag::f(230, nz)];
    assert!(cadcraft_io::canonical_curve_from_groups("CIRCLE", &tags(1.0, 1.0)).is_some());
    assert!(cadcraft_io::canonical_curve_from_groups("CIRCLE", &tags(-1.0, 1.0)).is_none());
    assert!(cadcraft_io::canonical_curve_from_groups("CIRCLE", &tags(f64::NAN, 1.0)).is_none());
    assert!(cadcraft_io::canonical_curve_from_groups("CIRCLE", &tags(1.0, 0.0)).is_none());
}

#[test]
fn a_heavy_polyline_with_fit_data_round_trips_in_the_ocs() {
    let mut pl = polyline();
    pl.heavy = true;
    pl.vertices[0].start_width = 0.0;
    pl.vertices[0].end_width = 0.0;
    let normal = Vec3::new(0.0, 0.0, -2.0);
    let out = dxf(&drawing(EntityKind::LwPolyline(pl.clone()), normal, 0.0));
    let entities = tags_of(&out, "ENTITIES");
    let at = entities.iter().position(|(k, _)| k == "POLYLINE").expect("a POLYLINE");
    let children: Vec<(String, Vec<Tag>)> = entities[at + 1..].iter().take_while(|(k, _)| k == "VERTEX" || k == "SEQEND").cloned().collect();
    let (c, kind) = cadcraft_io::canonical_polyline_from_groups(&entities[at].1, &children).unwrap();
    assert_eq!(c.extrusion, normal);
    let EntityKind::LwPolyline(read) = kind else { panic!("not a polyline") };
    assert!(read.heavy);
    assert_eq!(read.vertices.iter().map(|v| (v.p, v.bulge)).collect::<Vec<_>>(), pl.vertices.iter().map(|v| (v.p, v.bulge)).collect::<Vec<_>>());
    assert_eq!(read.elevation, pl.elevation);
    // The legacy reader mirrors the same record.
    let legacy = cadcraft_io::read(out.as_bytes(), "x.dxf").unwrap();
    let EntityKind::LwPolyline(mirrored) = &legacy.model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).unwrap().kind else {
        panic!("not a polyline")
    };
    assert_eq!(mirrored.vertices[1].p.x, -3.0);
}

fn solid() -> Solid {
    Solid { corners: [Vec3::new(0.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 3.0, 2.0), Vec3::new(2.0, 3.0, 2.0)] }
}

#[test]
fn each_solid_and_trace_field_alone_reaches_the_dxf_and_the_dwg() {
    type Mutation = fn(&mut Solid, &mut Vec3, &mut f64);
    let cases: Vec<(&str, Mutation)> = vec![
        ("first corner x", |s, _, _| s.corners[0].x = 0.5),
        ("third corner y", |s, _, _| s.corners[2].y = 3.5),
        // A DWG solid has one elevation for its four corners, as AutoCAD writes them.
        ("elevation", |s, _, _| s.corners.iter_mut().for_each(|c| c.z = 2.5)),
        ("normal y", |_, n, _| n.y = 3.5),
        ("thickness", |_, _, t| *t = -0.75),
    ];
    for (kind, trace) in [("SOLID", false), ("TRACE", true)] {
        let make = |mutate: Mutation| {
            let (mut s, mut n, mut t) = (solid(), NORMAL, 0.5);
            mutate(&mut s, &mut n, &mut t);
            let k = if trace { EntityKind::Trace(s.clone()) } else { EntityKind::Solid(s.clone()) };
            (drawing(k, n, t), s, n, t)
        };
        let base = groups(&dxf(&make(|_, _, _| {}).0), kind);
        for (name, mutate) in &cases {
            let (d, s, n, t) = make(*mutate);
            let g = groups(&dxf(&d), kind);
            assert_ne!(g, base, "{kind} {name}");
            for (i, c) in s.corners.iter().enumerate() {
                assert!(near(point(&g, 10 + i as i32).unwrap(), *c), "{kind} {name}: corner {i}");
            }
            assert!(near(point(&g, 210).unwrap(), n) && num(&g, 39) == Some(t), "{kind} {name}");
            let stored = dwg_entities(&d)
                .into_iter()
                .find_map(|e| if let EntityType::Solid(x) = e { Some(x) } else { None })
                .unwrap_or_else(|| panic!("a {kind} in the DWG"));
            let dwg = [stored.first_corner, stored.second_corner, stored.third_corner, stored.fourth_corner];
            assert!(dwg.iter().zip(&s.corners).all(|(a, b)| near(v3(*a), *b)), "{kind} {name}: DWG corners");
            assert!(near(v3(stored.normal), n) && stored.thickness == t && stored.is_trace == trace, "{kind} {name}: DWG plane");
        }
    }
}

#[test]
fn the_canonical_reader_returns_solids_and_traces_in_the_ocs() {
    let normal = Vec3::new(0.0, 3.0, -4.0);
    for (kind, trace) in [("SOLID", false), ("TRACE", true)] {
        let k = if trace { EntityKind::Trace(solid()) } else { EntityKind::Solid(solid()) };
        let out = dxf(&drawing(k.clone(), normal, 0.5));
        let (c, read) = cadcraft_io::canonical_curve_from_groups(kind, &record(&out, kind)).unwrap();
        assert_eq!((c.extrusion, c.thickness, read), (normal, 0.5, k), "{kind}");
    }
    // A triangle repeats its third corner.
    let tags = vec![Tag::f(10, 0.0), Tag::f(11, 1.0), Tag::f(12, 0.0), Tag::f(22, 1.0), Tag::f(230, 1.0)];
    let (_, read) = cadcraft_io::canonical_curve_from_groups("SOLID", &tags).unwrap();
    let EntityKind::Solid(s) = read else { panic!("solid") };
    assert_eq!(s.corners[3], s.corners[2]);
}

fn spline() -> cadcraft_geom::Spline {
    use cadcraft_geom::{FitOptions, Spline, SplineSpace};
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

/// A spline with fit points: DWG stores either the control scenario or the fit scenario, so its DWG leg checks the fit data.
fn fit_spline() -> cadcraft_geom::Spline {
    use cadcraft_geom::FitOptions;
    cadcraft_geom::Spline {
        fit: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)],
        fit_opts: FitOptions {
            start_tangent: Some(Vec2::new(1.0, 1.0)),
            end_tangent: Some(Vec2::new(1.0, -1.0)),
            tolerance: 0.25,
            ..FitOptions::default()
        },
        space: Some(cadcraft_geom::SplineSpace { control: vec![0.0, 4.0, 1.0], fit: vec![0.0, 1.5], start_tangent: 0.5, end_tangent: -0.5 }),
        ..spline()
    }
}

type SplineMutation = fn(&mut cadcraft_geom::Spline, &mut Vec3);

/// Each mutation alone changes the DXF, comes back from the canonical reader as written, and `dwg` holds for the DWG spline.
fn check_splines(
    base: cadcraft_geom::Spline,
    cases: Vec<(&str, SplineMutation)>,
    dwg: impl Fn(&str, &cadcraft_geom::Spline, &acadrust::entities::Spline),
) {
    let make = |mutate: SplineMutation| {
        let (mut s, mut n) = (base.clone(), NORMAL);
        mutate(&mut s, &mut n);
        (drawing(EntityKind::Spline(s.clone()), n, 0.0), s, n)
    };
    let base_groups = groups(&dxf(&make(|_, _| {}).0), "SPLINE");
    for (name, mutate) in cases {
        let (d, s, n) = make(mutate);
        let out = dxf(&d);
        assert_ne!(groups(&out, "SPLINE"), base_groups, "{name}");
        let (c, kind) = cadcraft_io::canonical_curve_from_groups("SPLINE", &record(&out, "SPLINE")).unwrap_or_else(|| panic!("{name}: unreadable"));
        assert_eq!(c.extrusion, n, "{name}");
        let EntityKind::Spline(read) = kind else { panic!("not a spline") };
        assert_eq!(
            (&read.knots, &read.control, &read.weights, &read.fit, read.degree),
            (&s.knots, &s.control, &s.weights, &s.fit, s.degree),
            "{name}"
        );
        assert_eq!(read.space, s.space, "{name}: the Z values");
        assert_eq!(
            (read.fit_opts.start_tangent, read.fit_opts.end_tangent, read.fit_opts.tolerance),
            (s.fit_opts.start_tangent, s.fit_opts.end_tangent, s.fit_opts.tolerance),
            "{name}"
        );
        let stored = dwg_entities(&d)
            .into_iter()
            .find_map(|e| if let EntityType::Spline(x) = e { Some(x) } else { None })
            .unwrap_or_else(|| panic!("{name}: a SPLINE in the DWG"));
        dwg(name, &s, &stored);
    }
}

#[test]
fn each_control_spline_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    let cases: Vec<(&str, SplineMutation)> = vec![
        ("control x", |s, _| s.control[1].x = 1.5),
        ("control z", |s, _| s.space.as_mut().unwrap().control[1] = 4.5),
        ("weight", |s, _| s.weights[1] = 0.75),
        ("knot", |s, _| s.knots[3] = 0.9),
        ("degree", |s, _| {
            s.degree = 1;
            s.knots = vec![0.0, 0.0, 0.5, 1.0, 1.0];
        }),
    ];
    check_splines(spline(), cases, |name, s, stored| {
        let zs = s.space.as_ref().unwrap();
        assert_eq!(stored.control_points.len(), s.control.len(), "{name}");
        for (i, p) in stored.control_points.iter().enumerate() {
            assert!(near(v3(*p), Vec3::new(s.control[i].x, s.control[i].y, zs.control[i])), "{name}: DWG control {i}: {p:?}");
        }
        assert_eq!((stored.knots.clone(), stored.weights.clone(), stored.degree as usize), (s.knots.clone(), s.weights.clone(), s.degree), "{name}");
    });
}

#[test]
fn each_fit_spline_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    let cases: Vec<(&str, SplineMutation)> = vec![
        ("fit point z", |s, _| s.space.as_mut().unwrap().fit[1] = 2.0),
        ("fit point x", |s, _| s.fit[1].x = 3.5),
        ("start tangent z", |s, _| s.space.as_mut().unwrap().start_tangent = 0.75),
        ("end tangent x", |s, _| s.fit_opts.end_tangent = Some(Vec2::new(2.0, -1.0))),
        ("tolerance", |s, _| s.fit_opts.tolerance = 0.5),
        ("normal x", |_, n| n.x = 0.25),
    ];
    check_splines(fit_spline(), cases, |name, s, stored| {
        let zs = s.space.as_ref().unwrap();
        assert_eq!(stored.fit_points.len(), s.fit.len(), "{name}");
        for (i, p) in stored.fit_points.iter().enumerate() {
            assert!(near(v3(*p), Vec3::new(s.fit[i].x, s.fit[i].y, zs.fit[i])), "{name}: DWG fit point {i}: {p:?}");
        }
        let (t0, t1) = (s.fit_opts.start_tangent.unwrap(), s.fit_opts.end_tangent.unwrap());
        assert!(near(v3(stored.begin_tangent), Vec3::new(t0.x, t0.y, zs.start_tangent)), "{name}: DWG start tangent {:?}", stored.begin_tangent);
        assert!(near(v3(stored.end_tangent), Vec3::new(t1.x, t1.y, zs.end_tangent)), "{name}: DWG end tangent {:?}", stored.end_tangent);
        assert!((stored.fit_tolerance - s.fit_opts.tolerance).abs() < 1e-12, "{name}: DWG fit tolerance {}", stored.fit_tolerance);
    });
}

#[test]
fn a_spatial_spline_is_not_flagged_planar_and_a_flat_one_is() {
    let flags = |s: cadcraft_geom::Spline| num(&groups(&dxf(&drawing(EntityKind::Spline(s), Vec3::Z, 0.0)), "SPLINE"), 70).unwrap() as i64;
    assert_eq!(flags(spline()) & 8, 0, "the controls leave the XY plane");
    let mut flat = spline();
    flat.space.as_mut().unwrap().control = vec![2.0, 2.0, 2.0];
    assert_eq!(flags(flat) & 8, 8, "the controls lie in a plane parallel to the normal's plane");
}

#[test]
fn the_canonical_spline_reader_refuses_mismatched_data() {
    let out = dxf(&drawing(EntityKind::Spline(spline()), Vec3::Z, 0.0));
    let mut tags = record(&out, "SPLINE");
    // Drop a Z value: the counts no longer match.
    let at = tags.iter().position(|t| t.code == 30).unwrap();
    tags.remove(at);
    assert!(cadcraft_io::canonical_curve_from_groups("SPLINE", &tags).is_none());
    // Knots that do not fit the control points.
    let mut tags = record(&out, "SPLINE");
    let at = tags.iter().position(|t| t.code == 40).unwrap();
    tags.remove(at);
    assert!(cadcraft_io::canonical_curve_from_groups("SPLINE", &tags).is_none());
}

fn leader() -> cadcraft_doc::Leader {
    cadcraft_doc::Leader {
        vertices: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 1.0, 2.0), Vec3::new(5.0, 1.0, 2.5)],
        spline: false,
        frame: Some(cadcraft_doc::LeaderFrame {
            direction: Vec3::new(1.0, 0.0, 0.0),
            block_offset: Vec3::new(0.0, 0.5, 0.0),
            annotation_offset: Vec3::new(1.0, 0.25, 0.0),
            text_height: 0.2,
            text_width: 3.0,
        }),
        ..cadcraft_doc::Leader::default()
    }
}

#[test]
fn each_leader_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    type Mutation = fn(&mut cadcraft_doc::Leader, &mut Vec3);
    fn frame(l: &mut cadcraft_doc::Leader) -> &mut cadcraft_doc::LeaderFrame {
        l.frame.as_mut().unwrap()
    }
    let cases: Vec<(&str, Mutation)> = vec![
        ("vertex z", |l, _| l.vertices[1].z = 2.5),
        ("vertex x", |l, _| l.vertices[2].x = 5.5),
        ("direction y", |l, _| frame(l).direction.y = 0.5),
        ("block offset z", |l, _| frame(l).block_offset.z = 0.75),
        ("annotation offset x", |l, _| frame(l).annotation_offset.x = 1.5),
        ("text height", |l, _| frame(l).text_height = 0.3),
        ("text width", |l, _| frame(l).text_width = 4.0),
        ("normal y", |_, n| n.y = 3.5),
    ];
    let make = |mutate: Mutation| {
        let (mut l, mut n) = (leader(), NORMAL);
        mutate(&mut l, &mut n);
        (drawing(EntityKind::Leader(l.clone()), n, 0.0), l, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "LEADER");
    for (name, mutate) in cases {
        let (d, l, n) = make(mutate);
        let out = dxf(&d);
        assert_ne!(groups(&out, "LEADER"), base, "{name}");
        let (c, kind) = cadcraft_io::canonical_curve_from_groups("LEADER", &record(&out, "LEADER")).unwrap_or_else(|| panic!("{name}: unreadable"));
        assert_eq!(c.extrusion, n, "{name}");
        assert_eq!(kind, EntityKind::Leader(l.clone()), "{name}");
        let stored = dwg_entities(&d)
            .into_iter()
            .find_map(|e| if let EntityType::Leader(x) = e { Some(x) } else { None })
            .unwrap_or_else(|| panic!("{name}: a LEADER in the DWG"));
        let f = l.frame.as_ref().unwrap();
        assert!(
            stored.vertices.iter().zip(&l.vertices).all(|(a, b)| near(v3(*a), *b)) && stored.vertices.len() == l.vertices.len(),
            "{name}: DWG vertices"
        );
        assert!(near(v3(stored.normal), n), "{name}: DWG normal {:?}", stored.normal);
        assert!(near(v3(stored.horizontal_direction), f.direction), "{name}: DWG direction {:?}", stored.horizontal_direction);
        assert!(near(v3(stored.block_offset), f.block_offset) && near(v3(stored.annotation_offset), f.annotation_offset), "{name}: DWG offsets");
        // A DWG leader has no text box size (AutoCAD derives it from the annotation): the sizes reach the DXF only.
    }
}

#[test]
fn the_canonical_leader_reader_keeps_z_and_the_legacy_one_drops_it() {
    let out = dxf(&drawing(EntityKind::Leader(leader()), Vec3::Z, 0.0));
    let (_, kind) = cadcraft_io::canonical_curve_from_groups("LEADER", &record(&out, "LEADER")).unwrap();
    let EntityKind::Leader(read) = kind else { panic!("not a leader") };
    assert_eq!(read.vertices[1].z, 2.0);
    let legacy = cadcraft_io::read(out.as_bytes(), "x.dxf").unwrap();
    let EntityKind::Leader(l) = &legacy.model.iter().find(|e| matches!(e.kind, EntityKind::Leader(_))).unwrap().kind else { panic!("leader") };
    assert!(l.vertices.iter().all(|v| v.z == 0.0) && l.frame.is_none(), "no canonical import is active");
    let tags = vec![Tag::f(10, 0.0), Tag::f(20, 0.0), Tag::f(10, 1.0)];
    assert!(cadcraft_io::canonical_curve_from_groups("LEADER", &tags).is_none(), "the vertex coordinates must match in count");
}

// Old serialized SPLINE and LEADER payloads.

use cadcraft_doc::{Promotion, promote_legacy_spline_leader};

/// An entity as an old document stored it: the JSON of a legacy payload, with no revision and no spatial fields.
fn old_entity(kind: EntityKind, normal: Vec3) -> Entity {
    let mut e = Entity::new(cadcraft_doc::Handle(0x100), kind);
    e.common = Common { extrusion: normal, ..Common::default() };
    let mut json = serde_json::to_value(&e).unwrap();
    let object = json.as_object_mut().unwrap();
    assert!(object.remove("geometryRevision").is_some() || e.geometry_revision == GeometryRevision::LegacyProjectedV1);
    let back: Entity = serde_json::from_value(json).unwrap();
    assert_eq!(back, e, "an old payload opens as the legacy entity it was");
    back
}

fn legacy_spline() -> cadcraft_geom::Spline {
    cadcraft_geom::Spline { space: None, fit: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)], ..spline() }
}

fn legacy_leader() -> cadcraft_doc::Leader {
    cadcraft_doc::Leader { frame: None, vertices: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 0.0)], ..leader() }
}

fn drawing_of(e: &Entity) -> Drawing {
    let mut d = Drawing::new_metric();
    d.model.push(e.clone());
    d
}

/// The groups of the record without its plane groups, which only the canonical writer adds.
fn without_plane(g: Vec<(i32, String)>) -> Vec<(i32, String)> {
    g.into_iter().filter(|(c, _)| !matches!(c, 210 | 220 | 230)).collect()
}

#[test]
fn an_old_plus_z_spline_keeps_its_2d_meaning_bit_for_bit_after_promotion_and_resave() {
    for normal in [Vec3::Z, Vec3::new(0.0, 0.0, 2.0)] {
        let mut e = old_entity(EntityKind::Spline(legacy_spline()), normal);
        let before = without_plane(groups(&dxf(&drawing_of(&e)), "SPLINE"));
        assert_eq!(promote_legacy_spline_leader(&mut e), Promotion::Promoted);
        assert_eq!(e.geometry_revision, GeometryRevision::CanonicalV2);
        let EntityKind::Spline(s) = &e.kind else { panic!("spline") };
        assert_eq!(s.space.as_ref().unwrap().control, vec![0.0; 3], "promoted at Z = 0");
        // Resaved by the canonical writer: every number is the text the legacy writer wrote (only the planar flag may
        // differ, and it stays set because the controls lie in the plane).
        let after = groups(&dxf(&drawing_of(&e)), "SPLINE");
        assert_eq!(point(&after, 210), Some(normal), "the plane is kept as it was");
        assert_eq!(without_plane(after), before, "{normal:?}: identical groups after promotion and resave");
        // And the canonical reader returns the same XY bits.
        let (_, kind) = cadcraft_io::canonical_curve_from_groups("SPLINE", &record(&dxf(&drawing_of(&e)), "SPLINE")).unwrap();
        let EntityKind::Spline(read) = kind else { panic!("spline") };
        assert_eq!((read.control, read.knots, read.weights, read.fit), (s.control.clone(), s.knots.clone(), s.weights.clone(), s.fit.clone()));
    }
}

#[test]
fn an_old_plus_z_leader_keeps_its_vertices_bit_for_bit_after_promotion_and_resave() {
    let mut e = old_entity(EntityKind::Leader(legacy_leader()), Vec3::Z);
    let before = groups(&dxf(&drawing_of(&e)), "LEADER");
    assert_eq!(promote_legacy_spline_leader(&mut e), Promotion::Promoted);
    let after = groups(&dxf(&drawing_of(&e)), "LEADER");
    let vertex_groups = |g: &[(i32, String)]| g.iter().filter(|(c, _)| matches!(c, 10 | 20 | 30 | 3 | 71 | 72 | 76)).cloned().collect::<Vec<_>>();
    assert_eq!(vertex_groups(&after), vertex_groups(&before), "identical vertices and flags after promotion and resave");
    let EntityKind::Leader(l) = &e.kind else { panic!("leader") };
    assert_eq!(l.vertices, legacy_leader().vertices);
    assert_eq!(l.frame.as_ref().unwrap().direction, Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn an_old_non_plus_z_payload_stays_legacy_opens_saves_unchanged_and_a_refused_edit_changes_nothing() {
    for (kind, normal) in
        [(EntityKind::Spline(legacy_spline()), Vec3::new(0.0, 0.0, -1.0)), (EntityKind::Leader(legacy_leader()), Vec3::new(0.0, 3.0, 4.0))]
    {
        let mut e = old_entity(kind, normal);
        let original = e.clone();
        let saved = dxf(&drawing_of(&e));
        assert_eq!(promote_legacy_spline_leader(&mut e), Promotion::KeptLegacy);
        assert_eq!(e, original, "nothing changed");
        assert_eq!(dxf(&drawing_of(&e)), saved, "saves as before");
        // A spatial edit has no legacy form: refused, and the entity is as it was.
        let (s, c) = 0.4f64.sin_cos();
        let rx = cadcraft_geom::Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(e.try_transform(&rx).is_err());
        assert_eq!(e, original, "a refused edit changes nothing");
        // A planar legacy edit still works as it does today.
        let shift = cadcraft_geom::Mat4 { m: [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(e.try_transform(&shift).is_ok());
        assert_ne!(e, original);
    }
}

#[test]
fn promotion_leaves_other_payloads_and_canonical_ones_alone() {
    let mut line = Entity::new(cadcraft_doc::Handle(1), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }));
    assert_eq!(promote_legacy_spline_leader(&mut line), Promotion::NotApplicable);
    let mut canonical = Entity::new(cadcraft_doc::Handle(2), EntityKind::Spline(spline()));
    canonical.geometry_revision = GeometryRevision::CanonicalV2;
    assert_eq!(promote_legacy_spline_leader(&mut canonical), Promotion::NotApplicable);
}

#[test]
fn an_unknown_geometry_revision_still_refuses_to_open() {
    let e = Entity::new(cadcraft_doc::Handle(3), EntityKind::Spline(legacy_spline()));
    let mut json = serde_json::to_value(&e).unwrap();
    json.as_object_mut().unwrap().insert("geometryRevision".into(), serde_json::json!("FutureV9"));
    assert!(serde_json::from_value::<Entity>(json).is_err());
}

// DIMENSION.

use cadcraft_doc::{DimKind, Dimension};

/// The WCS point of the OCS point (x, y, z) of the plane of `NORMAL` = (0, 3, 4), by the arbitrary axis rule written out by
/// hand: |Ny| >= 1/64, so the OCS X axis is Wz x N = (-1, 0, 0), the Y axis is N x X = (0, -0.8, 0.6), and Z is N / |N|.
fn plane(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(-x, -0.8 * y + 0.6 * z, 0.6 * y + 0.8 * z)
}

fn dimension(kind: DimKind) -> Dimension {
    Dimension {
        kind,
        defpt: plane(10.0, 3.0, 2.0),
        text_mid: Vec3::new(5.0, 3.0, 2.0),
        p13: plane(0.0, 0.0, 2.0),
        p14: plane(10.0, 0.0, 2.0),
        p15: plane(10.0, 5.0, 2.0),
        p16: Vec3::new(6.0, 4.0, 2.0),
        text: String::new(),
        style: "Standard".into(),
        measurement: 10.0,
        text_rotation: 0.0,
        user_text_pos: true,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn dimension_groups(d: &Dimension, normal: Vec3) -> (String, Vec<(i32, String)>) {
    let out = dxf(&drawing(EntityKind::Dimension(d.clone()), normal, 0.0));
    let g = groups(&out, "DIMENSION");
    (out, g)
}

fn dwg_dimension(d: &Dimension, normal: Vec3) -> acadrust::entities::Dimension {
    dwg_entities(&drawing(EntityKind::Dimension(d.clone()), normal, 0.0))
        .into_iter()
        .find_map(|e| if let EntityType::Dimension(x) = e { Some(*x) } else { None })
        .expect("a DIMENSION in the DWG")
}

#[test]
fn each_linear_dimension_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    type Mutation = fn(&mut Dimension, &mut Vec3);
    let cases: Vec<(&str, Mutation)> = vec![
        ("definition point", |d, _| d.defpt = plane(10.0, 3.5, 2.0)),
        ("first point z", |d, _| d.p13.z += 0.5),
        ("second point", |d, _| d.p14 = plane(11.0, 0.0, 2.0)),
        ("text midpoint x", |d, _| d.text_mid.x = 5.5),
        ("text midpoint elevation", |d, _| d.text_mid.z = 2.5),
        ("rotation", |d, _| d.kind = DimKind::Linear { rotation: 0.5 }),
        ("text rotation", |d, _| d.text_rotation = 0.25),
        ("normal x", |_, n| n.x = 0.25),
    ];
    let make = |mutate: Mutation| {
        let (mut d, mut n) = (dimension(DimKind::Linear { rotation: 0.0 }), NORMAL);
        mutate(&mut d, &mut n);
        (d, n)
    };
    let base = dimension_groups(&make(|_, _| {}).0, NORMAL).1;
    for (name, mutate) in cases {
        let (d, n) = make(mutate);
        let (out, g) = dimension_groups(&d, n);
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), d.defpt) && near(point(&g, 13).unwrap(), d.p13) && near(point(&g, 14).unwrap(), d.p14), "{name}");
        assert!(near(point(&g, 11).unwrap(), d.text_mid), "{name}: the OCS text midpoint keeps its Z: {g:?}");
        assert!(near(point(&g, 210).unwrap(), n), "{name}");
        let (_, kind) =
            cadcraft_io::canonical_curve_from_groups("DIMENSION", &record(&out, "DIMENSION")).unwrap_or_else(|| panic!("{name}: unreadable"));
        let EntityKind::Dimension(read) = kind else { panic!("not a dimension") };
        assert_eq!((read.defpt, read.text_mid, read.p13, read.p14), (d.defpt, d.text_mid, d.p13, d.p14), "{name}");
        let (DimKind::Linear { rotation: a }, DimKind::Linear { rotation: b }) = (read.kind, d.kind) else { panic!("kind") };
        assert!((a - b).abs() < 1e-12 && (read.text_rotation - d.text_rotation).abs() < 1e-12, "{name}");
        let stored = dwg_dimension(&d, n);
        let base_data = stored.base();
        assert!(near(v3(base_data.text_middle_point), d.text_mid) && near(v3(base_data.normal), n), "{name}: DWG text midpoint/normal");
        let acadrust::entities::Dimension::Linear(l) = &stored else { panic!("not linear in the DWG") };
        assert!(near(v3(l.first_point), d.p13) && near(v3(l.second_point), d.p14) && near(v3(l.definition_point), d.defpt), "{name}: DWG points");
    }
}

#[test]
fn the_measurement_is_taken_in_the_plane_whatever_its_tilt() {
    // p13 and p14 are 10 apart along the OCS X axis of the tilted plane, so the dimension measures 10 and the layout is
    // the planar one; a reading in the XY plane would not give that.
    let (_, g) = dimension_groups(&dimension(DimKind::Linear { rotation: 0.0 }), NORMAL);
    assert!((num(&g, 42).unwrap() - 10.0).abs() < 1e-9, "{g:?}");
    let mut vertical = dimension(DimKind::Linear { rotation: std::f64::consts::FRAC_PI_2 });
    vertical.p14 = plane(10.0, 4.0, 2.0);
    let (_, g) = dimension_groups(&vertical, NORMAL);
    assert!((num(&g, 42).unwrap() - 4.0).abs() < 1e-9, "a vertical dimension measures the OCS y distance: {g:?}");
}

#[test]
fn the_other_dimension_kinds_keep_their_points_in_the_dxf_the_dwg_and_the_canonical_reader() {
    for kind in [DimKind::Aligned, DimKind::Angular, DimKind::Angular3P, DimKind::Radius, DimKind::Diameter, DimKind::Ordinate { x_type: true }] {
        let d = dimension(kind);
        let (out, g) = dimension_groups(&d, NORMAL);
        let (_, read) =
            cadcraft_io::canonical_curve_from_groups("DIMENSION", &record(&out, "DIMENSION")).unwrap_or_else(|| panic!("{kind:?}: unreadable"));
        let EntityKind::Dimension(read) = read else { panic!("not a dimension") };
        assert_eq!(read.kind, kind);
        assert_eq!((read.defpt, read.text_mid), (d.defpt, d.text_mid), "{kind:?}");
        let uses = |points: &[i32]| points.iter().all(|c| point(&g, *c).is_some());
        match kind {
            DimKind::Aligned | DimKind::Ordinate { .. } => {
                assert!(uses(&[13, 14]) && read.p13 == d.p13 && read.p14 == d.p14, "{kind:?}");
            }
            DimKind::Angular => {
                assert!(uses(&[13, 14, 15, 16]), "{kind:?}");
                assert_eq!((read.p13, read.p14, read.p15, read.p16), (d.p13, d.p14, d.p15, d.p16), "the arc point stays an OCS point");
            }
            DimKind::Angular3P => assert!(uses(&[13, 14, 15]) && read.p15 == d.p15, "{kind:?}"),
            _ => assert!(uses(&[15]) && read.p15 == d.p15, "{kind:?}"),
        }
        let stored = dwg_dimension(&d, NORMAL);
        assert!(near(v3(stored.base().normal), NORMAL) && near(v3(stored.base().text_middle_point), d.text_mid), "{kind:?}: DWG");
    }
}

#[test]
fn the_canonical_dimension_reader_applies_the_oblique_angle_in_the_plane_and_refuses_a_zero_normal() {
    let mut d = dimension(DimKind::Aligned);
    // The extension lines lean 30 degrees off the dimension line direction, measured in the plane (the OCS angle of p14 - p13 is 0).
    d.marks.oblique = Some(std::f64::consts::FRAC_PI_6);
    let (out, g) = dimension_groups(&d, NORMAL);
    assert!((num(&g, 52).unwrap() - 30.0).abs() < 1e-9, "{g:?}");
    let (_, read) = cadcraft_io::canonical_curve_from_groups("DIMENSION", &record(&out, "DIMENSION")).unwrap();
    let EntityKind::Dimension(read) = read else { panic!("dimension") };
    assert!((read.marks.oblique.unwrap() - std::f64::consts::FRAC_PI_6).abs() < 1e-9);
    let mut zero = record(&out, "DIMENSION");
    zero.iter_mut().filter(|t| matches!(t.code, 210 | 220 | 230)).for_each(|t| *t = Tag::f(t.code, 0.0));
    assert!(cadcraft_io::canonical_curve_from_groups("DIMENSION", &zero).is_none());
}

#[test]
fn a_dimension_block_of_a_canonical_dimension_holds_wcs_lines_in_its_plane() {
    let mut d = dimension(DimKind::Linear { rotation: 0.0 });
    d.user_text_pos = false;
    let drawing = drawing(EntityKind::Dimension(d.clone()), NORMAL, 0.0);
    let text = dxf(&drawing);
    // The generated *D block: its LINE endpoints lie in the dimension plane (distance 2 above the OCS xy plane along the normal).
    let n = NORMAL.normalized();
    let blocks = tags_of(&text, "BLOCKS");
    let block_lines: Vec<Vec3> = blocks
        .iter()
        .filter(|(k, _)| k == "LINE")
        .flat_map(|(_, tags)| {
            let g: Vec<(i32, String)> = tags.iter().map(|t| (t.code, t.str())).collect();
            [point(&g, 10).unwrap(), point(&g, 11).unwrap()]
        })
        .collect();
    assert!(!block_lines.is_empty(), "the dimension block has lines");
    assert!(block_lines.iter().all(|p| (p.dot(n) - 2.0).abs() < 1e-9), "every block line lies in the plane: {block_lines:?}");
    // And the DXF reads back as a document (the block is valid DXF).
    assert!(cadcraft_io::read(text.as_bytes(), "x.dxf").is_ok());
}

#[test]
fn a_canonical_dimension_keeps_its_associations_with_a_tilted_source_line() {
    use cadcraft_doc::{AssocSnap, DimAssoc, Line};
    // dimension-assoc-tilted-001: a source LINE with nonzero Z; one attachment follows its end, the other its start.
    let mut d = Drawing::new_metric();
    let line_handle = d.new_handle();
    let line = Line { a: plane(0.0, 0.0, 2.0), b: plane(10.0, 0.0, 2.0) };
    d.model.push(Entity::new(line_handle, EntityKind::Line(line.clone())));
    let mut dm = dimension(DimKind::Linear { rotation: 0.0 });
    dm.assoc = vec![
        DimAssoc { point: "p13".into(), handle: line_handle, snap: AssocSnap::Start },
        DimAssoc { point: "p14".into(), handle: line_handle, snap: AssocSnap::End },
    ];
    let handle = d.new_handle();
    let mut e = Entity::new(handle, EntityKind::Dimension(dm.clone()));
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: NORMAL, ..Common::default() };
    d.model.push(e);
    let text = dxf(&d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects = sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap();
    let assoc = records(&objects.tags).into_iter().find(|(k, _)| k == "DIMASSOC").expect("the DIMASSOC object is written, not dropped").1;
    let g: Vec<(i32, String)> = assoc.iter().map(|t| (t.code, t.str())).collect();
    let references: Vec<&String> = g.iter().filter(|t| t.0 == 331).map(|t| &t.1).collect();
    assert_eq!(references.len(), 2, "one reference per attachment: {g:?}");
    assert!(references.iter().all(|h| u64::from_str_radix(h.trim(), 16).ok() == Some(line_handle.0)), "both name the source line");
    // The reference points keep their Z (the tilted plane), as the dimension holds them.
    let xyz = |code: i32| -> Vec<f64> { g.iter().filter(|t| t.0 == code).map(|t| t.1.trim().parse().unwrap()).collect() };
    let (xs, ys, zs) = (xyz(10), xyz(20), xyz(30));
    assert!(near(Vec3::new(xs[0], ys[0], zs[0]), dm.p13) && near(Vec3::new(xs[1], ys[1], zs[1]), dm.p14), "{g:?}");
    // Written back and read again (the legacy reader), the link to the line is still there.
    let back = cadcraft_io::read(text.as_bytes(), "x.dxf").unwrap();
    let linked =
        back.model.iter().any(|e| matches!(&e.kind, EntityKind::Dimension(x) if x.assoc.len() == 2 && x.assoc.iter().all(|a| a.handle.0 != 0)));
    assert!(linked, "the associations survive the save");
}

// LINE, POINT, ELLIPSE, 3DFACE, 3D POLYLINE.

#[test]
fn each_line_and_point_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    type LineMutation = fn(&mut cadcraft_doc::Line, &mut Vec3, &mut f64);
    let line_cases: Vec<(&str, LineMutation)> = vec![
        ("start z", |l, _, _| l.a.z = 1.5),
        ("end y", |l, _, _| l.b.y = 1.5),
        ("normal y", |_, n, _| n.y = 3.5),
        ("thickness", |_, _, t| *t = -0.75),
    ];
    let make_line = |mutate: LineMutation| {
        let (mut l, mut n, mut t) = (cadcraft_doc::Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(4.0, 1.0, 2.0) }, NORMAL, 0.5);
        mutate(&mut l, &mut n, &mut t);
        (drawing(EntityKind::Line(l.clone()), n, t), l, n, t)
    };
    let base = groups(&dxf(&make_line(|_, _, _| {}).0), "LINE");
    for (name, mutate) in line_cases {
        let (d, l, n, t) = make_line(mutate);
        let out = dxf(&d);
        let g = groups(&out, "LINE");
        assert_ne!(g, base, "{name}");
        assert!(
            near(point(&g, 10).unwrap(), l.a) && near(point(&g, 11).unwrap(), l.b) && near(point(&g, 210).unwrap(), n) && num(&g, 39) == Some(t),
            "{name}"
        );
        let (c, kind) = cadcraft_io::canonical_curve_from_groups("LINE", &record(&out, "LINE")).unwrap();
        assert_eq!((c.extrusion, c.thickness, kind), (n, t, EntityKind::Line(l.clone())), "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Line(x) = e { Some(x) } else { None }).expect("a LINE");
        assert!(near(v3(s.start), l.a) && near(v3(s.end), l.b) && near(v3(s.normal), n) && s.thickness == t, "{name}: DWG");
    }

    type PointMutation = fn(&mut cadcraft_doc::Point, &mut Vec3, &mut f64);
    let point_cases: Vec<(&str, PointMutation)> = vec![
        ("position z", |p, _, _| p.p.z = 1.5),
        ("angle", |p, _, _| p.angle = 0.75),
        ("normal x", |_, n, _| n.x = 0.25),
        ("thickness", |_, _, t| *t = 1.25),
    ];
    let make_point = |mutate: PointMutation| {
        let (mut p, mut n, mut t) = (cadcraft_doc::Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.5 }, NORMAL, 0.5);
        mutate(&mut p, &mut n, &mut t);
        (drawing(EntityKind::Point(p.clone()), n, t), p, n, t)
    };
    let base = groups(&dxf(&make_point(|_, _, _| {}).0), "POINT");
    for (name, mutate) in point_cases {
        let (d, p, n, t) = make_point(mutate);
        let out = dxf(&d);
        let g = groups(&out, "POINT");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), p.p) && near(point(&g, 210).unwrap(), n) && num(&g, 39) == Some(t), "{name}");
        assert!((num(&g, 50).unwrap() - p.angle.to_degrees()).abs() < 1e-9, "{name}");
        let (_, kind) = cadcraft_io::canonical_curve_from_groups("POINT", &record(&out, "POINT")).unwrap();
        let EntityKind::Point(read) = kind else { panic!("point") };
        assert_eq!(read.p, p.p, "{name}");
        assert!((read.angle - p.angle).abs() < 1e-12, "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|e| if let EntityType::Point(x) = e { Some(x) } else { None }).expect("a POINT");
        assert!(near(v3(s.location), p.p) && near(v3(s.normal), n) && s.thickness == t, "{name}: DWG");
        assert!((s.x_axis_angle - p.angle).abs() < 1e-9, "{name}: DWG angle {} (radians)", s.x_axis_angle);
    }
}

#[test]
fn each_ellipse_field_alone_reaches_the_dxf_the_dwg_and_the_canonical_reader() {
    type Mutation = fn(&mut cadcraft_doc::Ellipse, &mut Vec3);
    // The major axis must lie in the plane of NORMAL = (0, 3, 4): (1, 0, 0) and (0, 4, -3) do.
    let cases: Vec<(&str, Mutation)> = vec![
        ("center z", |e, _| e.center.z = 3.5),
        ("major length", |e, _| e.major = Vec3::new(5.0, 0.0, 0.0)),
        ("major direction", |e, _| e.major = Vec3::new(0.0, 4.0, -3.0)),
        ("ratio", |e, _| e.ratio = 0.25),
        ("start", |e, _| e.start = 0.5),
        ("end", |e, _| e.end = 2.5),
        ("normal magnitude", |e, n| {
            *n = Vec3::new(0.0, 6.0, 8.0);
            e.normal = *n;
        }),
    ];
    let make = |mutate: Mutation| {
        let (mut e, mut n) = (
            cadcraft_doc::Ellipse {
                center: Vec3::new(1.0, 2.0, 3.0),
                major: Vec3::new(4.0, 0.0, 0.0),
                normal: NORMAL,
                ratio: 0.5,
                start: 0.3,
                end: 2.0,
            },
            NORMAL,
        );
        mutate(&mut e, &mut n);
        (drawing(EntityKind::Ellipse(e.clone()), n, 0.0), e, n)
    };
    let base = groups(&dxf(&make(|_, _| {}).0), "ELLIPSE");
    for (name, mutate) in cases {
        let (d, e, n) = make(mutate);
        let out = dxf(&d);
        let g = groups(&out, "ELLIPSE");
        assert_ne!(g, base, "{name}");
        assert!(near(point(&g, 10).unwrap(), e.center) && near(point(&g, 11).unwrap(), e.major) && near(point(&g, 210).unwrap(), n), "{name}");
        assert_eq!((num(&g, 40), num(&g, 41), num(&g, 42)), (Some(e.ratio), Some(e.start), Some(e.end)), "{name}");
        let (c, kind) = cadcraft_io::canonical_curve_from_groups("ELLIPSE", &record(&out, "ELLIPSE")).unwrap();
        assert_eq!((c.extrusion, kind), (n, EntityKind::Ellipse(e.clone())), "{name}");
        let s = dwg_entities(&d).into_iter().find_map(|x| if let EntityType::Ellipse(x) = x { Some(x) } else { None }).expect("an ELLIPSE");
        assert!(near(v3(s.center), e.center) && near(v3(s.major_axis), e.major) && near(v3(s.normal), n), "{name}: DWG");
        assert!(
            (s.minor_axis_ratio - e.ratio).abs() < 1e-12 && (s.start_parameter - e.start).abs() < 1e-9 && (s.end_parameter - e.end).abs() < 1e-9,
            "{name}: DWG"
        );
    }
}

#[test]
fn a_3dface_a_3d_polyline_and_a_mesh_keep_their_wcs_positions() {
    let face = cadcraft_doc::Face3d {
        corners: [Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 1.5), Vec3::new(2.0, 2.0, 2.0), Vec3::new(0.0, 2.0, 2.5)],
        hidden_edges: 5,
    };
    let out = dxf(&drawing(EntityKind::Face3d(face.clone()), NORMAL, 0.0));
    let (_, kind) = cadcraft_io::canonical_curve_from_groups("3DFACE", &record(&out, "3DFACE")).unwrap();
    assert_eq!(kind, EntityKind::Face3d(face.clone()));
    let stored = dwg_entities(&drawing(EntityKind::Face3d(face.clone()), NORMAL, 0.0))
        .into_iter()
        .find_map(|e| if let EntityType::Face3D(x) = e { Some(x) } else { None })
        .expect("a 3DFACE");
    assert!(near(v3(stored.third_corner), face.corners[2]) && near(v3(stored.fourth_corner), face.corners[3]));

    let poly = cadcraft_doc::Polyline3d {
        records: Default::default(),
        points: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.0, 3.0, 4.0), Vec3::new(5.0, 1.0, -2.0)],
        closed: true,
        fit: None,
    };
    let out = dxf(&drawing(EntityKind::Polyline3d(poly.clone()), NORMAL, 0.0));
    let entities = tags_of(&out, "ENTITIES");
    let at = entities.iter().position(|(k, _)| k == "POLYLINE").expect("a POLYLINE");
    let children: Vec<(String, Vec<Tag>)> = entities[at + 1..].iter().take_while(|(k, _)| k == "VERTEX" || k == "SEQEND").cloned().collect();
    let (c, kind) = cadcraft_io::canonical_polyline_from_groups(&entities[at].1, &children).unwrap();
    assert_eq!(c.extrusion, NORMAL, "the 3D polyline keeps its extrusion");
    let EntityKind::Polyline3d(read) = kind else { panic!("not a 3D polyline") };
    assert_eq!((read.points, read.closed), (poly.points, poly.closed));
}

#[test]
fn a_ray_a_construction_line_and_an_image_keep_their_wcs_frames() {
    let ray = cadcraft_doc::RayLine { base: Vec3::new(1.0, 2.0, 3.0), dir: Vec3::new(0.0, 0.6, 0.8) };
    for (kind, canonical) in [("RAY", EntityKind::Ray(ray.clone())), ("XLINE", EntityKind::XLine(ray.clone()))] {
        let d = drawing(canonical.clone(), NORMAL, 0.0);
        let out = dxf(&d);
        let g = groups(&out, kind);
        assert!(near(point(&g, 10).unwrap(), ray.base) && near(point(&g, 11).unwrap(), ray.dir), "{kind}");
        let (_, read) = cadcraft_io::canonical_curve_from_groups(kind, &record(&out, kind)).unwrap();
        assert_eq!(read, canonical, "{kind}");
        let stored = dwg_entities(&d);
        let (base, dir) = stored
            .iter()
            .find_map(|e| match e {
                EntityType::Ray(x) => Some((x.base_point, x.direction)),
                EntityType::XLine(x) => Some((x.base_point, x.direction)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("a {kind} in the DWG"));
        assert!(near(v3(base), ray.base) && near(v3(dir), ray.dir), "{kind}: DWG");
    }
    // An image: the insertion point and the pixel vectors keep their Z, the clip stays in pixels.
    let image = cadcraft_doc::Image {
        insert: Vec3::new(1.0, 2.0, 3.0),
        u: Vec3::new(0.5, 0.0, 0.25),
        v: Vec3::new(0.0, 0.5, 0.25),
        size: Vec2::new(100.0, 50.0),
        clip: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)],
        clipping: true,
        ..cadcraft_doc::Image::default()
    };
    let d = drawing(EntityKind::Image(image.clone()), NORMAL, 0.0);
    let out = dxf(&d);
    let (_, read) = cadcraft_io::canonical_curve_from_groups("IMAGE", &record(&out, "IMAGE")).unwrap();
    let EntityKind::Image(read) = read else { panic!("image") };
    assert_eq!((read.insert, read.u, read.v, read.size, read.clip), (image.insert, image.u, image.v, image.size, image.clip));
}

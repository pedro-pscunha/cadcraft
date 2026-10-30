//! Every CIRCLE, ARC and 2D polyline field of a canonical payload is part of the replay key, so changing it alone
//! retires the source snapshot.

use cadcraft_doc::{Arc, Circle, Common, EntityKind, LwPolyline, Solid, geometry_key};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};

fn distinct(keys: &[String]) {
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

fn common() -> Common {
    Common { extrusion: Vec3::new(0.0, 3.0, 4.0), thickness: 0.5, ..Common::default() }
}

#[test]
fn each_circle_and_arc_field_alone_changes_the_key() {
    let circle = Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 };
    let arc = Arc { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0, start: 0.25, end: 2.0 };
    type Mutation = fn(&mut Common);
    let planes: Vec<(&str, Mutation)> = vec![
        ("normal x", |c| c.extrusion.x += 0.5),
        ("normal y", |c| c.extrusion.y += 0.5),
        ("normal z", |c| c.extrusion.z += 0.5),
        ("thickness", |c| c.thickness = -0.5),
    ];
    let key = |k: EntityKind, c: &Common| geometry_key(&k, c).unwrap();
    let mut keys = vec![key(EntityKind::Circle(circle.clone()), &common()), key(EntityKind::Arc(arc.clone()), &common())];
    for (_, mutate) in &planes {
        let mut c = common();
        mutate(&mut c);
        keys.push(key(EntityKind::Circle(circle.clone()), &c));
        keys.push(key(EntityKind::Arc(arc.clone()), &c));
    }
    keys.push(key(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 3.5), ..circle.clone() }), &common()));
    keys.push(key(EntityKind::Circle(Circle { radius: 2.5, ..circle }), &common()));
    keys.push(key(EntityKind::Arc(Arc { center: Vec3::new(1.5, 2.0, 3.0), ..arc.clone() }), &common()));
    keys.push(key(EntityKind::Arc(Arc { start: 0.5, ..arc.clone() }), &common()));
    keys.push(key(EntityKind::Arc(Arc { end: 2.5, ..arc }), &common()));
    distinct(&keys);
}

fn polyline() -> LwPolyline {
    LwPolyline {
        records: Default::default(),
        vertices: vec![PolyVertex { p: Vec2::new(0.0, 0.0), bulge: 0.5, start_width: 0.25, end_width: 0.5 }, PolyVertex::new(Vec2::new(3.0, 1.0))],
        closed: false,
        const_width: 0.0,
        elevation: 2.0,
        plinegen: false,
        heavy: false,
        fit: None,
    }
}

#[test]
fn each_polyline_field_alone_changes_the_key() {
    let key = |p: &LwPolyline, c: &Common| geometry_key(&EntityKind::LwPolyline(p.clone()), c).unwrap();
    let base = key(&polyline(), &common());
    type Mutation = fn(&mut LwPolyline, &mut Common);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("vertex x", |p, _| p.vertices[1].p.x += 0.5),
        ("vertex y", |p, _| p.vertices[1].p.y += 0.5),
        ("bulge", |p, _| p.vertices[0].bulge = 0.75),
        ("start width", |p, _| p.vertices[0].start_width = 0.75),
        ("end width", |p, _| p.vertices[0].end_width = 0.75),
        ("constant width", |p, _| p.const_width = 0.5),
        ("elevation", |p, _| p.elevation = 2.5),
        ("closed", |p, _| p.closed = true),
        ("plinegen", |p, _| p.plinegen = true),
        ("normal x", |_, c| c.extrusion.x += 0.5),
        ("normal z", |_, c| c.extrusion.z += 0.5),
        ("thickness", |_, c| c.thickness = 1.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut p, mut c) = (polyline(), common());
        mutate(&mut p, &mut c);
        let k = key(&p, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    distinct(&keys);
}

#[test]
fn each_solid_field_alone_changes_the_key() {
    let solid = Solid { corners: [Vec3::new(0.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 3.0, 2.0), Vec3::new(2.0, 3.0, 2.0)] };
    let key = |s: &Solid, c: &Common| geometry_key(&EntityKind::Solid(s.clone()), c).unwrap();
    let mut keys = vec![key(&solid, &common())];
    for i in 0..4 {
        let mut s = solid.clone();
        s.corners[i].z += 0.5;
        keys.push(key(&s, &common()));
        let mut s = solid.clone();
        s.corners[i].x += 0.5;
        keys.push(key(&s, &common()));
    }
    let mut c = common();
    c.extrusion.y += 0.5;
    keys.push(key(&solid, &c));
    let mut c = common();
    c.thickness = -0.5;
    keys.push(key(&solid, &c));
    distinct(&keys);
}

fn spline() -> cadcraft_geom::Spline {
    use cadcraft_geom::{FitOptions, Spline, SplineSpace};
    Spline {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, 0.0)],
        weights: vec![1.0, 0.5, 1.0],
        fit: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)],
        closed: false,
        fit_opts: FitOptions {
            start_tangent: Some(Vec2::new(1.0, 1.0)),
            end_tangent: Some(Vec2::new(1.0, -1.0)),
            tolerance: 0.25,
            ..FitOptions::default()
        },
        space: Some(SplineSpace { control: vec![0.0, 4.0, 1.0], fit: vec![0.0, 1.5], start_tangent: 0.5, end_tangent: -0.5 }),
    }
}

#[test]
fn each_spatial_spline_field_alone_changes_the_key() {
    let key = |s: &cadcraft_geom::Spline, c: &Common| geometry_key(&EntityKind::Spline(s.clone()), c).unwrap();
    let base = key(&spline(), &common());
    type Mutation = fn(&mut cadcraft_geom::Spline, &mut Common);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("control x", |s, _| s.control[1].x += 0.5),
        ("control z", |s, _| s.space.as_mut().unwrap().control[1] += 0.5),
        ("fit point z", |s, _| s.space.as_mut().unwrap().fit[1] += 0.5),
        ("start tangent z", |s, _| s.space.as_mut().unwrap().start_tangent += 0.5),
        ("end tangent z", |s, _| s.space.as_mut().unwrap().end_tangent += 0.5),
        ("tangent x", |s, _| s.fit_opts.end_tangent = Some(Vec2::new(2.0, -1.0))),
        ("tolerance", |s, _| s.fit_opts.tolerance = 0.5),
        ("weight", |s, _| s.weights[1] = 0.75),
        ("knot", |s, _| s.knots[3] = 0.9),
        ("closed", |s, _| s.closed = true),
        ("normal y", |_, c| c.extrusion.y += 0.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut s, mut c) = (spline(), common());
        mutate(&mut s, &mut c);
        let k = key(&s, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    distinct(&keys);
    assert_ne!(base, key(&cadcraft_geom::Spline { space: None, ..spline() }, &common()), "a planar spline is not the spatial one");
}

#[test]
fn each_canonical_leader_field_alone_changes_the_key() {
    use cadcraft_doc::{Leader, LeaderFrame};
    let leader = || Leader {
        vertices: vec![Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 1.0, 2.0)],
        frame: Some(LeaderFrame {
            direction: Vec3::new(1.0, 0.0, 0.0),
            block_offset: Vec3::new(0.0, 0.5, 0.0),
            annotation_offset: Vec3::new(1.0, 0.25, 0.0),
            text_height: 0.2,
            text_width: 3.0,
        }),
        ..Leader::default()
    };
    let key = |l: &Leader, c: &Common| geometry_key(&EntityKind::Leader(l.clone()), c).unwrap();
    let base = key(&leader(), &common());
    type Mutation = fn(&mut Leader, &mut Common);
    fn frame(l: &mut Leader) -> &mut LeaderFrame {
        l.frame.as_mut().unwrap()
    }
    let mutations: Vec<(&str, Mutation)> = vec![
        ("vertex z", |l, _| l.vertices[1].z += 0.5),
        ("direction z", |l, _| frame(l).direction.z += 0.5),
        ("block offset y", |l, _| frame(l).block_offset.y += 0.5),
        ("annotation offset z", |l, _| frame(l).annotation_offset.z += 0.5),
        ("text height", |l, _| frame(l).text_height += 0.5),
        ("text width", |l, _| frame(l).text_width += 0.5),
        ("normal x", |_, c| c.extrusion.x += 0.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut l, mut c) = (leader(), common());
        mutate(&mut l, &mut c);
        let k = key(&l, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    distinct(&keys);
    let mut renamed = leader();
    renamed.style = "Other".into();
    renamed.arrow = false;
    assert_eq!(key(&renamed, &common()), base, "the style and the arrow switch are properties");
}

#[test]
fn each_canonical_dimension_field_alone_changes_the_key() {
    use cadcraft_doc::{DimKind, Dimension};
    let dimension = || Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(10.0, 3.0, 2.0),
        text_mid: Vec3::new(5.0, 3.0, 2.0),
        p13: Vec3::new(0.0, 0.0, 2.0),
        p14: Vec3::new(10.0, 0.0, 2.0),
        p15: Vec3::new(10.0, 5.0, 2.0),
        p16: Vec3::new(6.0, 4.0, 2.0),
        text: String::new(),
        style: "Standard".into(),
        measurement: 10.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    };
    let key = |d: &Dimension, c: &Common| geometry_key(&EntityKind::Dimension(d.clone()), c).unwrap();
    let base = key(&dimension(), &common());
    type Mutation = fn(&mut Dimension, &mut Common);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("definition point z", |d, _| d.defpt.z += 0.5),
        ("text midpoint z", |d, _| d.text_mid.z += 0.5),
        ("first point y", |d, _| d.p13.y += 0.5),
        ("second point z", |d, _| d.p14.z += 0.5),
        ("third point x", |d, _| d.p15.x += 0.5),
        ("arc point z", |d, _| d.p16.z += 0.5),
        ("rotation", |d, _| d.kind = DimKind::Linear { rotation: 0.5 }),
        ("kind", |d, _| d.kind = DimKind::Aligned),
        ("text rotation", |d, _| d.text_rotation = 0.5),
        ("text placement", |d, _| d.user_text_pos = true),
        ("oblique", |d, _| d.marks.oblique = Some(0.3)),
        ("normal x", |_, c| c.extrusion.x += 0.5),
        ("thickness", |_, c| c.thickness = 1.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut d, mut c) = (dimension(), common());
        mutate(&mut d, &mut c);
        let k = key(&d, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    distinct(&keys);
    let mut regenerated = dimension();
    regenerated.measurement = 11.0;
    regenerated.text = "x".into();
    regenerated.style = "Other".into();
    regenerated.block = Some("*D3".into());
    assert_eq!(key(&regenerated, &common()), base, "the measurement, text, style and block are regenerated, not replayed");
}

#[test]
fn each_ellipse_line_and_point_field_alone_changes_the_key() {
    use cadcraft_doc::{Ellipse, Line, Point};
    let key = |k: EntityKind, c: &Common| geometry_key(&k, c).unwrap();
    let ellipse = || Ellipse { center: Vec3::new(1.0, 2.0, 3.0), major: Vec3::new(4.0, 0.0, 0.0), normal: Vec3::Z, ratio: 0.5, start: 0.3, end: 2.0 };
    let line = || Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(4.0, 1.0, 2.0) };
    let point = || Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.5 };
    let mut keys =
        vec![key(EntityKind::Ellipse(ellipse()), &common()), key(EntityKind::Line(line()), &common()), key(EntityKind::Point(point()), &common())];
    let e = |f: fn(&mut Ellipse)| {
        let mut x = ellipse();
        f(&mut x);
        key(EntityKind::Ellipse(x), &common())
    };
    keys.extend([
        e(|x| x.center.z += 0.5),
        e(|x| x.major.y = 0.5),
        e(|x| x.ratio = 0.25),
        e(|x| x.start = 0.4),
        e(|x| x.end = 2.5),
        key(EntityKind::Ellipse(ellipse()), &Common { extrusion: Vec3::new(0.0, 3.0, 5.0), ..common() }),
    ]);
    keys.extend([
        key(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.5), ..line() }), &common()),
        key(EntityKind::Line(Line { b: Vec3::new(4.0, 1.5, 2.0), ..line() }), &common()),
        key(EntityKind::Line(line()), &Common { thickness: -0.5, ..common() }),
        key(EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 3.5), ..point() }), &common()),
        key(EntityKind::Point(Point { angle: 0.75, ..point() }), &common()),
        key(EntityKind::Point(point()), &Common { extrusion: Vec3::new(0.25, 3.0, 4.0), ..common() }),
    ]);
    distinct(&keys);
}

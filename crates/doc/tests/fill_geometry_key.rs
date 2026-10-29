//! Every canonical wipeout field is part of the replay key, so changing it alone retires the source snapshot.

use cadcraft_doc::{
    Common, EntityKind, Hatch, HatchEdge, HatchLoop, HatchSpline, Helix, HelixConstrain, HelixFrame, MLine, MLineJust, MLineStyle, MLineVertex,
    MLineVertexFrame, Wipeout, WipeoutFrame, geometry_key,
};
use cadcraft_geom::{Vec2, Vec3};

fn wipeout() -> Wipeout {
    Wipeout {
        boundary: Vec::new(),
        frame: Some(WipeoutFrame {
            origin: Vec3::new(10.0, 1.0, 2.0),
            u: Vec3::new(4.0, 0.0, 0.5),
            v: Vec3::new(0.0, 2.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            clip_type: 2,
            clip: vec![Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(0.5, 0.5), Vec2::new(-0.5, -0.5)],
        }),
    }
}

#[test]
fn each_wipeout_frame_field_alone_changes_the_key() {
    let key = |w: &Wipeout| geometry_key(&EntityKind::Wipeout(w.clone()), &Common::default()).unwrap();
    let base = key(&wipeout());
    type Mutation = fn(&mut WipeoutFrame);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("origin z", |f| f.origin.z += 0.5),
        ("u x", |f| f.u.x += 0.5),
        ("u z", |f| f.u.z += 0.5),
        ("v y", |f| f.v.y += 0.5),
        ("v z", |f| f.v.z += 0.5),
        ("size", |f| f.size.x = 2.0),
        ("clip type", |f| f.clip_type = 1),
        ("clip vertex", |f| f.clip[1].y = 0.25),
        ("clip count", |f| f.clip.truncate(3)),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let mut w = wipeout();
        mutate(w.frame.as_mut().unwrap());
        let k = key(&w);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
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

#[test]
fn each_helix_frame_field_alone_changes_the_key() {
    let key = |h: &Helix| geometry_key(&EntityKind::Helix(h.clone()), &Common::default()).unwrap();
    let base = key(&helix());
    type Mutation = fn(&mut Helix);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("center z", |h| h.center.z += 0.5),
        ("axis x", |h| h.frame.as_mut().unwrap().axis.x += 0.5),
        ("axis y", |h| h.frame.as_mut().unwrap().axis.y += 0.5),
        ("axis z", |h| h.frame.as_mut().unwrap().axis.z += 0.5),
        ("start x", |h| h.frame.as_mut().unwrap().start.x += 0.5),
        ("start z", |h| h.frame.as_mut().unwrap().start.z += 0.5),
        ("radius", |h| h.base_radius += 0.5),
        ("top radius", |h| h.top_radius += 0.5),
        ("height", |h| h.height += 0.5),
        ("turns", |h| h.turns += 1.0),
        ("handedness", |h| h.ccw = false),
        ("constraint", |h| h.constrain = HelixConstrain::Turns),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let mut h = helix();
        mutate(&mut h);
        let k = key(&h);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

fn mline() -> MLine {
    let mut m = MLine::new(&MLineStyle::default(), &[], false, MLineJust::Zero, 2.0);
    for p in [Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 0.0, 5.0)] {
        let mut v = MLineVertex::new(p);
        v.frame =
            Some(MLineVertexFrame { dir: Vec3::new(0.6, 0.0, 0.8), miter: Vec3::new(0.0, 1.0, 0.0), params: vec![vec![1.0, 5.0], vec![-1.0, 5.0]] });
        m.vertices.push(v);
    }
    m
}

#[test]
fn each_mline_frame_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 2.0, 0.0), ..Common::default() };
    let key = |m: &MLine, c: &Common| geometry_key(&EntityKind::MLine(m.clone()), c).unwrap();
    let base = key(&mline(), &common);
    type Mutation = fn(&mut MLine, &mut Common);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("vertex z", |m, _| m.vertices[1].p.z += 0.5),
        ("direction y", |m, _| m.vertices[0].frame.as_mut().unwrap().dir.y += 0.5),
        ("direction z", |m, _| m.vertices[1].frame.as_mut().unwrap().dir.z += 0.5),
        ("miter x", |m, _| m.vertices[1].frame.as_mut().unwrap().miter.x += 0.5),
        ("miter z", |m, _| m.vertices[0].frame.as_mut().unwrap().miter.z += 0.5),
        ("parameter", |m, _| m.vertices[0].frame.as_mut().unwrap().params[1][1] += 0.5),
        ("break", |m, _| m.vertices[1].frame.as_mut().unwrap().params[0].extend([1.0, 2.0])),
        ("scale", |m, _| m.scale += 0.5),
        ("closed", |m, _| m.closed = true),
        ("start caps", |m, _| m.no_start_caps = true),
        ("normal x", |_, c| c.extrusion.x += 0.5),
        ("normal y", |_, c| c.extrusion.y += 0.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut m, mut c) = (mline(), common.clone());
        mutate(&mut m, &mut c);
        let k = key(&m, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
    let mut renamed = mline();
    renamed.style = "Other".into();
    assert_eq!(key(&renamed, &common), base, "the style name is a property");
}

fn hatch() -> Hatch {
    let spline = HatchSpline {
        degree: 3,
        rational: true,
        periodic: false,
        knots: vec![0.0, 0.0, 1.0, 1.0],
        control: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0)],
        weights: vec![1.0, 0.5],
        fit: vec![Vec2::new(0.0, 0.0)],
        start_tangent: Vec2::new(1.0, 1.0),
        end_tangent: Vec2::new(1.0, -1.0),
    };
    let edges = vec![
        HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
        HatchEdge::Arc { center: Vec2::new(1.0, 0.5), radius: 2.0, start: 0.25, end: 1.5, ccw: true },
        HatchEdge::Ellipse { center: Vec2::new(5.0, 5.0), major: Vec2::new(2.0, 0.0), ratio: 0.5, start: 0.0, end: 3.0, ccw: true },
        HatchEdge::Spline(spline),
        HatchEdge::Polyline { vertices: vec![cadcraft_geom::PolyVertex::new(Vec2::new(0.0, 0.0))], closed: true },
    ];
    Hatch {
        pattern: "SOLID".into(),
        solid: true,
        loops: vec![HatchLoop { flags: 1, edges, outer: true, ..HatchLoop::default() }],
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

#[test]
fn each_canonical_hatch_edge_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 0.0, 2.0), ..Common::default() };
    let key = |h: &Hatch, c: &Common| geometry_key(&EntityKind::Hatch(h.clone()), c).unwrap();
    let base = key(&hatch(), &common);
    type Mutation = fn(&mut Hatch, &mut Common);
    fn spline(h: &mut Hatch) -> &mut HatchSpline {
        match &mut h.loops[0].edges[3] {
            HatchEdge::Spline(s) => s,
            _ => unreachable!(),
        }
    }
    let mutations: Vec<(&str, Mutation)> = vec![
        ("flags", |h, _| h.loops[0].flags = 17),
        ("line end", |h, _| h.loops[0].edges[0] = HatchEdge::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.5) }),
        ("arc direction", |h, _| {
            if let HatchEdge::Arc { ccw, .. } = &mut h.loops[0].edges[1] {
                *ccw = false;
            }
        }),
        ("arc end", |h, _| {
            if let HatchEdge::Arc { end, .. } = &mut h.loops[0].edges[1] {
                *end += 0.5;
            }
        }),
        ("ellipse ratio", |h, _| {
            if let HatchEdge::Ellipse { ratio, .. } = &mut h.loops[0].edges[2] {
                *ratio = 0.25;
            }
        }),
        ("ellipse major", |h, _| {
            if let HatchEdge::Ellipse { major, .. } = &mut h.loops[0].edges[2] {
                major.y = 0.5;
            }
        }),
        ("spline weight", |h, _| spline(h).weights[1] = 0.75),
        ("spline knot", |h, _| spline(h).knots[2] = 0.9),
        ("spline fit point", |h, _| spline(h).fit[0].x = 0.5),
        ("spline end tangent", |h, _| spline(h).end_tangent.y = -2.0),
        ("spline periodic", |h, _| spline(h).periodic = true),
        ("polyline closed", |h, _| {
            if let HatchEdge::Polyline { closed, .. } = &mut h.loops[0].edges[4] {
                *closed = false;
            }
        }),
        ("elevation", |h, _| h.elevation = 2.5),
        ("normal x", |_, c| c.extrusion.x += 0.5),
        ("normal z", |_, c| c.extrusion.z += 0.5),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let (mut h, mut c) = (hatch(), common.clone());
        mutate(&mut h, &mut c);
        let k = key(&h, &c);
        assert_ne!(k, base, "{name}");
        keys.push(k);
    }
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

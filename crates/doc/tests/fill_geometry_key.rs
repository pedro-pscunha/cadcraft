//! Every canonical wipeout field is part of the replay key, so changing it alone retires the source snapshot.

use cadcraft_doc::{
    Common, EntityKind, Helix, HelixConstrain, HelixFrame, MLine, MLineJust, MLineStyle, MLineVertex, MLineVertexFrame, Wipeout, WipeoutFrame,
    geometry_key,
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

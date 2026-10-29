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

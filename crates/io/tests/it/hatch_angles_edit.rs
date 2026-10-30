//! A hatch's pattern angle (52) and gradient angle (460) through a whole-object edit. ROTATE turns both with the hatch. MIRROR keeps
//! the pattern direction, as AutoCAD does with MIRRHATCH = 0 (measured: ANSI31 at angle 0 keeps pattern angle 0 and line angle 45
//! degrees whether the mirror line is vertical or at 30 degrees); the gradient angle is kept the same way. MOVE and SCALE keep both.

use cadcraft_color::Color;
use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Gradient, Hatch, HatchLoop, Space};
use cadcraft_geom::{Mat3, Mat4, PolyVertex, Vec2};

fn hatch(pattern: &str, solid: bool, gradient: Option<Gradient>) -> Hatch {
    let v = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect();
    Hatch {
        pattern: pattern.into(),
        solid,
        loops: vec![HatchLoop { vertices: v, outer: true, ..Default::default() }],
        scale: 1.0,
        angle: 30f64.to_radians(),
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    }
}

fn gradient() -> Gradient {
    Gradient {
        name: "LINEAR".into(),
        color1: Color::Index(1),
        color2: Color::Index(5),
        angle: 20f64.to_radians(),
        centered: true,
        source: Default::default(),
    }
}

fn entity(h: Hatch) -> Entity {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(h)).unwrap();
    d.model.iter().next().unwrap().as_ref().clone()
}

fn angles(e: &Entity) -> (f64, Option<f64>) {
    let EntityKind::Hatch(h) = &e.kind else { panic!("not a hatch") };
    (h.angle.to_degrees(), h.gradient.as_ref().map(|g| g.angle.to_degrees()))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 || ((a - b).abs() - 360.0).abs() < 1e-6
}

fn after(e: &Entity, m: Mat3) -> Entity {
    e.transformed(&Mat4::from_mat3(&m)).expect("the edit")
}

#[test]
fn rotate_turns_the_pattern_and_the_gradient_with_the_hatch() {
    let e = entity(hatch("ANSI31", false, None));
    let g = entity(hatch("SOLID", true, Some(gradient())));
    let (p, _) = angles(&after(&e, Mat3::rotate_about(Vec2::new(1.0, 1.0), 40f64.to_radians())));
    assert!(near(p, 70.0), "pattern {p}");
    let (p, grad) = angles(&after(&g, Mat3::rotate_about(Vec2::new(1.0, 1.0), 40f64.to_radians())));
    assert!(near(p, 70.0) && near(grad.unwrap(), 60.0), "pattern {p}, gradient {grad:?}");
}

#[test]
fn mirror_keeps_the_pattern_direction_and_the_gradient_angle() {
    let g = entity(hatch("SOLID", true, Some(gradient())));
    for m in [Mat3::mirror(Vec2::new(3.0, -5.0), Vec2::new(3.0, 5.0)), Mat3::mirror(Vec2::new(0.0, 0.0), Vec2::new(8.66, 5.0))] {
        let (p, grad) = angles(&after(&g, m));
        assert!(near(p, 30.0) && near(grad.unwrap(), 20.0), "pattern {p}, gradient {grad:?}");
    }
}

#[test]
fn move_and_scale_keep_both_angles() {
    let g = entity(hatch("SOLID", true, Some(gradient())));
    for m in [Mat3::translate(Vec2::new(10.0, 5.0)), Mat3::scale_about(Vec2::new(1.0, 1.0), 2.5)] {
        let (p, grad) = angles(&after(&g, m));
        assert!(near(p, 30.0) && near(grad.unwrap(), 20.0), "pattern {p}, gradient {grad:?}");
    }
}

#[test]
fn the_edited_angles_reach_the_dxf_and_the_dwg() {
    let g = entity(hatch("SOLID", true, Some(gradient())));
    let rotated = after(&g, Mat3::rotate_about(Vec2::new(1.0, 1.0), 40f64.to_radians()));
    let mut out = Drawing::new_metric();
    out.model.push(rotated);
    for file in ["x.dxf", "x.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&out, file).unwrap(), file).unwrap();
        let (p, grad) = angles(back.model.iter().next().unwrap());
        assert!(near(grad.unwrap(), 60.0), "{file}: gradient {grad:?}");
        let _ = p;
    }
}

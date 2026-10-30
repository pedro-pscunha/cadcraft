//! The arc, spline, text and fill world primitives: validation, the complete parent transform and the XY projection.

use cadcraft_doc::{Prim, Text, TextFrame, WorldFill, WorldFillKind, WorldGeometry, WorldPattern, WorldPrimitive, WorldText, WorldTextContent};
use cadcraft_geom::{Mat4, Segment, Vec3};

const X: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };
const Y: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn arc_point(center: Vec3, x: Vec3, y: Vec3, t: f64) -> Vec3 {
    center + x * t.cos() + y * t.sin()
}

fn matrix(rows: [[f64; 4]; 3]) -> Mat4 {
    Mat4 { m: [rows[0], rows[1], rows[2], [0.0, 0.0, 0.0, 1.0]] }
}

/// A non-uniform scale with a shear and a translation: it is an affine map but not a similarity.
fn shear() -> Mat4 {
    matrix([[2.0, 0.5, 0.0, 1.0], [0.0, 1.0, 0.25, -2.0], [0.0, 0.0, 3.0, 4.0]])
}

/// A quarter turn about Z, a uniform scale of 2 and a translation.
fn similarity() -> Mat4 {
    matrix([[0.0, -2.0, 0.0, 10.0], [2.0, 0.0, 0.0, -5.0], [0.0, 0.0, 2.0, 1.0]])
}

#[test]
fn an_arc_stays_exact_under_any_affine_parent() {
    let (c, x, y) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0));
    let arc = WorldPrimitive::Arc { center: c, x_axis: x, y_axis: y, start: 0.3, end: 2.0 };
    for parent in [similarity(), shear()] {
        let WorldPrimitive::Arc { center, x_axis, y_axis, start, end } = arc.transformed(&parent).unwrap() else { panic!("not an arc") };
        assert_eq!((start, end), (0.3, 2.0));
        for t in [0.3, 0.9, 1.7, 2.0] {
            assert!(near(arc_point(center, x_axis, y_axis, t), parent.apply(arc_point(c, x, y, t))), "t = {t}");
        }
    }
}

#[test]
fn a_spline_moves_its_controls_only() {
    let spline = WorldPrimitive::Spline {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![Vec3::ZERO, Vec3::new(1.0, 2.0, 0.0), Vec3::new(3.0, 0.0, 1.0)],
        weights: vec![1.0, 0.5, 1.0],
    };
    let WorldPrimitive::Spline { degree, knots, control, weights } = spline.transformed(&shear()).unwrap() else { panic!("not a spline") };
    assert_eq!((degree, knots, weights), (2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], vec![1.0, 0.5, 1.0]));
    assert!(near(control[1], shear().apply(Vec3::new(1.0, 2.0, 0.0))));
}

#[test]
fn malformed_primitives_are_refused() {
    let bad_knots = WorldPrimitive::Spline { degree: 2, knots: vec![0.0, 1.0], control: vec![Vec3::ZERO; 3], weights: Vec::new() };
    let bad_weights = WorldPrimitive::Spline { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], control: vec![Vec3::ZERO; 2], weights: vec![1.0] };
    let nan = WorldPrimitive::Arc { center: Vec3::new(f64::NAN, 0.0, 0.0), x_axis: X, y_axis: Y, start: 0.0, end: 1.0 };
    for bad in [bad_knots, bad_weights, nan] {
        assert!(bad.validate().is_err(), "{bad:?}");
        assert!(WorldGeometry::from_primitives(vec![bad]).is_err());
    }
    let mut perspective = Mat4::IDENTITY;
    perspective.m[3][0] = 0.1;
    assert!(WorldPrimitive::Point(Vec3::ZERO).transformed(&perspective).is_err());
}

fn text() -> WorldPrimitive {
    let t = Text {
        insert: Vec3::ZERO,
        align_pt: None,
        height: 2.0,
        value: "T".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
        backward: false,
        upside_down: false,
    };
    let frame = TextFrame { origin: Vec3::new(1.0, 2.0, 0.0), x_axis: X, y_axis: Y, normal: Vec3::Z };
    WorldPrimitive::Text(WorldText { frame, align: Some(Vec3::new(3.0, 2.0, 0.0)), content: WorldTextContent::Text(t) })
}

#[test]
fn a_text_follows_a_similarity_and_refuses_anything_else() {
    let WorldPrimitive::Text(moved) = text().transformed(&similarity()).unwrap() else { panic!("not a text") };
    assert!(near(moved.frame.origin, similarity().apply(Vec3::new(1.0, 2.0, 0.0))));
    assert!(near(moved.frame.x_axis, Y) && near(moved.frame.y_axis, Vec3::new(-1.0, 0.0, 0.0)) && near(moved.frame.normal, Vec3::Z));
    assert!(near(moved.align.unwrap(), similarity().apply(Vec3::new(3.0, 2.0, 0.0))));
    let WorldTextContent::Text(t) = moved.content else { panic!("not a text") };
    assert_eq!(t.height, 4.0, "the scale grows the height");
    assert!(text().transformed(&shear()).is_err());
    let mirror = matrix([[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
    assert!(text().transformed(&mirror).is_err());
    // A text has no 2D primitive: the frame stays in the geometry.
    assert!(WorldGeometry::from_primitives(vec![text()]).unwrap().project_xy().is_empty());
}

fn square() -> Vec<WorldPrimitive> {
    let p = [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 0.0)];
    (0..4).map(|i| WorldPrimitive::Line { a: p[i], b: p[(i + 1) % 4] }).collect()
}

fn fill(kind: WorldFillKind) -> WorldPrimitive {
    WorldPrimitive::Fill(WorldFill { loops: vec![square()], kind, style: 0 })
}

fn pattern() -> WorldFillKind {
    WorldFillKind::Pattern(WorldPattern {
        name: "ANSI31".into(),
        scale: 1.5,
        angle: 0.25,
        origin: Vec3::new(1.0, 1.0, 0.0),
        x_axis: X,
        y_axis: Y,
        lines: Vec::new(),
    })
}

#[test]
fn solid_and_wipeout_fills_accept_any_affine_but_patterns_need_a_similarity() {
    for kind in [WorldFillKind::Solid, WorldFillKind::Wipeout] {
        let WorldPrimitive::Fill(moved) = fill(kind).transformed(&shear()).unwrap() else { panic!("not a fill") };
        let WorldPrimitive::Line { b, .. } = &moved.loops[0][1] else { panic!("not a line") };
        assert!(near(*b, shear().apply(Vec3::new(2.0, 2.0, 0.0))));
    }
    assert!(fill(pattern()).transformed(&shear()).is_err());
    let WorldPrimitive::Fill(moved) = fill(pattern()).transformed(&similarity()).unwrap() else { panic!("not a fill") };
    let WorldFillKind::Pattern(p) = moved.kind else { panic!("not a pattern") };
    assert_eq!((p.scale, p.angle), (3.0, 0.25));
    assert!(near(p.origin, similarity().apply(Vec3::new(1.0, 1.0, 0.0))) && near(p.x_axis, Y));
}

#[test]
fn a_fill_projects_to_a_polygon_per_loop() {
    let prims = WorldGeometry::from_primitives(vec![fill(WorldFillKind::Solid)]).unwrap().project_xy();
    assert!(matches!(prims.as_slice(), [Prim::Fill(points)] if points.len() >= 4));
}

#[test]
fn an_arc_projects_to_an_ellipse_and_an_edge_on_circle_to_a_segment() {
    let circle =
        |y: Vec3| WorldPrimitive::Arc { center: Vec3::ZERO, x_axis: Vec3::new(2.0, 0.0, 0.0), y_axis: y, start: 0.0, end: std::f64::consts::TAU };
    let flat = WorldGeometry::from_primitives(vec![circle(Vec3::new(0.0, 2.0, 0.0))]).unwrap().project_xy();
    assert!(matches!(flat.as_slice(), [Prim::Ellipse(e)] if (e.major.len() - 2.0).abs() < 1e-9 && (e.ratio - 1.0).abs() < 1e-9));
    // The same circle seen edge-on (its Y radius along Z): a segment of length 4 on the X axis.
    let edge = WorldGeometry::from_primitives(vec![circle(Vec3::new(0.0, 0.0, 2.0))]).unwrap().project_xy();
    let [Prim::Seg(Segment::Line(l))] = edge.as_slice() else { panic!("{edge:?}") };
    assert!((l.a.dist(l.b) - 4.0).abs() < 1e-9 && l.a.y == 0.0 && l.b.y == 0.0);
}

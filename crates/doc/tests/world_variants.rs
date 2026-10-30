//! The arc, spline, text and fill world primitives: validation, the complete parent transform and the XY projection.

use cadcraft_doc::{
    Gradient, Handle, Prim, Text, TextFrame, WorldFill, WorldFillKind, WorldGeometry, WorldGradient, WorldPattern, WorldPrimitive, WorldText,
    WorldTextContent,
};
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
fn an_unrepresentable_projected_parameter_span_refuses_instead_of_becoming_full() {
    for axis_u in [Vec3::new(1e18, 0.0, 0.0), Vec3::new(0.0, 0.0, 1e18)] {
        let geometry = WorldGeometry::from_primitives(
            Handle(0x124),
            vec![WorldPrimitive::Conic { center: Vec3::ZERO, axis_u, axis_v: Vec3::new(0.0, 2e18, 0.0), start: 0.0, end: 1e-18 }],
        )
        .unwrap();
        let error = geometry.project_xy().unwrap_err();
        assert!(error.to_string().contains("124") && error.to_string().contains("span"), "{error}");
    }
}

#[test]
fn an_arc_stays_exact_under_any_affine_parent() {
    let (c, x, y) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0));
    let arc = WorldPrimitive::Conic { center: c, axis_u: x, axis_v: y, start: 0.3, end: 2.0 };
    for parent in [similarity(), shear()] {
        let WorldPrimitive::Conic { center, axis_u, axis_v, start, end } = arc.transformed(&parent).unwrap() else { panic!("not an arc") };
        assert_eq!((start, end), (0.3, 2.0));
        for t in [0.3, 0.9, 1.7, 2.0] {
            assert!(near(arc_point(center, axis_u, axis_v, t), parent.apply(arc_point(c, x, y, t))), "t = {t}");
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
fn hostile_spline_degree_refuses_without_arithmetic_overflow() {
    let spline = WorldPrimitive::Spline { degree: usize::MAX, knots: Vec::new(), control: vec![Vec3::ZERO], weights: Vec::new() };
    assert!(spline.validate().is_err());
}

#[test]
fn nonfinite_fill_payloads_and_source_frames_are_refused() {
    let WorldFillKind::Pattern(mut p) = pattern() else { panic!("not pattern") };
    p.lines.push(cadcraft_doc::library::PatternLine { angle: 0.0, origin: (0.0, 0.0), delta: (1.0, 1.0), dashes: vec![f64::NAN] });
    let gradient = WorldGradient {
        gradient: Gradient {
            name: "LINEAR".into(),
            color1: cadcraft_color::Color::Index(1),
            color2: cadcraft_color::Color::Index(5),
            angle: f64::NAN,
            centered: false,
        },
        origin: Vec3::ZERO,
        x_axis: X,
        y_axis: Y,
    };
    for primitive in [fill(WorldFillKind::Pattern(p)), fill(WorldFillKind::Gradient(gradient))] {
        assert!(primitive.validate().is_err());
        assert!(WorldGeometry::from_primitives(Handle(0), vec![primitive]).is_err());
    }
    let WorldPrimitive::Text(mut text) = text() else { panic!("not text") };
    text.frame.normal.x = f64::NAN;
    assert!(WorldPrimitive::Text(text).transformed(&Mat4::IDENTITY).is_err());
}

#[test]
fn malformed_primitives_are_refused() {
    let bad_knots = WorldPrimitive::Spline { degree: 2, knots: vec![0.0, 1.0], control: vec![Vec3::ZERO; 3], weights: Vec::new() };
    let bad_weights = WorldPrimitive::Spline { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], control: vec![Vec3::ZERO; 2], weights: vec![1.0] };
    let nan = WorldPrimitive::Conic { center: Vec3::new(f64::NAN, 0.0, 0.0), axis_u: X, axis_v: Y, start: 0.0, end: 1.0 };
    for bad in [bad_knots, bad_weights, nan] {
        assert!(bad.validate().is_err(), "{bad:?}");
        assert!(WorldGeometry::from_primitives(Handle(0), vec![bad]).is_err());
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
fn text_layout_points_follow_every_affine_parent() {
    let WorldPrimitive::Text(source) = text() else { panic!("not a text") };
    let mirror = matrix([[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
    for parent in [similarity(), shear(), mirror] {
        let WorldPrimitive::Text(moved) = text().transformed(&parent).unwrap() else { panic!("not a text") };
        assert_eq!(moved.content, source.content, "layout sizes stay local");
        for (x, y) in [(0.0, 0.0), (2.0, 3.0), (-1.0, 4.0)] {
            let original = source.frame.origin + source.frame.x_axis * x + source.frame.y_axis * y;
            let actual = moved.frame.origin + moved.frame.x_axis * x + moved.frame.y_axis * y;
            assert!(near(actual, parent.apply(original)), "parent {parent:?}");
        }
        assert!(near(moved.align.unwrap(), parent.apply(source.align.unwrap())));
        assert!(moved.frame.normal.dot(moved.frame.x_axis).abs() < 1e-9);
        assert!(moved.frame.normal.dot(moved.frame.y_axis).abs() < 1e-9);
    }
}

#[test]
fn affine_text_composition_and_collapsed_frames_remain_displayable() {
    let first = text().transformed(&shear()).unwrap();
    let actual = first.transformed(&similarity()).unwrap();
    let expected = text().transformed(&similarity().mul(&shear())).unwrap();
    let (WorldPrimitive::Text(actual), WorldPrimitive::Text(expected)) = (actual, expected) else { panic!("not text") };
    assert!(near(actual.frame.origin, expected.frame.origin));
    assert!(near(actual.frame.x_axis, expected.frame.x_axis) && near(actual.frame.y_axis, expected.frame.y_axis));
    assert!(near(actual.frame.normal, expected.frame.normal));
    assert_eq!(actual.content, expected.content);
    let collapsed = matrix([[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]]);
    let WorldPrimitive::Text(flat) = text().transformed(&collapsed).unwrap() else { panic!("not text") };
    assert_eq!(flat.frame.normal, Vec3::ZERO);
    assert_eq!(flat.frame.y_axis, Vec3::ZERO);
}

#[test]
fn text_keeps_its_frame_outside_the_curve_projection() {
    assert!(WorldGeometry::from_primitives(Handle(0), vec![text()]).unwrap().project_xy().unwrap().is_empty());
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
fn all_fill_bases_accept_affine_parents() {
    for kind in [WorldFillKind::Solid, WorldFillKind::Wipeout] {
        let WorldPrimitive::Fill(moved) = fill(kind).transformed(&shear()).unwrap() else { panic!("not a fill") };
        let WorldPrimitive::Line { b, .. } = &moved.loops[0][1] else { panic!("not a line") };
        assert!(near(*b, shear().apply(Vec3::new(2.0, 2.0, 0.0))));
    }
    for parent in [shear(), similarity()] {
        let WorldPrimitive::Fill(moved) = fill(pattern()).transformed(&parent).unwrap() else { panic!("not a fill") };
        let WorldFillKind::Pattern(p) = moved.kind else { panic!("not a pattern") };
        assert_eq!((p.scale, p.angle), (1.5, 0.25), "pattern lengths stay local");
        let original = Vec3::new(1.0, 1.0, 0.0) + X * 2.0 + Y * 3.0;
        assert!(near(p.origin + p.x_axis * 2.0 + p.y_axis * 3.0, parent.apply(original)));
    }
}

#[test]
fn a_gradient_retains_its_affine_basis_and_local_parameters() {
    let gradient = Gradient {
        name: "LINEAR".into(),
        color1: cadcraft_color::Color::Index(1),
        color2: cadcraft_color::Color::Index(5),
        angle: 0.5,
        centered: false,
    };
    let source = WorldGradient { gradient: gradient.clone(), origin: Vec3::new(1.0, 2.0, 3.0), x_axis: X, y_axis: Y };
    let WorldPrimitive::Fill(moved) = fill(WorldFillKind::Gradient(source.clone())).transformed(&shear()).unwrap() else { panic!("not fill") };
    let WorldFillKind::Gradient(moved) = moved.kind else { panic!("not gradient") };
    assert_eq!(moved.gradient, gradient);
    assert!(near(moved.origin + moved.x_axis * 2.0 + moved.y_axis * 3.0, shear().apply(source.origin + X * 2.0 + Y * 3.0)));
}

#[test]
fn a_fill_projects_to_a_polygon_per_loop() {
    let prims = WorldGeometry::from_primitives(Handle(0), vec![fill(WorldFillKind::Solid)]).unwrap().project_xy().unwrap();
    assert!(matches!(prims.as_slice(), [Prim::Fill(points)] if points.len() >= 4));
}

#[test]
fn an_arc_projects_to_an_ellipse_and_an_edge_on_circle_to_a_segment() {
    let circle =
        |y: Vec3| WorldPrimitive::Conic { center: Vec3::ZERO, axis_u: Vec3::new(2.0, 0.0, 0.0), axis_v: y, start: 0.0, end: std::f64::consts::TAU };
    let flat = WorldGeometry::from_primitives(Handle(0), vec![circle(Vec3::new(0.0, 2.0, 0.0))]).unwrap().project_xy().unwrap();
    assert!(matches!(flat.as_slice(), [Prim::Ellipse(e)] if (e.major.len() - 2.0).abs() < 1e-9 && (e.ratio - 1.0).abs() < 1e-9));
    // The same circle seen edge-on (its Y radius along Z): a segment of length 4 on the X axis.
    let edge = WorldGeometry::from_primitives(Handle(0), vec![circle(Vec3::new(0.0, 0.0, 2.0))]).unwrap().project_xy().unwrap();
    let [Prim::Seg(Segment::Line(l))] = edge.as_slice() else { panic!("{edge:?}") };
    assert!((l.a.dist(l.b) - 4.0).abs() < 1e-9 && l.a.y == 0.0 && l.b.y == 0.0);
}

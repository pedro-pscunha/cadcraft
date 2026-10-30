//! A canonical entity with a thickness is swept along the normal of its plane: its outline, the far copy of the outline and the side
//! edges, projected for display. Each family is checked against XY positions worked out independently; the legacy painter ignores a
//! thickness (the plan view), which a control keeps.

use cadcraft_doc::{Arc, Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line, LwPolyline, Point, Solid, Space, Text};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_render::{DisplayList, Kind, Options, build};

/// The plane of every case: OCS X is world -X and OCS Y is (0, -0.8, 0.6); the unit normal is (0, 0.6, 0.8).
const NORMAL: Vec3 = Vec3 { x: 0.0, y: 3.0, z: 4.0 };

fn drawing(kind: EntityKind, thickness: f64, canonical: bool) -> Drawing {
    let mut d = Drawing::new_metric();
    let mut e = Entity::new(Handle(0x400), kind);
    if canonical {
        e.geometry_revision = GeometryRevision::CanonicalV2;
    }
    e.common.extrusion = NORMAL;
    e.common.thickness = thickness;
    d.model.push(e);
    d
}

fn fine() -> Options {
    Options { tolerance: 1e-5, ..Options::default() }
}

fn polylines(list: &DisplayList) -> Vec<Vec<Vec2>> {
    list.prims.iter().filter(|p| p.kind == Kind::Polyline).map(|p| list.points(p).to_vec()).collect()
}

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).len() < 1e-9
}

/// Whether there is a two-point polyline from `a` to `b` (either way).
fn has_edge(lines: &[Vec<Vec2>], a: Vec2, b: Vec2) -> bool {
    lines.iter().any(|l| l.len() == 2 && ((near(l[0], a) && near(l[1], b)) || (near(l[0], b) && near(l[1], a))))
}

fn extent(lines: &[Vec<Vec2>]) -> (Vec2, Vec2) {
    let pts = lines.iter().flatten();
    pts.fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

fn build_lines(d: &Drawing) -> Vec<Vec<Vec2>> {
    polylines(&build(d, &Space::Model, &fine()))
}

#[test]
fn a_circle_has_a_far_copy_and_four_side_edges_at_its_quadrants() {
    // Radius 2 centered at the world point (-1, 1.4, 5.2) (OCS (1, 2, 5)), thickness 5: the far copy is centered at
    // center + 5 * (0, 0.6, 0.8) = (-1, 4.4, 9.2). The quadrants project to (-3, 1.4), (-1, -0.2), (1, 1.4) and (-1, 3.0).
    let d = drawing(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 2.0 }), 5.0, true);
    let lines = build_lines(&d);
    let curves: Vec<Vec<Vec2>> = lines.iter().filter(|l| l.len() > 8).cloned().collect();
    assert_eq!(curves.len(), 2, "the circle and its far copy");
    let (near_lo, near_hi) = extent(&curves[..1]);
    let (far_lo, far_hi) = extent(&curves[1..]);
    // The tessellation (tolerance 1e-5) reaches the extremes to within its sagitta.
    let close = |a: Vec2, b: Vec2| (a - b).len() < 1e-4;
    assert!(close(near_lo, Vec2::new(-3.0, -0.2)) && close(near_hi, Vec2::new(1.0, 3.0)), "{near_lo:?} {near_hi:?}");
    assert!(close(far_lo, Vec2::new(-3.0, 2.8)) && close(far_hi, Vec2::new(1.0, 6.0)), "{far_lo:?} {far_hi:?}");
    for (x, y) in [(-3.0, 1.4), (-1.0, -0.2), (1.0, 1.4), (-1.0, 3.0)] {
        assert!(has_edge(&lines, Vec2::new(x, y), Vec2::new(x, y + 3.0)), "the edge at ({x}, {y})");
    }
}

#[test]
fn a_line_is_a_rectangle() {
    // From (0,0,0) to (4,0,0) with thickness 5: the far copy is 5 * (0, 0.6, 0.8), (0, 3) in the view.
    let d = drawing(EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(4.0, 0.0, 0.0) }), 5.0, true);
    let lines = build_lines(&d);
    assert!(has_edge(&lines, Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0)) && has_edge(&lines, Vec2::new(0.0, 3.0), Vec2::new(4.0, 3.0)));
    assert!(has_edge(&lines, Vec2::new(0.0, 0.0), Vec2::new(0.0, 3.0)) && has_edge(&lines, Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0)));
    assert_eq!(lines.len(), 4);
}

#[test]
fn an_arc_has_edges_at_its_ends_and_a_negative_thickness_goes_the_other_way() {
    // Radius 2 from 0 to 90 degrees around the origin: the ends are the world points (-2, 0, 0) and (0, -1.6, 1.2); thickness -1 is
    // -(0, 0.6, 0.8), (0, -0.6) in the view.
    let d = drawing(EntityKind::Arc(Arc { center: Vec3::ZERO, radius: 2.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 }), -1.0, true);
    let lines = build_lines(&d);
    assert!(has_edge(&lines, Vec2::new(-2.0, 0.0), Vec2::new(-2.0, -0.6)), "{lines:?}");
    assert!(has_edge(&lines, Vec2::new(0.0, -1.6), Vec2::new(0.0, -2.2)), "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.len() == 2).count(), 2, "the two side edges");
}

#[test]
fn a_point_is_a_line_to_its_far_copy() {
    // The point (1, 2, 3) with thickness 2: 2 * (0, 0.6, 0.8) = (0, 1.2) in the view.
    let d = drawing(EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.0 }), 2.0, true);
    assert!(has_edge(&build_lines(&d), Vec2::new(1.0, 2.0), Vec2::new(1.0, 3.2)));
}

#[test]
fn a_solid_has_edges_at_its_four_corners() {
    // OCS corners (0,0), (2,0), (0,2), (2,2): the view positions (0,0), (-2,0), (0,-1.6), (-2,-1.6); thickness 5 shifts by (0, 3).
    let corners = [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0), Vec3::new(2.0, 2.0, 0.0)];
    let d = drawing(EntityKind::Solid(Solid { corners }), 5.0, true);
    let lines = build_lines(&d);
    for (x, y) in [(0.0, 0.0), (-2.0, 0.0), (0.0, -1.6), (-2.0, -1.6)] {
        assert!(has_edge(&lines, Vec2::new(x, y), Vec2::new(x, y + 3.0)), "the edge at ({x}, {y})");
    }
}

fn polyline(width: f64) -> EntityKind {
    EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0)]
            .iter()
            .map(|(x, y)| PolyVertex { p: Vec2::new(*x, *y), bulge: 0.0, start_width: 0.0, end_width: 0.0 })
            .collect(),
        closed: false,
        const_width: width,
        elevation: 0.0,
        plinegen: false,
        heavy: false,
        fit: None,
    })
}

#[test]
fn a_polyline_has_edges_at_its_vertices_and_a_wide_one_a_far_band() {
    // The vertices are (0,0), (-2,0) and (-2,-1.6) in the view; thickness 5 shifts by (0, 3).
    let narrow = build_lines(&drawing(polyline(0.0), 5.0, true));
    for (x, y) in [(0.0, 0.0), (-2.0, 0.0), (-2.0, -1.6)] {
        assert!(has_edge(&narrow, Vec2::new(x, y), Vec2::new(x, y + 3.0)), "the edge at ({x}, {y})");
    }
    // A wide one: the band and its far copy (the far triangles are the near ones shifted).
    let list = build(&drawing(polyline(0.5), 5.0, true), &Space::Model, &fine());
    let bands: Vec<&[Vec2]> = list.prims.iter().filter(|p| p.kind == Kind::Tris).map(|p| list.points(p)).collect();
    assert_eq!(bands.len(), 2, "the band and its far copy");
    assert_eq!(bands[0].len(), bands[1].len());
    assert!(bands[0].iter().zip(bands[1]).all(|(a, b)| near(*a + Vec2::new(0.0, 3.0), *b)));
    assert!(has_edge(&polylines(&list), Vec2::new(-2.0, 0.0), Vec2::new(-2.0, 3.0)));
}

#[test]
fn a_text_has_a_far_copy_of_its_strokes() {
    let text = EntityKind::Text(Text {
        insert: Vec3::new(1.0, 1.0, 0.0),
        align_pt: None,
        height: 1.0,
        value: "AB".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: cadcraft_doc::HAlign::Left,
        valign: cadcraft_doc::VAlign::Baseline,
        backward: false,
        upside_down: false,
    });
    let plain = build_lines(&drawing(text.clone(), 0.0, true));
    let swept = build_lines(&drawing(text, 5.0, true));
    assert!(!plain.is_empty());
    assert!(swept.len() > 2 * plain.len(), "the strokes, their far copy and the side edges");
    assert_eq!(&swept[..plain.len()], &plain[..]);
    for (a, b) in plain.iter().zip(&swept[plain.len()..]) {
        assert!(a.iter().zip(b).all(|(p, q)| near(*p + Vec2::new(0.0, 3.0), *q)), "{a:?} vs {b:?}");
    }
}

#[test]
fn the_legacy_painter_ignores_a_thickness() {
    let circle = |thickness| drawing(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 0.0), radius: 2.0 }), thickness, false);
    let (flat, thick) = (build(&circle(0.0), &Space::Model, &fine()), build(&circle(5.0), &Space::Model, &fine()));
    assert_eq!(flat.verts, thick.verts);
    assert_eq!(flat.prims.len(), thick.prims.len());
}

#[test]
fn a_thickness_in_the_xy_plane_is_edge_on_and_adds_nothing_visible() {
    let mut d = drawing(EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(4.0, 0.0, 0.0) }), 5.0, true);
    let mut e = d.model.iter().next().unwrap().as_ref().clone();
    e.common.extrusion = Vec3::Z;
    d.model = Default::default();
    d.model.push(e);
    let lines = build_lines(&d);
    assert_eq!(lines.len(), 2, "the line and its far copy on top of it: {lines:?}");
}

#[test]
fn a_text_in_an_outline_font_has_no_thickness_as_in_autocad() {
    // A text style set in an installed TrueType face: AutoCAD extrudes SHX text only, so the text is drawn flat.
    let Some(face) = ["Arial", "Helvetica", "DejaVu Sans", "Liberation Sans"]
        .into_iter()
        .find(|f| matches!(cadcraft_fonts::TextFont::resolve(f), cadcraft_fonts::TextFont::Outline(_)))
    else {
        return;
    };
    let text = |thickness| {
        let mut d = drawing(
            EntityKind::Text(Text {
                insert: Vec3::new(1.0, 1.0, 0.0),
                align_pt: None,
                height: 1.0,
                value: "AB".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Outline".into(),
                halign: cadcraft_doc::HAlign::Left,
                valign: cadcraft_doc::VAlign::Baseline,
                backward: false,
                upside_down: false,
            }),
            thickness,
            true,
        );
        d.text_styles.push(cadcraft_doc::TextStyle { name: "Outline".into(), font: face.into(), ..cadcraft_doc::TextStyle::default() });
        build(&d, &Space::Model, &fine())
    };
    let (flat, thick) = (text(0.0), text(5.0));
    assert_eq!(flat.prims.len(), thick.prims.len());
    assert_eq!(flat.tris.len(), thick.tris.len());
    assert_eq!(flat.verts, thick.verts);
}

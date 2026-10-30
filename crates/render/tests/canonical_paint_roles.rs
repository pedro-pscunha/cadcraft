//! Line patterns, wide polylines and point markers of canonical entities: drawn along the curve in WCS, in the plane of the
//! entity, and projected for display. Each is checked against XYZ values worked out independently, and against the legacy painter
//! for an entity in the XY plane.

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line, Linetype, LwPolyline, Point, Space};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_render::{DisplayList, Kind, Options, build};

fn drawing_with(kind: EntityKind, normal: Vec3, canonical: bool) -> Drawing {
    let mut d = Drawing::new_metric();
    d.linetypes.push(Linetype::simple("DASHED", "dashes", &[0.5, -0.5]));
    let mut e = Entity::new(Handle(0x200), kind);
    if canonical {
        e.geometry_revision = GeometryRevision::CanonicalV2;
    }
    e.common.extrusion = normal;
    e.common.linetype = "DASHED".into();
    d.model.push(e);
    d
}

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).len() < 1e-9
}

fn polylines(list: &DisplayList) -> Vec<Vec<Vec2>> {
    list.prims.iter().filter(|p| p.kind == Kind::Polyline).map(|p| list.points(p).to_vec()).collect()
}

/// The Z of the point of the plane through `through` with normal `n` that lies above the XY position `p`.
fn on_plane(p: Vec2, through: Vec3, n: Vec3) -> Vec3 {
    Vec3::new(p.x, p.y, through.z - (n.x * (p.x - through.x) + n.y * (p.y - through.y)) / n.z)
}

#[test]
fn a_pattern_runs_along_the_length_of_a_spatial_line_not_its_projection() {
    // From (0,0,0) to (10,0,10): 14.142 long, 10 long in the view. Each 0.5 dash covers 0.35355 in the view.
    let d = drawing_with(EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 10.0) }), Vec3::Z, true);
    let dashes = polylines(&build(&d, &Space::Model, &Options::default()));
    assert_eq!(dashes.len(), 15, "14.142 long: a dash and a gap are 1");
    let length = |dash: &[Vec2]| (dash[dash.len() - 1].x - dash[0].x) * std::f64::consts::SQRT_2;
    for dash in &dashes[..dashes.len() - 1] {
        assert!((length(dash) - 0.5).abs() < 1e-9, "a dash is 0.5 along the line: {dash:?}");
    }
    for pair in dashes.windows(2) {
        let gap = (pair[1][0].x - pair[0][pair[0].len() - 1].x) * std::f64::consts::SQRT_2;
        assert!((gap - 0.5).abs() < 1e-9, "a gap is 0.5 along the line: {gap}");
    }
}

#[test]
fn a_pattern_in_the_xy_plane_is_what_the_legacy_painter_draws() {
    let line = |canonical| drawing_with(EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 0.0), b: Vec3::new(9.0, 6.0, 0.0) }), Vec3::Z, canonical);
    let (legacy, canonical) =
        (polylines(&build(&line(false), &Space::Model, &Options::default())), polylines(&build(&line(true), &Space::Model, &Options::default())));
    assert_eq!(legacy.len(), canonical.len());
    for (a, b) in legacy.iter().zip(&canonical) {
        assert!(a.len() == b.len() && a.iter().zip(b).all(|(p, q)| near(*p, *q)), "{a:?} vs {b:?}");
    }
}

#[test]
fn a_pattern_on_a_tilted_circle_has_dashes_of_the_true_arc_length() {
    let normal = Vec3::new(0.0, 3.0, 4.0);
    let d = drawing_with(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }), normal, true);
    let dashes = polylines(&build(&d, &Space::Model, &Options { tolerance: 1e-5, ..Options::default() }));
    assert!(dashes.len() > 10);
    // Lift the dash ends back onto the circle's plane: the center is (-1, 1.4, 5.2) and the unit normal (0, 0.6, 0.8).
    let (center, n) = (Vec3::new(-1.0, 1.4, 5.2), Vec3::new(0.0, 0.6, 0.8));
    for dash in &dashes[..dashes.len() - 1] {
        let (a, b) = (on_plane(dash[0], center, n), on_plane(dash[dash.len() - 1], center, n));
        assert!(((a - center).len() - 4.0).abs() < 1e-4 && ((b - center).len() - 4.0).abs() < 1e-4, "on the circle: {a:?} {b:?}");
        // The chord of an arc of 0.5 on a radius of 4 is 2 * 4 * sin(0.5 / 8) = 0.49959.
        assert!(((b - a).len() - 0.49959).abs() < 1e-3, "{}", (b - a).len());
    }
}

fn wide(const_width: f64, vertices: Vec<(f64, f64)>) -> EntityKind {
    EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: vertices.into_iter().map(|(x, y)| PolyVertex { p: Vec2::new(x, y), bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
        closed: false,
        const_width,
        elevation: 1.0,
        plinegen: false,
        heavy: false,
        fit: None,
    })
}

#[test]
fn a_wide_polyline_is_a_band_in_its_own_plane_projected_for_display() {
    // OCS of the normal (0, 3, 4): X is world -X, Y is (0, -0.8, 0.6). The segment (0,0)-(10,0) at elevation 1 is (0, 0.6, 0.8) to
    // (-10, 0.6, 0.8); a band 2 wide has its corners 1 along OCS Y: (0, -0.2, 1.4) and (0, 1.4, 0.2), and the same at x = -10.
    let mut d = drawing_with(wide(2.0, vec![(0.0, 0.0), (10.0, 0.0)]), Vec3::new(0.0, 3.0, 4.0), true);
    let mut solid = d.model.iter().next().unwrap().as_ref().clone();
    solid.common.linetype = "Continuous".into();
    d.model = Default::default();
    d.model.push(solid);
    let list = build(&d, &Space::Model, &Options::default());
    let bands: Vec<_> = list.prims.iter().filter(|p| p.kind == Kind::Tris).collect();
    assert!(!bands.is_empty(), "a band");
    let corners = [Vec2::new(0.0, -0.2), Vec2::new(0.0, 1.4), Vec2::new(-10.0, -0.2), Vec2::new(-10.0, 1.4)];
    let points: Vec<Vec2> = bands.iter().flat_map(|band| list.points(band).iter().copied()).collect();
    // Every triangle corner is on the band (x from -10 to 0 and y from -0.2 to 1.4 here: the band is axis-aligned in the view), and
    // the four corners of the band are among them.
    assert!(points.iter().all(|p| p.x >= -10.0 - 1e-9 && p.x <= 1e-9 && p.y >= -0.2 - 1e-9 && p.y <= 1.4 + 1e-9), "{points:?}");
    let (min_x, max_x) = points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    assert!(corners.iter().all(|c| points.iter().any(|p| near(*p, *c))), "x from {min_x} to {max_x}");
}

#[test]
fn a_wide_polyline_in_the_xy_plane_is_what_the_legacy_painter_draws() {
    let draw = |canonical| {
        let mut d = drawing_with(wide(0.5, vec![(0.0, 0.0), (6.0, 0.0), (6.0, 4.0)]), Vec3::Z, canonical);
        d.linetypes.clear();
        for e in d.model.iter() {
            assert_eq!(e.geometry_revision == GeometryRevision::CanonicalV2, canonical);
        }
        let mut model = Vec::new();
        for e in d.model.iter() {
            let mut e = e.as_ref().clone();
            e.common.linetype = "Continuous".into();
            model.push(e);
        }
        d.model = Default::default();
        model.into_iter().for_each(|e| d.model.push(e));
        build(&d, &Space::Model, &Options::default())
    };
    let (legacy, canonical) = (draw(false), draw(true));
    assert_eq!(legacy.tris.len(), canonical.tris.len());
    assert!(legacy.tris.iter().zip(&canonical.tris).all(|(a, b)| near(*a, *b)));
    assert_eq!(legacy.prims.len(), canonical.prims.len(), "the hairline centre line too");
}

fn marker_drawing(canonical: bool, normal: Vec3) -> Drawing {
    let mut d = drawing_with(EntityKind::Point(Point { p: Vec3::new(2.0, 3.0, 4.0), angle: 0.0 }), normal, canonical);
    d.header.set_i64("PDMODE", 2);
    d.header.set_f64("PDSIZE", 2.0);
    d
}

#[test]
fn a_point_marker_is_drawn_in_the_plane_of_the_point() {
    // A plus 2 across in the plane of the normal (0, 3, 4): its arms run along OCS X (world -X) and OCS Y (0, -0.8, 0.6).
    let list = build(&marker_drawing(true, Vec3::new(0.0, 3.0, 4.0)), &Space::Model, &Options::default());
    let lines = polylines(&list);
    assert_eq!(lines.len(), 2);
    let ends = |l: &[Vec2]| (l[0], l[l.len() - 1]);
    let (a, b) = ends(&lines[0]);
    assert!(near(a, Vec2::new(3.0, 3.0)) && near(b, Vec2::new(1.0, 3.0)), "{:?}", lines[0]);
    let (c, e) = ends(&lines[1]);
    assert!(near(c, Vec2::new(2.0, 3.8)) && near(e, Vec2::new(2.0, 2.2)), "{:?}", lines[1]);
}

#[test]
fn a_point_marker_in_the_xy_plane_is_what_the_legacy_painter_draws() {
    let (legacy, canonical) = (
        polylines(&build(&marker_drawing(false, Vec3::Z), &Space::Model, &Options::default())),
        polylines(&build(&marker_drawing(true, Vec3::Z), &Space::Model, &Options::default())),
    );
    assert_eq!(legacy.len(), canonical.len());
    for (a, b) in legacy.iter().zip(&canonical) {
        assert!(a.len() == b.len() && a.iter().zip(b).all(|(p, q)| near(*p, *q)), "{a:?} vs {b:?}");
    }
}

#[test]
fn a_dashed_wide_polyline_dashes_its_band_along_the_polyline() {
    // The same band with the 0.5 / 0.5 pattern: dashes at x 0 to -0.5, -1 to -1.5, ... and the last one at -9 to -9.5.
    let d = drawing_with(wide(2.0, vec![(0.0, 0.0), (10.0, 0.0)]), Vec3::new(0.0, 3.0, 4.0), true);
    let list = build(&d, &Space::Model, &Options::default());
    let xs: Vec<f64> = list.prims.iter().filter(|p| p.kind == Kind::Tris).flat_map(|p| list.points(p).iter().map(|q| q.x)).collect();
    let (min_x, max_x) = xs.iter().fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
    assert!((min_x + 9.5).abs() < 1e-9 && max_x.abs() < 1e-9, "{min_x} {max_x}");
}

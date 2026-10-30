//! Grips of a canonical entity come from its WCS geometry (the resolver pick and snap use) with the WCS point each has; dragging
//! one goes through the canonical transform or refuses typed, and never edits the 2D reading of the fields.

use cadcraft_doc::{Circle, Common, Entity, EntityKind, GeometryRevision, Handle, Line};
use cadcraft_engine::Session;
use cadcraft_engine::grips::GripMode;
use cadcraft_geom::{Vec2, Vec3};

fn session_with(kind: EntityKind, normal: Vec3) -> (Session, Handle) {
    let mut session = Session::new();
    let handle = session.doc_mut().unwrap().new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, ..Common::default() };
    session.doc_mut().unwrap().model.push(e);
    (session, handle)
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn kind(session: &Session, h: Handle) -> EntityKind {
    session.doc().unwrap().entity(h).unwrap().kind.clone()
}

/// A circle of radius 4 in the plane of normal (0, 3, 4): it projects to an ellipse (semi-axes 4 and 3.2) centered at (-1, 1.4),
/// the center at Z 5.2.
fn tilted_circle() -> (Session, Handle) {
    session_with(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }), Vec3::new(0.0, 3.0, 4.0))
}

#[test]
fn the_grips_of_a_tilted_circle_are_its_projected_center_and_quadrants_with_their_wcs_points() {
    let (session, h) = tilted_circle();
    let grips = session.grips_with_world(h).unwrap();
    assert_eq!(grips.len(), 5);
    assert!((grips[0].point - Vec2::new(-1.0, 1.4)).len() < 1e-9);
    assert!(near(grips[0].world.unwrap(), Vec3::new(-1.0, 1.4, 5.2)));
    // The quadrants are the circle's points along its OCS axes (OCS X is world -X, OCS Y is (0, -0.8, 0.6)): +X' and +Y' of the
    // circle, then their opposites.
    let want = [Vec3::new(-5.0, 1.4, 5.2), Vec3::new(-1.0, 1.4 - 3.2, 5.2 + 2.4), Vec3::new(3.0, 1.4, 5.2), Vec3::new(-1.0, 1.4 + 3.2, 5.2 - 2.4)];
    for (g, w) in grips[1..].iter().zip(want) {
        assert!(near(g.world.unwrap(), w), "{:?} vs {w:?}", g.world);
        assert!((g.point - w.xy()).len() < 1e-9);
    }
    assert_eq!(session.grips_of(h).unwrap().len(), 5);
}

#[test]
fn dragging_the_center_of_a_tilted_circle_moves_it_in_wcs_keeping_its_plane_and_z() {
    let (mut session, h) = tilted_circle();
    session.grip_edit(h, 0, Vec2::new(2.0, 3.4), GripMode::Stretch).unwrap();
    let EntityKind::Circle(c) = kind(&session, h) else { panic!("circle") };
    // Moved by (3, 2, 0) in WCS: the OCS center changes by the OCS image of that vector (x -3, y -1.6, z 1.2).
    let frame = cadcraft_geom::OcsFrame::new(Vec3::new(0.0, 3.0, 4.0)).unwrap();
    let want = Vec3::new(1.0, 2.0, 5.0) + frame.to_ocs(Vec3::new(3.0, 2.0, 0.0));
    assert!(near(c.center, want), "{:?} vs {want:?}", c.center);
    assert_eq!(c.radius, 4.0);
    assert_eq!(session.doc().unwrap().entity(h).unwrap().common.extrusion, Vec3::new(0.0, 3.0, 4.0));
}

#[test]
fn a_quadrant_of_a_tilted_circle_changes_its_radius_in_its_own_plane() {
    let (mut session, h) = tilted_circle();
    // Quadrant 0 is the circle point at (-5, 1.4, 5.2); dragging it to x = -6 along the same Y gives a radius of 5.
    session.grip_edit(h, 1, Vec2::new(-6.0, 1.4), GripMode::Stretch).unwrap();
    let EntityKind::Circle(c) = kind(&session, h) else { panic!("circle") };
    assert!((c.radius - 5.0).abs() < 1e-9 && near(c.center, Vec3::new(1.0, 2.0, 5.0)), "{c:?}");
    // Quadrant 1 is (-1, -1.8, 7.6) after the first drag moved only quadrant 0 further: use the new circle's.
    let (mut session, h) = tilted_circle();
    // The cursor at (-1, -3.8) is carried along Z onto the circle's plane: Z 9.1, so the point is (0, -5.2, 3.9) from the center.
    session.grip_edit(h, 2, Vec2::new(-1.0, -3.8), GripMode::Stretch).unwrap();
    let EntityKind::Circle(c) = kind(&session, h) else { panic!("circle") };
    assert!((c.radius - 6.5).abs() < 1e-9, "{c:?}");
    assert_eq!(session.doc().unwrap().entity(h).unwrap().common.extrusion, Vec3::new(0.0, 3.0, 4.0), "the plane stays");
}

#[test]
fn a_mirror_of_a_circle_grip_refuses_typed_and_changes_nothing() {
    let (mut session, h) = tilted_circle();
    let before = session.doc().unwrap().model.clone();
    let error = session.grip_edit(h, 0, Vec2::new(0.0, 9.0), GripMode::Mirror).unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(session.doc().unwrap().model, before);
}

#[test]
fn a_plane_the_view_direction_does_not_cross_refuses_a_stretch() {
    // A circle in a vertical plane (normal along X): the cursor cannot reach a point of that plane by its XY position.
    let (mut session, h) = session_with(EntityKind::Circle(Circle { center: Vec3::new(0.0, 0.0, 0.0), radius: 2.0 }), Vec3::new(1.0, 0.0, 0.0));
    let before = session.doc().unwrap().model.clone();
    let error = session.grip_edit(h, 1, Vec2::new(0.0, 5.0), GripMode::Stretch).unwrap_err();
    assert!(error.to_string().contains("canonical"), "{error}");
    assert_eq!(session.doc().unwrap().model, before);
}

#[test]
fn a_spatial_line_has_end_and_middle_grips_with_their_z_and_its_middle_moves_it_keeping_z() {
    let (mut session, h) = session_with(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(10.0, 0.0, 5.0) }), Vec3::Z);
    let grips = session.grips_with_world(h).unwrap();
    let worlds: Vec<Vec3> = grips.iter().map(|g| g.world.unwrap()).collect();
    assert_eq!(worlds, [Vec3::new(0.0, 0.0, 1.0), Vec3::new(5.0, 0.0, 3.0), Vec3::new(10.0, 0.0, 5.0)]);
    // An end moves in XY and keeps its Z.
    session.grip_edit(h, 2, Vec2::new(12.0, 3.0), GripMode::Stretch).unwrap();
    assert_eq!(kind(&session, h), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(12.0, 3.0, 5.0) }));
    // The middle moves the whole line, both Z values kept.
    session.grip_edit(h, 1, Vec2::new(7.0, 1.5), GripMode::Stretch).unwrap();
    let EntityKind::Line(l) = kind(&session, h) else { panic!("line") };
    assert!(near(l.a, Vec3::new(1.0, 0.0, 1.0)) && near(l.b, Vec3::new(13.0, 3.0, 5.0)), "{l:?}");
}

#[test]
fn the_move_mode_moves_a_canonical_object_through_the_canonical_transform() {
    let (mut session, h) = tilted_circle();
    session.grip_edit(h, 0, Vec2::new(4.0, 1.4), GripMode::Move).unwrap();
    let flat_grips = session.grips_with_world(h).unwrap();
    assert!(near(flat_grips[0].world.unwrap(), Vec3::new(4.0, 1.4, 5.2)), "{:?}", flat_grips[0]);
}

#[test]
fn a_canonical_entity_the_resolver_refuses_shows_no_grips() {
    let spline = cadcraft_geom::Spline::from_control(vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0), Vec2::new(6.0, 0.0)], 2);
    let (session, h) = session_with(EntityKind::Spline(spline), Vec3::Z);
    assert!(session.grips_of(h).unwrap().is_empty());
}

#[test]
fn a_legacy_line_keeps_its_grips_and_has_no_world_point() {
    let mut session = Session::new();
    let h = session.doc_mut().unwrap().new_handle();
    session.doc_mut().unwrap().model.push(Entity::new(h, EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 0.0, 0.0) })));
    let grips = session.grips_with_world(h).unwrap();
    assert_eq!(grips.iter().map(|g| g.point).collect::<Vec<_>>(), [Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(10.0, 0.0)]);
    assert!(grips.iter().all(|g| g.world.is_none()));
}

#[test]
fn an_arc_point_reshapes_the_arc_through_three_points_in_its_plane() {
    use cadcraft_doc::Arc;
    let (mut session, h) =
        session_with(EntityKind::Arc(Arc { center: Vec3::new(0.0, 0.0, 2.0), radius: 5.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 }), Vec3::Z);
    // Grips: start (5, 0), middle, end (0, 5), center. Drag the middle to (4, 4).
    session.grip_edit(h, 1, Vec2::new(4.0, 4.0), GripMode::Stretch).unwrap();
    let EntityKind::Arc(a) = kind(&session, h) else { panic!("arc") };
    assert_eq!(a.center.z, 2.0, "the elevation stays");
    let c = a.center.xy();
    for p in [Vec2::new(5.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 5.0)] {
        assert!((c.dist(p) - a.radius).abs() < 1e-9, "{p:?} is not on the new arc: {a:?}");
    }
}

#[test]
fn a_polyline_vertex_and_a_segment_middle_move_in_the_polylines_plane() {
    use cadcraft_doc::LwPolyline;
    use cadcraft_geom::PolyVertex;
    let vertex = |x: f64, y: f64| PolyVertex { p: Vec2::new(x, y), bulge: 0.0, start_width: 0.0, end_width: 0.0 };
    let polyline = LwPolyline {
        records: Default::default(),
        vertices: vec![vertex(0.0, 0.0), vertex(4.0, 0.0), vertex(4.0, 3.0)],
        closed: false,
        const_width: 0.0,
        elevation: 1.0,
        plinegen: false,
        heavy: false,
        fit: None,
    };
    let normal = Vec3::new(0.0, 3.0, 4.0);
    let (mut session, h) = session_with(EntityKind::LwPolyline(polyline), normal);
    let before = session.grips_with_world(h).unwrap();
    assert_eq!(before.len(), 5, "three vertices and two segment middles");
    // The vertex 1 grip (index 2) goes where the cursor is, carried onto the plane.
    let target = before[2].point + Vec2::new(1.5, -0.5);
    session.grip_edit(h, 2, target, GripMode::Stretch).unwrap();
    let after = session.grips_with_world(h).unwrap();
    assert!((after[2].point - target).len() < 1e-9, "the grip is where the cursor was");
    assert!((after[0].point - before[0].point).len() < 1e-9 && (after[4].point - before[4].point).len() < 1e-9, "the other vertices stay");
    let plane_point = after[2].world.unwrap() - before[0].world.unwrap();
    assert!(plane_point.dot(normal).abs() < 1e-9, "the vertex stays in the plane: {plane_point:?}");
    assert_eq!(session.doc().unwrap().entity(h).unwrap().common.extrusion, normal);
    // A segment middle moves both of its vertices by the same step.
    let middle = after[1].point;
    session.grip_edit(h, 1, middle + Vec2::new(1.0, 0.0), GripMode::Stretch).unwrap();
    let moved = session.grips_with_world(h).unwrap();
    assert!((moved[0].point - after[0].point - Vec2::new(1.0, 0.0)).len() < 1e-9, "{:?}", moved[0]);
    assert!((moved[2].point - after[2].point - Vec2::new(1.0, 0.0)).len() < 1e-9, "{:?}", moved[2]);
}

#[test]
fn a_spline_control_point_moves_in_xy_and_keeps_its_z() {
    use cadcraft_geom::{FitOptions, Spline, SplineSpace};
    let spline = Spline {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0), Vec2::new(6.0, 0.0)],
        weights: Vec::new(),
        fit: Vec::new(),
        closed: false,
        fit_opts: FitOptions::default(),
        space: Some(SplineSpace { control: vec![0.0, 2.5, 1.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 }),
    };
    let (mut session, h) = session_with(EntityKind::Spline(spline), Vec3::Z);
    session.grip_edit(h, 1, Vec2::new(3.0, 6.0), GripMode::Stretch).unwrap();
    let EntityKind::Spline(s) = kind(&session, h) else { panic!("spline") };
    assert_eq!(s.control[1], Vec2::new(3.0, 6.0));
    assert_eq!(s.space.unwrap().control, vec![0.0, 2.5, 1.0], "the Z values stay");
}

#[test]
fn a_legacy_circle_quadrant_still_stretches_its_two_d_radius() {
    let mut session = Session::new();
    let h = session.doc_mut().unwrap().new_handle();
    session.doc_mut().unwrap().model.push(Entity::new(h, EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 0.0), radius: 4.0 })));
    session.grip_edit(h, 1, Vec2::new(8.0, 2.0), GripMode::Stretch).unwrap();
    assert_eq!(kind(&session, h), EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 0.0), radius: 7.0 }));
}

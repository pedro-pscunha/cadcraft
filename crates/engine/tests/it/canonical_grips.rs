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
fn a_grip_that_would_stretch_the_two_d_reading_refuses_typed_and_changes_nothing() {
    let (mut session, h) = tilted_circle();
    let before = session.doc().unwrap().model.clone();
    let error = session.grip_edit(h, 1, Vec2::new(9.0, 9.0), GripMode::Stretch).unwrap_err();
    assert!(error.to_string().contains("canonical"), "{error}");
    assert_eq!(session.doc().unwrap().model, before);
    // A mirror is a reflection: a circle's canonical transform refuses it, the same way.
    let error = session.grip_edit(h, 0, Vec2::new(0.0, 9.0), GripMode::Mirror).unwrap_err();
    assert!(error.to_string().to_lowercase().contains("canonical") || error.to_string().contains("text"), "{error}");
    assert_eq!(session.doc().unwrap().model, before);
}

#[test]
fn a_spatial_line_has_end_and_middle_grips_with_their_z_and_its_middle_moves_it_keeping_z() {
    let (mut session, h) = session_with(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(10.0, 0.0, 5.0) }), Vec3::Z);
    let grips = session.grips_with_world(h).unwrap();
    let worlds: Vec<Vec3> = grips.iter().map(|g| g.world.unwrap()).collect();
    assert_eq!(worlds, [Vec3::new(0.0, 0.0, 1.0), Vec3::new(5.0, 0.0, 3.0), Vec3::new(10.0, 0.0, 5.0)]);
    // An end would stretch the line itself: that refuses until the drawing has a canonical edit for it.
    let error = session.grip_edit(h, 2, Vec2::new(12.0, 3.0), GripMode::Stretch).unwrap_err();
    assert!(error.to_string().contains("canonical"), "{error}");
    // The middle moves the whole line, both Z values kept.
    session.grip_edit(h, 1, Vec2::new(7.0, 1.5), GripMode::Stretch).unwrap();
    let EntityKind::Line(l) = kind(&session, h) else { panic!("line") };
    assert!(near(l.a, Vec3::new(2.0, 1.5, 1.0)) && near(l.b, Vec3::new(12.0, 1.5, 5.0)), "{l:?}");
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

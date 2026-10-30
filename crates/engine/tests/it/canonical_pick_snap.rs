//! Pick, selection and snap on canonical entities use their WCS geometry seen along Z, not the 2D reading of their fields:
//! a circle in a tilted plane is picked and snapped as the ellipse it projects to, a 3D line snaps with the Z it has, and a
//! text is picked on its words. A legacy entity keeps its 2D path.

use cadcraft_doc::{Circle, Common, Drawing, Entity, EntityKind, GeometryRevision, Line, Solid, Space};
use cadcraft_engine::select;
use cadcraft_engine::snap::{self, mode};
use cadcraft_geom::{Bounds2, Vec2, Vec3};

fn drawing(kind: EntityKind, normal: Vec3) -> (Drawing, cadcraft_doc::Handle) {
    let mut d = Drawing::new_metric();
    let handle = d.new_handle();
    let mut e = Entity::new(handle, kind);
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common { extrusion: normal, ..Common::default() };
    d.model.push(e);
    (d, handle)
}

/// A circle of radius 4 in the plane of normal (0, 3, 4): OCS X is world -X, OCS Y is (0, -0.8, 0.6), so it projects to an
/// ellipse with semi-axes 4 (along X) and 3.2 (along Y), centered at (-1, 1.4) with Z 5.2.
fn tilted_circle() -> (Drawing, cadcraft_doc::Handle) {
    drawing(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }), Vec3::new(0.0, 3.0, 4.0))
}

const CENTER: Vec2 = Vec2 { x: -1.0, y: 1.4 };

#[test]
fn a_tilted_circle_is_picked_on_its_projected_ellipse() {
    let (d, h) = tilted_circle();
    // The top of the ellipse is 3.2 above the center; the 2D reading of the fields (radius 4) would be at 4.
    assert_eq!(select::pick(&d, &Space::Model, CENTER + Vec2::new(0.0, 3.2), 0.1), Some(h));
    assert_eq!(select::pick(&d, &Space::Model, CENTER + Vec2::new(0.0, 4.0), 0.1), None, "the unprojected radius is not on it");
    assert_eq!(select::pick(&d, &Space::Model, CENTER + Vec2::new(4.0, 0.0), 0.1), Some(h));
}

#[test]
fn a_crossing_box_touches_the_projected_outline() {
    let (d, h) = tilted_circle();
    // A small box at the ellipse's top touches it; one at the unprojected radius does not.
    let around = |p: Vec2| Bounds2::new(p - Vec2::new(0.2, 0.2), p + Vec2::new(0.2, 0.2));
    assert_eq!(select::select_window(&d, &Space::Model, around(CENTER + Vec2::new(0.0, 3.2)), true), vec![h]);
    assert!(select::select_window(&d, &Space::Model, around(CENTER + Vec2::new(0.0, 4.0)), true).is_empty());
    // A box inside the ellipse touches nothing.
    assert!(select::select_window(&d, &Space::Model, around(CENTER), true).is_empty());
}

#[test]
fn a_tilted_circle_snaps_at_the_ellipse_with_the_wcs_point_it_has() {
    let (d, _) = tilted_circle();
    let near = |p: Vec2| p + Vec2::new(0.05, 0.03);
    let center = snap::osnap(&d, &Space::Model, near(CENTER), 0.3, mode::CEN, None, false).unwrap();
    assert!((center.point - CENTER).len() < 1e-9, "{:?}", center.point);
    let world = center.world.expect("the center has one WCS point");
    assert!((world - Vec3::new(-1.0, 1.4, 5.2)).len() < 1e-9, "{world:?}");
    let top = snap::osnap(&d, &Space::Model, near(CENTER + Vec2::new(0.0, 3.2)), 0.3, mode::QUA, None, false).unwrap();
    assert!((top.point - (CENTER + Vec2::new(0.0, 3.2))).len() < 1e-6, "{:?}", top.point);
    // The quadrant at the top of the view is the circle point (0, -0.8 * 4, 0.6 * 4) + the center's offset.
    let world = top.world.expect("a quadrant has one WCS point");
    assert!(
        (world - Vec3::new(-1.0, 1.4 + 3.2, 5.2 + 2.4)).len() < 1e-6 || (world - Vec3::new(-1.0, 1.4 + 3.2, 5.2 - 2.4)).len() < 1e-6,
        "{world:?}"
    );
}

#[test]
fn a_spatial_line_snaps_with_its_z_at_its_ends_middle_and_nearest_point() {
    let (d, _) = drawing(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 1.0), b: Vec3::new(10.0, 0.0, 5.0) }), Vec3::Z);
    let at = |p: Vec2, m: u32| snap::osnap(&d, &Space::Model, p, 0.5, m, None, false).unwrap();
    assert_eq!(at(Vec2::new(10.1, 0.1), mode::END).world, Some(Vec3::new(10.0, 0.0, 5.0)));
    assert_eq!(at(Vec2::new(5.1, 0.1), mode::MID).world, Some(Vec3::new(5.0, 0.0, 3.0)));
    let nearest = at(Vec2::new(4.0, 0.2), mode::NEA);
    let z = nearest.world.expect("nearest on a line").z;
    assert!((z - 2.6).abs() < 1e-9, "{z}");
}

#[test]
fn a_filled_solid_is_picked_inside() {
    let corners = [Vec3::new(0.0, 0.0, 2.0), Vec3::new(10.0, 0.0, 2.0), Vec3::new(0.0, 10.0, 2.0), Vec3::new(0.0, 10.0, 2.0)];
    let (d, h) = drawing(EntityKind::Solid(Solid { corners }), Vec3::Z);
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(2.0, 2.0), 0.1), Some(h), "inside the triangle");
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(8.0, 8.0), 0.1), None, "outside it, inside its bounding box");
}

#[test]
fn a_legacy_circle_keeps_its_2d_reading() {
    let mut d = Drawing::new_metric();
    let h = d.new_handle();
    d.model.push(Entity::new(h, EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 0.0), radius: 4.0 })));
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(1.0, 6.0), 0.1), Some(h));
    assert_eq!(snap::osnap(&d, &Space::Model, Vec2::new(1.05, 2.05), 0.3, mode::CEN, None, false).unwrap().world, None);
}

#[test]
fn a_canonical_mtext_is_picked_on_its_words_turned_by_its_full_direction() {
    let mtext = cadcraft_doc::MText {
        insert: Vec3::new(10.0, 10.0, 0.0),
        height: 2.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "HELLO".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(0.6, 0.8, 0.0)),
    };
    let (d, h) = drawing(EntityKind::MText(mtext), Vec3::Z);
    let along = Vec2::new(0.6, 0.8);
    let up = Vec2::new(-0.8, 0.6);
    let origin = Vec2::new(10.0, 10.0);
    // The text hangs below its top-left insertion point, along the direction.
    // The spatial index uses the entity bounds, which a canonical text only gets right once the bounds are canonical too, so
    // this tests the distance the pick measures.
    let e = d.entity(h).unwrap();
    assert_eq!(select::entity_distance(&d, e, origin + along * 3.0 - up * 1.0, 0.05), 0.0);
    assert!(select::entity_distance(&d, e, origin + along * 3.0 + up * 4.0, 0.05) > 1.0);
    // The Insertion snap is the WCS insertion point.
    let hit = snap::osnap(&d, &Space::Model, origin + Vec2::new(0.1, 0.05), 0.3, mode::INS, None, false).unwrap();
    assert!((hit.point - origin).len() < 1e-9);
}

#[test]
fn a_canonical_multileader_is_picked_on_its_lines_and_its_text() {
    use cadcraft_doc::{MLeader, MLeaderContext, MText};
    let text = MText {
        insert: Vec3::new(20.0, 10.0, 3.0),
        height: 2.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "NOTE".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(1.0, 0.0, 0.0)),
    };
    let mut m = MLeader::single(Vec3::new(18.0, 10.0, 3.0), 2.0, false, vec![vec![Vec3::new(0.0, 0.0, 3.0), Vec3::new(10.0, 5.0, 3.0)]], Some(text));
    m.context = MLeaderContext { geometry_revision: GeometryRevision::CanonicalV2, origin: Vec3::new(0.0, 0.0, 3.0), ..MLeaderContext::default() };
    let (d, h) = drawing(EntityKind::MLeader(m), Vec3::Z);
    // On the first segment of the line, on the text, and nowhere.
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(5.0, 2.5), 0.1), Some(h));
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(22.0, 9.0), 0.05), Some(h));
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(5.0, 9.0), 0.1), None);
    // The end of the line has its WCS Z.
    let hit = snap::osnap(&d, &Space::Model, Vec2::new(0.05, 0.05), 0.3, mode::END, None, false).unwrap();
    assert_eq!(hit.world, Some(Vec3::new(0.0, 0.0, 3.0)));
}

#[test]
fn a_canonical_entity_the_resolver_refuses_is_picked_on_its_bounds_and_never_snapped() {
    // A spline with no Z values has no spatial form: the resolver refuses it.
    let spline = cadcraft_geom::Spline::from_control(vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0), Vec2::new(6.0, 0.0)], 2);
    let (d, h) = drawing(EntityKind::Spline(spline), Vec3::Z);
    assert_eq!(select::pick(&d, &Space::Model, Vec2::new(0.0, 0.0), 0.1), Some(h), "a corner of the bounds");
    assert!(snap::osnap(&d, &Space::Model, Vec2::new(0.05, 0.05), 0.3, mode::END | mode::NEA, None, false).is_none());
}

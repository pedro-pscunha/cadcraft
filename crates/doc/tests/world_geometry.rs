use cadcraft_doc::{DocError, Entity, EntityKind, GeometryRevision, Handle, Line, Point, Prim, WorldPrimitive};
use cadcraft_geom::{Mat4, Segment, Vec2, Vec3};

fn line() -> Entity {
    let mut entity = Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 3.0), b: Vec3::new(4.0, 5.0, 6.0) }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity
}

#[test]
fn world_geometry_retains_xyz_until_projection_after_the_parent_transform() {
    let entity = line();
    let parent = Mat4 { m: [[0.0, -2.0, 0.0, 10.0], [1.0, 0.0, 0.0, 20.0], [0.0, 0.0, -3.0, 30.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity.world_geometry(&parent).unwrap();
    assert_eq!(geometry.primitives(), vec![WorldPrimitive::Line { a: Vec3::new(6.0, 21.0, 21.0), b: Vec3::new(0.0, 24.0, 12.0) }]);
    let projected = geometry.project_xy();
    let [Prim::Seg(Segment::Line(segment))] = projected.as_slice() else { panic!("projected line") };
    assert_eq!(segment.a, Vec2::new(6.0, 21.0));
    assert_eq!(segment.b, Vec2::new(0.0, 24.0));
}

#[test]
fn point_geometry_uses_the_same_spatial_parent_and_projection_contract() {
    let mut entity = Entity::new(Handle(0x101), EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    let mut parent = Mat4::IDENTITY;
    parent.m[0][2] = 2.0;
    let geometry = entity.world_geometry(&parent).unwrap();
    assert_eq!(geometry.primitives(), vec![WorldPrimitive::Point(Vec3::new(7.0, 2.0, 3.0))]);
    assert!(matches!(geometry.project_xy().as_slice(), [Prim::Point(p)] if *p == Vec2::new(7.0, 2.0)));
}

#[test]
fn legacy_payloads_do_not_silently_become_world_xy_geometry() {
    let mut entity = line();
    entity.geometry_revision = GeometryRevision::LegacyProjectedV1;
    let error = entity.world_geometry(&Mat4::IDENTITY).unwrap_err();
    assert!(error.to_string().contains("legacy"));
}

#[test]
fn projected_degeneracy_does_not_erase_spatial_geometry() {
    let mut entity = line();
    entity.kind = EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 3.0), b: Vec3::new(1.0, 2.0, 9.0) });
    let geometry = entity.world_geometry(&Mat4::IDENTITY).unwrap();
    assert!(matches!(geometry.primitives(), [WorldPrimitive::Line { a, b }] if a != b));
    assert!(matches!(geometry.project_xy().as_slice(), [Prim::Seg(Segment::Line(line))] if line.a == line.b));
}

#[test]
fn non_affine_parents_and_non_finite_results_refuse_with_entity_context() {
    let entity = line();
    let mut projective = Mat4::IDENTITY;
    projective.m[3][0] = 0.1;
    assert!(entity.world_geometry(&projective).is_err());
    let mut overflow = Mat4::IDENTITY;
    overflow.m[0][0] = f64::MAX;
    assert!(matches!(entity.world_geometry(&overflow), Err(DocError::NonFinite(Handle(0x100)))));
}

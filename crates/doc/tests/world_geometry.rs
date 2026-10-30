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
    let projected = geometry.project_xy().unwrap();
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
    assert!(matches!(geometry.project_xy().unwrap().as_slice(), [Prim::Point(p)] if *p == Vec2::new(7.0, 2.0)));
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
    assert!(matches!(geometry.project_xy().unwrap().as_slice(), [Prim::Seg(Segment::Line(line))] if line.a == line.b));
}

#[test]
fn canonical_circles_use_ocs_before_the_full_spatial_parent() {
    let mut entity = Entity::new(Handle(0x102), EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    let geometry = entity.world_geometry(&Mat4::IDENTITY).unwrap();
    let [WorldPrimitive::Conic { center, axis_u, axis_v, .. }] = geometry.primitives() else { panic!("world conic") };
    assert_eq!(*center, Vec3::new(-1.0, 3.0, 2.0));
    assert_eq!(*axis_u, Vec3::new(-2.0, 0.0, 0.0));
    assert_eq!(*axis_v, Vec3::new(0.0, 0.0, 2.0));
    assert_ne!(*axis_u, Vec3::ZERO);
    assert_ne!(*axis_v, Vec3::ZERO);
    let projected = geometry.project_xy().unwrap();
    let [Prim::Seg(Segment::Line(segment))] = projected.as_slice() else { panic!("edge-on segment") };
    assert_eq!(segment.a.y, 3.0);
    assert_eq!(segment.b.y, 3.0);
    assert_eq!(segment.a.x.min(segment.b.x), -3.0);
    assert_eq!(segment.a.x.max(segment.b.x), 1.0);
}

#[test]
fn canonical_ellipses_use_the_common_plane_and_keep_wcs_major_components() {
    let mut entity = Entity::new(
        Handle(0x103),
        EntityKind::Ellipse(cadcraft_doc::Ellipse {
            center: Vec3::new(1.0, 2.0, 3.0),
            major: Vec3::new(2.0, 0.0, 0.0),
            normal: Vec3::Z,
            ratio: 0.5,
            start: 0.2,
            end: 1.4,
        }),
    );
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    let before = entity.clone();
    let geometry = entity.world_geometry(&Mat4::IDENTITY).unwrap();
    let [WorldPrimitive::Conic { center, axis_u, axis_v, start, end }] = geometry.primitives() else { panic!("world conic") };
    assert_eq!(*center, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(*axis_u, Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(*axis_v, Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(start.to_bits(), 0.2_f64.to_bits());
    assert_eq!(end.to_bits(), 1.4_f64.to_bits());
    assert_eq!(entity, before);
}

#[test]
fn partial_arc_parameters_survive_ocs_and_parent_composition() {
    let mut entity =
        Entity::new(Handle(0x104), EntityKind::Arc(cadcraft_doc::Arc { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0, start: 0.2, end: 1.4 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    let parent = Mat4 { m: [[1.0, 0.0, 0.0, 10.0], [0.0, 0.0, -1.0, 20.0], [0.0, 1.0, 0.0, 30.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity.world_geometry(&parent).unwrap();
    let [WorldPrimitive::Conic { center, axis_u, axis_v, start, end }] = geometry.primitives() else { panic!("world arc") };
    assert_eq!(*center, Vec3::new(9.0, 18.0, 33.0));
    assert_eq!(*axis_u, Vec3::new(-2.0, 0.0, 0.0));
    assert_eq!(*axis_v, Vec3::new(0.0, -2.0, 0.0));
    assert_eq!(start.to_bits(), 0.2_f64.to_bits());
    assert_eq!(end.to_bits(), 1.4_f64.to_bits());
    let projected = geometry.project_xy().unwrap();
    let [Prim::Ellipse(ellipse)] = projected.as_slice() else { panic!("projected arc") };
    for (parameter, actual) in [(0.2_f64, ellipse.at_param(ellipse.start)), (1.4_f64, ellipse.at_param(ellipse.end))] {
        let expected = (*center + *axis_u * parameter.cos() + *axis_v * parameter.sin()).xy();
        assert!((actual - expected).len() < 1e-12);
    }
}

#[test]
fn zero_conic_projection_is_a_point_and_projection_overflow_is_explicit() {
    let mut entity = Entity::new(Handle(0x105), EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::ZERO, radius: 1.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    let collapsed = Mat4 { m: [[0.0, 0.0, 0.0, 2.0], [0.0, 0.0, 0.0, 3.0], [1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity.world_geometry(&collapsed).unwrap();
    assert!(matches!(geometry.project_xy().unwrap().as_slice(), [Prim::Point(point)] if *point == Vec2::new(2.0, 3.0)));
    let EntityKind::Circle(circle) = &mut entity.kind else { panic!("circle") };
    circle.radius = 1.7e308;
    let huge = Mat4 { m: [[1.0, 1.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity.world_geometry(&huge).unwrap();
    assert!(matches!(geometry.project_xy(), Err(DocError::NonFinite(Handle(0x105)))));
}

#[test]
fn an_edge_on_partial_conic_keeps_interior_parameter_extrema() {
    let mut entity = Entity::new(Handle(0x106), EntityKind::Arc(cadcraft_doc::Arc { center: Vec3::ZERO, radius: 2.0, start: -0.5, end: 0.5 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    let projected = entity.world_geometry(&Mat4::IDENTITY).unwrap().project_xy().unwrap();
    let [Prim::Seg(Segment::Line(segment))] = projected.as_slice() else { panic!("partial segment") };
    assert!((segment.a.x.min(segment.b.x) + 2.0).abs() < 1e-12);
    assert!((segment.a.x.max(segment.b.x) + 2.0 * 0.5_f64.cos()).abs() < 1e-12);
    assert_eq!(segment.a.y, 0.0);
    assert_eq!(segment.b.y, 0.0);
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

use cadcraft_doc::{Circle, Common, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line, Space};
use cadcraft_geom::{Bounds2, Vec2, Vec3};
use cadcraft_render::{entity_extents, space_extents};

#[test]
fn entity_extents_dispatch_by_revision_without_a_projected_fallback() {
    let mut drawing = Drawing::new_metric();
    let mut entity = Entity::new(Handle(0x200), EntityKind::Circle(Circle { center: Vec3::new(100.0, 0.0, 0.0), radius: 2.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 0.0, -1.0);
    let bounds = entity_extents(&drawing, &entity, 0).unwrap();
    assert_eq!(bounds, Bounds2::new(Vec2::new(-102.0, -2.0), Vec2::new(-98.0, 2.0)));
    entity.common.extrusion = Vec3::ZERO;
    let refusal = entity_extents(&drawing, &entity, 0).unwrap_err();
    assert!(refusal.bounds.is_empty());
    assert_eq!(refusal.refusals.first().unwrap().handle, entity.handle);
    drawing.model.push(entity);
}

#[test]
fn space_extents_return_available_bounds_and_every_typed_refusal() {
    let mut drawing = Drawing::new_metric();
    drawing.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 0.0), b: Vec3::new(3.0, 4.0, 0.0) })).unwrap();
    for handle in [0x210, 0x211] {
        let mut entity = Entity::new(Handle(handle), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }));
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common.extrusion = Vec3::ZERO;
        drawing.model.push(entity);
    }
    let refusal = space_extents(&drawing, &Space::Model).unwrap_err();
    assert_eq!(refusal.bounds, Bounds2::new(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)));
    assert_eq!(refusal.refusals.iter().map(|r| r.handle).collect::<Vec<_>>(), vec![Handle(0x210), Handle(0x211)]);
}

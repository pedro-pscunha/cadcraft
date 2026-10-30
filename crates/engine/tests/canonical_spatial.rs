use std::sync::Arc;

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Text};
use cadcraft_engine::spatial::{SpatialIndex, index_store};
use cadcraft_geom::{Bounds2, Vec2, Vec3};

fn canonical(kind: EntityKind, normal: Vec3) -> Entity {
    let mut entity = Entity::new(Handle(0x190), kind);
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = normal;
    entity
}

#[test]
fn spatial_bounds_index_canonical_world_xy_instead_of_source_ocs_xy() {
    let mut drawing = Drawing::new_metric();
    drawing.model.push(canonical(EntityKind::Circle(Circle { center: Vec3::new(100.0, 0.0, 0.0), radius: 2.0 }), Vec3::new(0.0, 0.0, -1.0)));
    let index = SpatialIndex::build(&drawing, &drawing.model);
    assert_eq!(index.bounds(0), Bounds2::new(Vec2::new(-102.0, -2.0), Vec2::new(-98.0, 2.0)));
    let candidates = index.query(&Bounds2::new(Vec2::new(-103.0, -3.0), Vec2::new(-97.0, 3.0)), false).unwrap();
    assert_eq!(candidates, vec![0]);
}

#[test]
fn refused_world_bounds_remain_candidates_and_retain_their_reason() {
    let mut drawing = Drawing::new_metric();
    drawing.model.push(canonical(EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }), Vec3::ZERO));
    let index = SpatialIndex::build(&drawing, &drawing.model);
    assert_eq!(index.geometry_refusals().count(), 1);
    let candidates = index.query(&Bounds2::new(Vec2::new(100.0, 100.0), Vec2::new(101.0, 101.0)), false).unwrap();
    assert_eq!(candidates, vec![0], "a failed bound must not silently cull the object");
}

#[test]
fn canonical_glyph_bounds_invalidate_the_cache_when_the_text_style_changes() {
    let mut drawing = Drawing::new_metric();
    drawing.model.push(canonical(
        EntityKind::Text(Text {
            insert: Vec3::ZERO,
            align_pt: None,
            height: 1.0,
            value: "A".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
            backward: false,
            upside_down: false,
        }),
        Vec3::Z,
    ));
    let first = index_store(&drawing, &drawing.model);
    drawing.text_styles.first_mut().unwrap().backwards = true;
    let next = index_store(&drawing, &drawing.model);
    assert!(!Arc::ptr_eq(&first, &next));
    assert_ne!(first.bounds(0), next.bounds(0));
}

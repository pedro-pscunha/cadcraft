use std::sync::Arc;

use cadcraft_doc::{Block, DocError, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Line, Space, legacy_entity_bounds};
use cadcraft_geom::Vec3;

fn canonical_line() -> Entity {
    let mut entity = Entity::new(Handle(0x200), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(2.0, 3.0, 4.0) }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity
}

#[test]
fn legacy_queries_refuse_a_canonical_root_with_its_handle() {
    let mut drawing = Drawing::new_metric();
    let entity = canonical_line();
    assert!(matches!(legacy_entity_bounds(&drawing, &entity, 0), Err(DocError::CanonicalExtent(Handle(0x200)))));
    drawing.model.push(entity);
    assert!(matches!(drawing.legacy_extents(&Space::Model), Err(DocError::CanonicalExtent(Handle(0x200)))));
}

#[test]
fn a_legacy_insert_cannot_hide_a_canonical_descendant() {
    let mut drawing = Drawing::new_metric();
    let mut block = Block::new("B");
    block.entities.push(canonical_line());
    drawing.blocks.insert("B".into(), Arc::new(block));
    let entity = Entity::new(
        Handle(0x201),
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    );
    assert!(matches!(legacy_entity_bounds(&drawing, &entity, 0), Err(DocError::CanonicalExtent(Handle(0x200)))));
    drawing.model.push(entity);
    assert!(matches!(drawing.legacy_extents(&Space::Model), Err(DocError::CanonicalExtent(Handle(0x200)))));
}

use std::cell::Cell;
use std::sync::Arc;

use cadcraft_doc::{Block, DocError, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line};
use cadcraft_geom::Vec3;

fn line(handle: Handle, revision: GeometryRevision) -> Entity {
    let mut entity = Entity::new(handle, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5.0, 0.0, 3.0) }));
    entity.geometry_revision = revision;
    entity
}

#[test]
fn model_modification_refuses_canonical_data_before_the_callback() {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    drawing.model.push(line(handle, GeometryRevision::CanonicalV2));
    let before = drawing.model.clone();
    let called = Cell::new(false);
    assert!(
        drawing
            .modify_entity(handle, |entity| {
                called.set(true);
                entity.common.thickness = 2.0;
            })
            .is_err()
    );
    assert!(!called.get());
    assert_eq!(drawing.model, before);
}

#[test]
fn block_modification_refuses_before_it_clones_the_block_or_calls_the_callback() {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    let mut block = Block::new("Canonical");
    block.entities.push(line(handle, GeometryRevision::CanonicalV2));
    let before = Arc::new(block);
    drawing.blocks.insert("Canonical".into(), before.clone());
    let called = Cell::new(false);
    assert!(
        drawing
            .modify_entity_including_blocks(handle, |entity| {
                called.set(true);
                entity.common.thickness = 2.0;
            })
            .is_err()
    );
    assert!(!called.get());
    assert!(Arc::ptr_eq(drawing.blocks.get("Canonical").unwrap(), &before));
}

#[test]
fn a_legacy_callback_cannot_change_the_record_identity() {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    drawing.model.push(line(handle, GeometryRevision::LegacyProjectedV1));
    let before = drawing.model.clone();
    assert!(drawing.modify_entity(handle, |entity| entity.handle = Handle(0xFFFF)).is_err());
    assert_eq!(drawing.model, before);
}

#[test]
fn raw_legacy_callbacks_use_the_same_finite_candidate_check() {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    drawing.model.push(line(handle, GeometryRevision::LegacyProjectedV1));
    let before = drawing.model.clone();
    let result = drawing.modify_entity(handle, |entity| entity.common.thickness = f64::INFINITY);
    assert!(matches!(result, Err(DocError::NonFinite(got)) if got == handle));
    assert_eq!(drawing.model, before);
}

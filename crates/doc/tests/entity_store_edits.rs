use std::cell::Cell;

use cadcraft_doc::{Entity, EntityKind, EntityStore, GeometryRevision, Handle, Line};
use cadcraft_geom::Vec3;

fn store(revision: GeometryRevision) -> EntityStore {
    let mut store = EntityStore::new();
    let mut entity = Entity::new(Handle(1), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5.0, 0.0, 3.0) }));
    entity.geometry_revision = revision;
    store.push(entity);
    store
}

#[test]
fn a_store_callback_refuses_canonical_geometry_before_access() {
    let mut store = store(GeometryRevision::CanonicalV2);
    let before = store.clone();
    let called = Cell::new(false);
    assert!(
        store
            .modify(Handle(1), |entity| {
                called.set(true);
                entity.common.thickness = 2.0;
            })
            .is_err()
    );
    assert!(!called.get());
    assert_eq!(store, before);
}

#[test]
fn store_metadata_updates_do_not_enter_legacy_geometry_callbacks() {
    let mut store = store(GeometryRevision::CanonicalV2);
    let before = store.get(Handle(1)).unwrap().clone();
    assert!(store.modify_metadata(Handle(1), |common| common.visible = false).unwrap());
    let after = store.get(Handle(1)).unwrap();
    assert_eq!(after.kind, before.kind);
    assert_eq!(after.geometry_revision, before.geometry_revision);
    assert!(!after.common.visible);
}

#[test]
fn a_store_callback_cannot_commit_non_finite_or_reidentified_data() {
    for identity in [false, true] {
        let mut store = store(GeometryRevision::LegacyProjectedV1);
        let before = store.clone();
        assert!(
            store
                .modify(Handle(1), |entity| {
                    if identity {
                        entity.handle = Handle(2);
                    } else {
                        entity.common.thickness = f64::INFINITY;
                    }
                })
                .is_err()
        );
        assert_eq!(store, before);
    }
}

#[test]
fn an_absent_record_does_not_call_either_edit_callback() {
    let mut store = store(GeometryRevision::LegacyProjectedV1);
    let called = Cell::new(false);
    assert!(!store.modify(Handle(9), |_| called.set(true)).unwrap());
    assert!(!store.modify_metadata(Handle(9), |_| called.set(true)).unwrap());
    assert!(!called.get());
}

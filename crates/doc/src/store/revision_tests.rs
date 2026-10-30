use super::*;
use crate::{EntityKind, GeometryRevision, Insert, Point};
use cadcraft_geom::Vec3;
use std::collections::HashSet;

fn point(h: u64, canonical: bool) -> Entity {
    let mut entity = Entity::new(Handle(h), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }));
    if canonical {
        entity.geometry_revision = GeometryRevision::CanonicalV2;
    }
    entity
}
fn insert(h: u64, name: &str) -> Entity {
    Entity::new(
        Handle(h),
        EntityKind::Insert(Insert {
            block: name.into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    )
}
fn check(store: &EntityStore) {
    let mut expected: HashSet<Handle> = store.iter().filter(|e| e.require_legacy_geometry().is_err()).map(|e| e.handle).collect();
    let mut shadow = store.clone();
    while let Some(handle) = shadow.first_non_legacy_geometry() {
        assert!(expected.remove(&handle));
        assert!(shadow.remove(handle).is_some());
    }
    assert!(expected.is_empty());
    let expected: HashSet<&str> = store
        .iter()
        .filter(|e| e.require_legacy_geometry().is_ok())
        .filter_map(|e| match &e.kind {
            EntityKind::Insert(insert) => Some(insert.block.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(store.referenced_blocks().collect::<HashSet<_>>(), expected);
}

#[test]
fn summaries_follow_every_mutation_and_keep_snapshots_independent() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = EntityStore::new();
    let same_shard = (3..1000).find(|&h| shard_of(Handle(h)) == shard_of(Handle(2))).ok_or("same shard handle")?;
    store.push(point(1, true));
    store.push_front(insert(2, "BODY"));
    store.push(insert(same_shard, "BODY"));
    check(&store);
    let snapshot = store.clone();
    assert!(store.remove(Handle(2)).is_some());
    check(&store);
    store.modify(Handle(same_shard), |entity| {
        if let EntityKind::Insert(insert) = &mut entity.kind {
            insert.block = "OTHER".into();
        }
    })?;
    check(&store);
    store.modify_metadata(Handle(1), |common| common.layer = "edited".into())?;
    check(&store);
    assert!(store.replace(point(1, false)));
    check(&store);
    store.push_front(point(1, true));
    check(&store);
    store.push(insert(2, "BODY"));
    check(&store);
    assert!(store.bring_to_front(Handle(1)));
    check(&store);
    assert!(store.send_to_back(Handle(1)));
    check(&store);
    assert!(store.move_next_to(Handle(1), Handle(2), true));
    check(&store);
    store.sort_by_key(|entity| entity.handle.0);
    check(&store);
    check(&snapshot);
    assert_eq!(snapshot.first_non_legacy_geometry(), Some(Handle(1)));
    assert!(snapshot.referenced_blocks().all(|name| name == "BODY"));
    Ok(())
}

#[test]
fn a_loaded_canonical_attribute_marks_its_legacy_insert_non_legacy() -> Result<(), Box<dyn std::error::Error>> {
    let attribute = serde_json::from_value(
        serde_json::json!({"tag":"A","geometryRevision":GeometryRevision::CanonicalV2,"text":{"insert":Vec3::ZERO,"height":1.0,"value":"value"}}),
    )?;
    let mut entity = insert(1, "BODY");
    if let EntityKind::Insert(insert) = &mut entity.kind {
        insert.attribs.push(attribute);
    }
    assert_eq!(entity.geometry_revision, GeometryRevision::LegacyProjectedV1);
    let mut store = EntityStore::new();
    store.push(entity);
    check(&store);
    assert_eq!(store.first_non_legacy_geometry(), Some(Handle(1)));
    assert!(store.replace(insert(1, "BODY")));
    check(&store);
    assert_eq!(store.first_non_legacy_geometry(), None);
    assert_eq!(store.referenced_blocks().collect::<Vec<_>>(), vec!["BODY"]);
    Ok(())
}

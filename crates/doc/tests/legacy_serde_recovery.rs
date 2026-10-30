use cadcraft_doc::{Entity, EntityKind, GeometryRevision, Handle, Leader, Promotion, promote_legacy_spline_leader};
use cadcraft_geom::{Mat4, Spline, Vec2, Vec3};

fn old_payload(kind: EntityKind, normal: Option<Vec3>) -> (Entity, serde_json::Value) {
    let mut original = Entity::new(Handle(0x104), kind);
    if let Some(normal) = normal {
        original.common.extrusion = normal;
    }
    let mut value = serde_json::to_value(&original).unwrap();
    value.as_object_mut().unwrap().remove("geometryRevision");
    if normal.is_none() {
        value["common"].as_object_mut().unwrap().remove("extrusion");
    }
    (original, value)
}

fn spline() -> EntityKind {
    EntityKind::Spline(Spline::from_control(vec![Vec2::new(-0.0, 1.25), Vec2::new(2.5, -3.75)], 1))
}

#[test]
fn deserialization_keeps_old_payloads_legacy_until_reader_activation() {
    for normal in [None, Some(Vec3::Z)] {
        let (original, value) = old_payload(spline(), normal);
        let loaded: Entity = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.geometry_revision, GeometryRevision::LegacyProjectedV1);
        assert_eq!(loaded, original);
    }
}

/// An old payload loaded, then promoted as reader activation will do.
fn promoted(value: serde_json::Value) -> Entity {
    let mut entity: Entity = serde_json::from_value(value).unwrap();
    assert_eq!(promote_legacy_spline_leader(&mut entity), Promotion::Promoted);
    entity
}

#[test]
fn serialized_plus_z_splines_recover_through_the_promotion() {
    for normal in [None, Some(Vec3::Z), Some(Vec3::new(0.0, 0.0, 2.0))] {
        let (original, value) = old_payload(spline(), normal);
        let recovered = promoted(value);
        assert_eq!(recovered.geometry_revision, GeometryRevision::CanonicalV2);
        assert_eq!(recovered.common, original.common);
        let (EntityKind::Spline(old), EntityKind::Spline(new)) = (&original.kind, &recovered.kind) else { panic!("not splines") };
        assert_eq!(
            new.control.iter().map(|p| [p.x.to_bits(), p.y.to_bits()]).collect::<Vec<_>>(),
            old.control.iter().map(|p| [p.x.to_bits(), p.y.to_bits()]).collect::<Vec<_>>()
        );
        assert_eq!(new.space.as_ref().unwrap().control, vec![0.0; old.control.len()]);
        let again: Entity = serde_json::from_value(serde_json::to_value(&recovered).unwrap()).unwrap();
        assert_eq!(again, recovered);
    }
}

#[test]
fn serialized_entity_collections_promote_leaders_without_changing_source_vertices() {
    let kind = EntityKind::Leader(Leader { vertices: vec![Vec3::new(-0.0, 1.25, 0.0), Vec3::new(2.5, -3.75, 0.0)], ..Leader::default() });
    let (original, value) = old_payload(kind, None);
    let loaded: Vec<Entity> = serde_json::from_value(serde_json::json!([value])).unwrap();
    let recovered = &promoted(serde_json::to_value(loaded.first().unwrap()).unwrap());
    assert_eq!(recovered.geometry_revision, GeometryRevision::CanonicalV2);
    let (EntityKind::Leader(old), EntityKind::Leader(new)) = (&original.kind, &recovered.kind) else { panic!("not leaders") };
    for (a, b) in old.vertices.iter().zip(&new.vertices) {
        assert_eq!([a.x.to_bits(), a.y.to_bits(), a.z.to_bits()], [b.x.to_bits(), b.y.to_bits(), b.z.to_bits()]);
    }
    assert!(new.frame.is_some());
}

#[test]
fn lossy_serialized_payloads_keep_the_legacy_representation_and_edit_rules() {
    for kind in [spline(), EntityKind::Leader(Leader { vertices: vec![Vec3::ZERO, Vec3::new(1.0, 2.0, 0.0)], ..Leader::default() })] {
        let (original, value) = old_payload(kind, Some(Vec3::new(0.0, 3.0, 4.0)));
        let mut recovered: Entity = serde_json::from_value(value).unwrap();
        assert_eq!(recovered, original);
        let spatial = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
        assert!(recovered.try_transform(&spatial).is_err());
        assert_eq!(recovered, original);
        let mut planar = Mat4::IDENTITY;
        planar.m[0][3] = 2.0;
        recovered.try_transform(&planar).unwrap();
        assert_ne!(recovered, original);
    }
}

#[test]
fn unknown_serialized_revisions_still_refuse_before_recovery() {
    let (_, mut value) = old_payload(spline(), None);
    value["geometryRevision"] = serde_json::json!("futureV9");
    assert!(serde_json::from_value::<Entity>(value).is_err());
}

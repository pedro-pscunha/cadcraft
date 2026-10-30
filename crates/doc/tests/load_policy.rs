//! Section 8 of the reader activation plan: `Deserialize` leaves an old payload Legacy and an explicit revision as it is; a load
//! promotes through `apply_load_policy` with the family mask of the caller, never through process state.

use cadcraft_doc::{
    Common, Entity, EntityKind, Family, FamilySet, GeometryRevision, Handle, Leader, LoadPolicy, Promotion, RawCompound, RawRecord, RawTag,
    apply_load_policy,
};
use cadcraft_geom::{Spline, Vec2, Vec3};

fn spline() -> EntityKind {
    EntityKind::Spline(Spline::from_control(vec![Vec2::new(-0.0, 1.25), Vec2::new(2.5, -3.75)], 1))
}

fn leader() -> EntityKind {
    EntityKind::Leader(Leader { vertices: vec![Vec3::new(-0.0, 1.25, 0.0), Vec3::new(2.5, -3.75, 0.0)], ..Leader::default() })
}

/// An old payload: no geometry revision, and no extrusion unless `normal` is given.
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

fn load(value: &serde_json::Value) -> Entity {
    serde_json::from_value(value.clone()).unwrap()
}

fn policy(families: &[Family]) -> LoadPolicy {
    LoadPolicy::new(FamilySet::of(families))
}

#[test]
fn a_old_payload_without_a_policy_stays_legacy_with_its_numbers() {
    for kind in [spline(), leader()] {
        let (original, value) = old_payload(kind, None);
        let loaded = load(&value);
        assert_eq!(loaded.geometry_revision, GeometryRevision::LegacyProjectedV1);
        assert_eq!(loaded, original);
    }
}

#[test]
fn b_a_policy_with_the_family_promotes_without_changing_a_number() {
    for (kind, family) in [(spline(), Family::Spline), (leader(), Family::Leader)] {
        let (original, value) = old_payload(kind, None);
        let mut entity = load(&value);
        assert_eq!(apply_load_policy(&mut entity, &policy(&[family])), Promotion::Promoted);
        assert_eq!(entity.geometry_revision, GeometryRevision::CanonicalV2);
        assert_eq!(entity.common, original.common);
        match (&original.kind, &entity.kind) {
            (EntityKind::Spline(old), EntityKind::Spline(new)) => {
                let bits = |s: &Spline| s.control.iter().map(|p| [p.x.to_bits(), p.y.to_bits()]).collect::<Vec<_>>();
                assert_eq!(bits(new), bits(old));
            }
            (EntityKind::Leader(old), EntityKind::Leader(new)) => {
                let bits = |l: &Leader| l.vertices.iter().map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).collect::<Vec<_>>();
                assert_eq!(bits(new), bits(old));
            }
            _ => panic!("the kind changed"),
        }
    }
}

#[test]
fn c_a_mask_without_the_family_leaves_it_legacy() {
    for (kind, other) in [(spline(), Family::Leader), (leader(), Family::Spline)] {
        let (original, value) = old_payload(kind, None);
        for mask in [policy(&[other]), LoadPolicy::new(FamilySet::NONE), policy(&[Family::Line, Family::Hatch])] {
            let mut entity = load(&value);
            assert_eq!(apply_load_policy(&mut entity, &mask), Promotion::NotApplicable);
            assert_eq!(entity.geometry_revision, GeometryRevision::LegacyProjectedV1);
            assert_eq!(entity, original);
        }
    }
}

#[test]
fn d_a_payload_on_another_plane_than_plus_z_is_kept_legacy() {
    for kind in [spline(), leader()] {
        let (original, value) = old_payload(kind, Some(Vec3::new(0.0, 3.0, 4.0)));
        let mut entity = load(&value);
        assert_eq!(apply_load_policy(&mut entity, &LoadPolicy::new(FamilySet::ALL)), Promotion::KeptLegacy);
        assert_eq!(entity.geometry_revision, GeometryRevision::LegacyProjectedV1);
        assert_eq!(entity, original);
    }
}

#[test]
fn e_an_explicit_canonical_revision_is_never_overridden() {
    for kind in [spline(), leader()] {
        let mut original = Entity::new(Handle(0x104), kind);
        original.geometry_revision = GeometryRevision::CanonicalV2;
        let mut entity = load(&serde_json::to_value(&original).unwrap());
        assert_eq!(entity.geometry_revision, GeometryRevision::CanonicalV2);
        assert_eq!(apply_load_policy(&mut entity, &LoadPolicy::new(FamilySet::ALL)), Promotion::NotApplicable);
        assert_eq!(entity, original);
    }
}

#[test]
fn f_a_bad_revision_string_is_an_error() {
    let (_, mut value) = old_payload(spline(), None);
    value["geometryRevision"] = serde_json::json!("futureV9");
    assert!(serde_json::from_value::<Entity>(value).is_err());
}

#[test]
fn a_raw_unit_round_trips_as_raw_and_no_policy_touches_it() {
    let tags = vec![RawTag { code: 5, value: "A0".into() }, RawTag { code: 330, value: "1F".into() }, RawTag { code: 10, value: "2".into() }];
    let unit = RawCompound::new(RawRecord::new("WIDGETX".into(), tags).unwrap(), Vec::new(), Common::default(), "No adapter.".into()).unwrap();
    let original = Entity::from_raw(unit);
    let mut loaded: Entity = serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap();
    assert_eq!(loaded, original);
    assert_eq!(loaded.geometry_revision, GeometryRevision::RawSource);
    for mask in [LoadPolicy::new(FamilySet::NONE), LoadPolicy::new(FamilySet::ALL)] {
        assert_eq!(apply_load_policy(&mut loaded, &mask), Promotion::NotApplicable);
        assert_eq!(loaded, original);
    }
}

#[test]
fn two_loads_with_different_masks_do_not_share_state() {
    let (_, value) = old_payload(spline(), None);
    let (mut with, mut without) = (load(&value), load(&value));
    assert_eq!(apply_load_policy(&mut with, &policy(&[Family::Spline])), Promotion::Promoted);
    assert_eq!(apply_load_policy(&mut without, &policy(&[Family::Leader])), Promotion::NotApplicable);
    assert_eq!(with.geometry_revision, GeometryRevision::CanonicalV2);
    assert_eq!(without.geometry_revision, GeometryRevision::LegacyProjectedV1);
}

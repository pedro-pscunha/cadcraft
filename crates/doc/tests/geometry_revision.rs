use cadcraft_doc::{Entity, EntityKind, GeometryRevision, Handle, Line};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

fn line() -> Entity {
    Entity::new(Handle(1), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) }))
}

#[test]
fn absent_payload_revisions_mean_legacy_projected_geometry() {
    let entity = line();
    let mut value = serde_json::to_value(&entity).unwrap();
    value.as_object_mut().unwrap().remove("geometryRevision");
    let restored: Entity = serde_json::from_value(value).unwrap();
    assert_eq!(restored.geometry_revision, GeometryRevision::LegacyProjectedV1);
}

#[test]
fn a_canonical_payload_never_uses_the_legacy_transform() {
    let mut entity = line();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    let before = entity.clone();
    let transform = Mat4::from_mat3(&Mat3::translate(Vec2::new(3.0, 4.0)));
    assert!(entity.try_transform(&transform).is_err());
    assert_eq!(entity, before);
}

#[test]
fn canonical_identity_keeps_all_payload_bits() {
    let mut entity = line();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    let before = entity.clone();
    entity.try_transform(&Mat4::IDENTITY).unwrap();
    assert_eq!(entity, before);
}

fn attribute() -> cadcraft_doc::Attrib {
    cadcraft_doc::Attrib::new(
        "TAG",
        cadcraft_doc::Text {
            insert: Vec3::new(1.0, 2.0, 3.0),
            align_pt: None,
            height: 1.0,
            value: "value".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        },
    )
}

#[test]
fn a_nested_attribute_checks_its_own_revision() {
    let mut attribute = attribute();
    attribute.geometry_revision = GeometryRevision::CanonicalV2;
    let before = attribute.clone();
    assert!(attribute.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(3.0, 4.0)))).is_err());
    assert_eq!(attribute, before);
}

#[test]
fn embedded_mtext_checks_its_revision_independently_of_the_attribute() {
    let mut attribute = attribute();
    attribute.multiline = Some(cadcraft_doc::MultilineAttrib {
        geometry_revision: GeometryRevision::CanonicalV2,
        mtext: cadcraft_doc::MText {
            insert: Vec3::new(4.0, 5.0, 6.0),
            height: 1.0,
            width: 2.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "value".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
        },
        read_as: "value".into(),
    });
    let before = attribute.clone();
    assert!(attribute.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(3.0, 4.0)))).is_err());
    assert_eq!(attribute, before);
}

#[test]
fn unknown_payload_revisions_refuse_deserialization() {
    let mut value = serde_json::to_value(line()).unwrap();
    value["geometryRevision"] = serde_json::json!("canonicalV99");
    assert!(serde_json::from_value::<Entity>(value).is_err());
}

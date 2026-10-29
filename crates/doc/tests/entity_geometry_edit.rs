use cadcraft_doc::{Common, Ellipse, Entity, EntityKind, Handle, Line};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

fn ellipse() -> Entity {
    Entity {
        geometry_revision: Default::default(),
        handle: Handle(0x100),
        common: Common { extrusion: Vec3::new(0.0, 1.0, 0.0), ..Default::default() },
        kind: EntityKind::Ellipse(Ellipse {
            center: Vec3::new(10.0, 20.0, 3.0),
            major: Vec3::new(5.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 1.0, 0.0),
            ratio: 0.4,
            start: 0.3,
            end: 2.4,
        }),
    }
}

#[test]
fn entity_dispatch_keeps_identity_and_translates_an_edge_on_plane() {
    let mut entity = ellipse();
    let before = entity.clone();
    entity.try_transform(&Mat4::IDENTITY).unwrap();
    assert_eq!(entity, before);
    entity.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(2.0, 3.0)))).unwrap();
    let EntityKind::Ellipse(after) = &entity.kind else { panic!("ellipse") };
    assert_eq!(after.center, Vec3::new(12.0, 23.0, 3.0));
    assert_eq!(after.normal, before.common.extrusion);
}

#[test]
fn entity_dispatch_returns_an_error_instead_of_a_silent_legacy_noop() {
    let mut entity = ellipse();
    let before = entity.clone();
    let error = entity.try_transform(&Mat4::from_mat3(&Mat3::rotate(0.5))).unwrap_err();
    assert!(error.to_string().contains("ellipse"));
    assert_eq!(entity, before);
}

#[test]
fn entity_dispatch_refuses_overflow_without_changing_any_fields() {
    let mut entity = Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::new(1e308, 0.0, 0.0), b: Vec3::new(1e308, 1.0, 0.0) }));
    let before = entity.clone();
    assert!(entity.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(1e308, 0.0)))).is_err());
    assert_eq!(entity, before);
}

#[test]
fn attribute_dispatch_updates_plain_and_embedded_payloads_atomically() {
    let text = cadcraft_doc::Text {
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
        backward: false,
        upside_down: false,
    };
    let mut attribute = cadcraft_doc::Attrib::new("TAG", text);
    attribute.multiline = Some(cadcraft_doc::MultilineAttrib {
        geometry_revision: Default::default(),
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
            direction: None,
        },
        read_as: "value".into(),
        normal: cadcraft_geom::Vec3::Z,
    });
    attribute.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(2.0, 3.0)))).unwrap();
    assert_eq!(attribute.text.insert, Vec3::new(3.0, 5.0, 3.0));
    assert_eq!(attribute.multiline.as_ref().unwrap().mtext.insert, Vec3::new(6.0, 8.0, 6.0));
    let before = attribute.clone();
    let mut invalid = Mat4::IDENTITY;
    invalid.m[0][0] = f64::NAN;
    assert!(attribute.try_transform(&invalid).is_err());
    assert_eq!(attribute, before);
}

#[test]
fn drawing_edits_discard_a_failed_candidate() {
    let mut drawing = cadcraft_doc::Drawing::new_metric();
    let handle =
        drawing.add(&cadcraft_doc::Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5.0, 0.0, 0.0) })).unwrap();
    let before = drawing.entity(handle).unwrap().as_ref().clone();
    let result: cadcraft_doc::Result<()> = drawing.try_modify_entity(handle, |entity| {
        entity.common.layer = "not committed".into();
        Err(cadcraft_doc::DocError::Invalid("rejected candidate".into()))
    });
    assert!(result.is_err());
    assert_eq!(drawing.entity(handle).unwrap().as_ref(), &before);
}

#[test]
fn an_unmigrated_payload_refuses_a_spatial_transform_atomically() {
    let mut entity = Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5.0, 1.0, 2.0) }));
    let before = entity.clone();
    let mut transform = Mat4::IDENTITY;
    transform.m[2][3] = 3.0;
    assert!(entity.try_transform(&transform).is_err());
    assert_eq!(entity, before);
}

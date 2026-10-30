use cadcraft_doc::{Entity, EntityKind, GeometryRevision, Handle, Line, MLeaderContext, MLeaderRoot};
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
    // A family with no canonical hook yet (a REGION); a LINE has one and moves in 3D.
    let mut entity = Entity::new(Handle(1), EntityKind::Region(cadcraft_doc::Region::default()));
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
            backward: false,
            upside_down: false,
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
            direction: None,
        },
        read_as: "value".into(),
        normal: cadcraft_geom::Vec3::Z,
    });
    let before = attribute.clone();
    assert!(attribute.try_transform(&Mat4::from_mat3(&Mat3::translate(Vec2::new(3.0, 4.0)))).is_err());
    assert_eq!(attribute, before);
}

#[test]
fn canonical_geometry_never_enters_an_untyped_candidate_callback() {
    let mut drawing = cadcraft_doc::Drawing::new_metric();
    let mut entity = line();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    let before = entity.clone();
    drawing.model.push(entity);
    let called = std::cell::Cell::new(false);
    let result = drawing.try_modify_entity(Handle(1), |entity| {
        called.set(true);
        entity.kind = EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::ZERO });
        Ok(())
    });
    assert!(result.is_err());
    assert!(!called.get());
    assert_eq!(drawing.entity(Handle(1)).unwrap().as_ref(), &before);
}

#[test]
fn a_canonical_child_blocks_the_legacy_parent_callback_before_it_runs() {
    let mut drawing = cadcraft_doc::Drawing::new_metric();
    let mut child = attribute();
    child.geometry_revision = GeometryRevision::CanonicalV2;
    let insert = cadcraft_doc::Insert {
        block: "Block".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![child.clone()],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let leader = cadcraft_doc::MLeader {
        roots: vec![MLeaderRoot::new(Vec3::ZERO, 0.0, false, Vec::new())],
        text: None,
        style: "Standard".into(),
        arrow_size: 1.0,
        spline: false,
        block: Some(insert.clone()),
        arrow_block: String::new(),
        context: MLeaderContext::default(),
    };
    let mut context_leader = leader.clone();
    context_leader.block = None;
    context_leader.context.geometry_revision = GeometryRevision::CanonicalV2;
    for kind in [EntityKind::AttDef(child), EntityKind::Insert(insert), EntityKind::MLeader(leader), EntityKind::MLeader(context_leader)] {
        let entity = Entity::new(Handle(1), kind);
        let before = entity.clone();
        drawing.model.push(entity);
        let called = std::cell::Cell::new(false);
        let result = drawing.try_modify_entity(Handle(1), |_| {
            called.set(true);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(drawing.entity(Handle(1)).unwrap().as_ref(), &before);
    }
}

#[test]
fn a_legacy_callback_cannot_commit_non_finite_geometry() {
    let mut entity = line();
    let before = entity.clone();
    let result = entity.try_legacy_edit(|entity| {
        entity.common.thickness = f64::INFINITY;
        Ok(())
    });
    assert!(matches!(result, Err(cadcraft_doc::DocError::NonFinite(Handle(1)))));
    assert_eq!(entity, before);
}

#[test]
fn a_legacy_callback_cannot_relabel_its_result_as_canonical() {
    let mut entity = line();
    let before = entity.clone();
    assert!(
        entity
            .try_legacy_edit(|entity| {
                entity.geometry_revision = GeometryRevision::CanonicalV2;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(entity, before);
}

#[test]
fn non_finite_nested_text_keeps_the_owning_entity_handle() {
    let mut failures = Vec::new();
    for embedded in [false, true] {
        let mut attribute = attribute();
        if embedded {
            attribute.multiline = Some(cadcraft_doc::MultilineAttrib {
                geometry_revision: GeometryRevision::LegacyProjectedV1,
                mtext: cadcraft_doc::MText {
                    insert: Vec3::ZERO,
                    height: 1e308,
                    width: 1.0,
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
        } else {
            attribute.text.height = 1e308;
        }
        let mut entity = Entity::new(
            Handle(0xABC),
            EntityKind::Insert(cadcraft_doc::Insert {
                block: "Block".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: vec![attribute],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        );
        let before = entity.clone();
        let result = entity.try_transform(&Mat4::from_mat3(&Mat3::scale(2.0, 2.0)));
        if !matches!(result, Err(cadcraft_doc::DocError::NonFinite(Handle(0xABC)))) {
            failures.push((embedded, result));
        }
        assert_eq!(entity, before);
    }
    assert!(failures.is_empty(), "nested text lost its owning handle: {failures:?}");
}

#[test]
fn unknown_payload_revisions_refuse_deserialization() {
    let mut value = serde_json::to_value(line()).unwrap();
    value["geometryRevision"] = serde_json::json!("canonicalV99");
    assert!(serde_json::from_value::<Entity>(value).is_err());
}

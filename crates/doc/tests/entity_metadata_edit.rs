use std::sync::Arc;

use cadcraft_doc::{Common, DocError, Drawing, Entity, EntityKind, GeometryRevision, GeometrySnapshot, Handle, Line, geometry_key};
use cadcraft_geom::Vec3;

fn drawing() -> (Drawing, Handle) {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    let mut entity = Entity::new(handle, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(5.0, 0.0, 3.0) }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.geometry = Some(Arc::new(GeometrySnapshot { records: Vec::new(), model: geometry_key(&entity.kind, &entity.common).unwrap() }));
    drawing.model.push(entity);
    (drawing, handle)
}

#[test]
fn metadata_edits_keep_the_payload_revision_plane_and_snapshot_owner() {
    let (mut drawing, handle) = drawing();
    let before = drawing.entity(handle).unwrap().clone();
    let result = drawing
        .try_modify_metadata(handle, |common| {
            common.color = cadcraft_doc::color::Color::Index(3);
            common.visible = false;
            Ok(17)
        })
        .unwrap();
    assert_eq!(result, 17);
    let after = drawing.entity(handle).unwrap();
    assert_eq!(after.kind, before.kind);
    assert_eq!(after.geometry_revision, before.geometry_revision);
    assert_eq!(after.common.extrusion, before.common.extrusion);
    assert_eq!(after.common.thickness.to_bits(), before.common.thickness.to_bits());
    assert!(Arc::ptr_eq(after.common.geometry.as_ref().unwrap(), before.common.geometry.as_ref().unwrap()));
    assert_eq!(after.common.color, cadcraft_doc::color::Color::Index(3));
    assert!(!after.common.visible);
}

#[test]
fn metadata_edits_cannot_change_plane_or_thickness_bits() {
    let edits: [fn(&mut Common); 5] = [
        |common| common.extrusion.x = 0.5,
        |common| common.extrusion.z = -1.0,
        |common| common.extrusion.x = -0.0,
        |common| common.thickness = 2.0,
        |common| common.thickness = -0.0,
    ];
    for edit in edits {
        let (mut drawing, handle) = drawing();
        let before = drawing.model.clone();
        assert!(
            drawing
                .try_modify_metadata(handle, |common| {
                    edit(common);
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(drawing.model, before);
    }
}

#[test]
fn metadata_edits_cannot_discard_or_replace_a_snapshot_owner() {
    for replace in [false, true] {
        let (mut drawing, handle) = drawing();
        let before = drawing.entity(handle).unwrap().clone();
        assert!(
            drawing
                .try_modify_metadata(handle, |common| {
                    common.geometry = if replace { Some(Arc::new(common.geometry.as_ref().unwrap().as_ref().clone())) } else { None };
                    Ok(())
                })
                .is_err()
        );
        let after = drawing.entity(handle).unwrap();
        assert_eq!(after, &before);
        assert!(Arc::ptr_eq(after.common.geometry.as_ref().unwrap(), before.common.geometry.as_ref().unwrap()));
    }
}

#[test]
fn metadata_edits_do_not_require_a_source_snapshot() {
    let mut drawing = Drawing::new_metric();
    let handle = drawing.new_handle();
    let mut entity = Entity::new(handle, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::Z }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    drawing.model.push(entity);
    drawing
        .try_modify_metadata(handle, |common| {
            common.visible = false;
            Ok(())
        })
        .unwrap();
    assert!(drawing.entity(handle).unwrap().common.geometry.is_none());
    assert!(!drawing.entity(handle).unwrap().common.visible);
}

#[test]
fn non_finite_metadata_uses_the_owning_entity_handle() {
    let (mut drawing, handle) = drawing();
    let before = drawing.model.clone();
    let result = drawing.try_modify_metadata(handle, |common| {
        common.ltscale = f64::INFINITY;
        Ok(())
    });
    assert!(matches!(result, Err(DocError::NonFinite(got)) if got == handle));
    assert_eq!(drawing.model, before);
}

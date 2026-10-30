use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Space, Text};
use cadcraft_geom::{Mat4, Vec2, Vec3};
use cadcraft_render::{Options, build, entity_world_bounds};

fn canonical(kind: EntityKind, normal: Vec3) -> Entity {
    let mut entity = Entity::new(Handle(0x190), kind);
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = normal;
    entity
}

#[test]
fn world_bounds_measure_the_completed_circle_projection() {
    let drawing = Drawing::new_metric();
    let entity = canonical(EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 }), Vec3::new(0.0, 1.0, 0.0));
    let bounds = entity_world_bounds(&drawing, &entity, &Mat4::IDENTITY).unwrap();
    assert_eq!(bounds.min, Vec2::new(-3.0, 3.0));
    assert_eq!(bounds.max, Vec2::new(1.0, 3.0));
}

#[test]
fn world_text_bounds_match_the_glyph_geometry_that_the_canvas_draws() {
    let mut drawing = Drawing::new_metric();
    let entity = canonical(
        EntityKind::Text(Text {
            insert: Vec3::new(1.0, 2.0, 3.0),
            align_pt: None,
            height: 1.0,
            value: "A".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
            backward: false,
            upside_down: false,
        }),
        Vec3::new(0.0, 1.0, 0.0),
    );
    drawing.model.push(entity.clone());
    let bounds = entity_world_bounds(&drawing, &entity, &Mat4::IDENTITY).unwrap();
    let list = build(&drawing, &Space::Model, &Options::default());
    list.require_complete_geometry().unwrap();
    assert!(!bounds.is_empty());
    assert_eq!((bounds.min.y, bounds.max.y), (3.0, 3.0));
    assert_eq!(bounds, list.bounds);
}

fn viewport() -> cadcraft_doc::Viewport {
    cadcraft_doc::Viewport {
        center: Vec3::new(5.0, 5.0, 0.0),
        width: 10.0,
        height: 10.0,
        view_center: Vec2::new(-100.0, 0.0),
        view_height: 10.0,
        id: 2,
        locked: false,
        frozen_layers: Vec::new(),
        layer_colors: Vec::new(),
        clip: None,
        on: true,
        source_status: None,
    }
}

#[test]
fn viewport_prefilter_uses_world_bounds_before_the_paint_refusal_path() {
    let mut drawing = Drawing::new_metric();
    let mut outside = canonical(EntityKind::Circle(Circle { center: Vec3::new(100.0, 0.0, 0.0), radius: 2.0 }), Vec3::Z);
    outside.common.thickness = 1.0;
    drawing.model.push(outside);
    let paper = Space::Paper("Layout1".into());
    drawing.add(&paper, cadcraft_doc::Common::default(), EntityKind::Viewport(viewport())).unwrap();
    let list = build(&drawing, &paper, &Options::default());
    assert!(list.geometry_refusals.is_empty(), "geometry outside the viewport does not enter its paint path");
}

#[test]
fn viewport_prefilter_keeps_geometry_whose_ocs_xy_is_far_from_its_world_xy() {
    let mut drawing = Drawing::new_metric();
    drawing.model.push(canonical(EntityKind::Circle(Circle { center: Vec3::new(100.0, 0.0, 0.0), radius: 2.0 }), Vec3::new(0.0, 0.0, -1.0)));
    let paper = Space::Paper("Layout1".into());
    drawing.add(&paper, cadcraft_doc::Common::default(), EntityKind::Viewport(viewport())).unwrap();
    let list = build(&drawing, &paper, &Options::default());
    list.require_complete_geometry().unwrap();
    assert!(list.prims.iter().any(|primitive| primitive.handle == cadcraft_render::VIEWPORT_CONTENT));
}

#[test]
fn world_bounds_refuse_legacy_or_invalid_plane_payloads() {
    let drawing = Drawing::new_metric();
    let mut entity = canonical(EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }), Vec3::ZERO);
    assert!(entity_world_bounds(&drawing, &entity, &Mat4::IDENTITY).is_err());
    entity.geometry_revision = GeometryRevision::LegacyProjectedV1;
    assert!(entity_world_bounds(&drawing, &entity, &Mat4::IDENTITY).is_err());
}

use std::sync::Arc;

use cadcraft_doc::{Block, Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Line, Space, Text};
use cadcraft_geom::Vec3;
use cadcraft_render::{Options, build};

fn circle(normal: Vec3) -> Drawing {
    let mut drawing = Drawing::new_metric();
    let mut entity = Entity::new(Handle(0x180), EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 3.0), radius: 2.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = normal;
    drawing.model.push(entity);
    drawing
}

#[test]
fn a_canonical_circle_displays_its_world_projection_not_the_source_ocs_xy() {
    let list = build(&circle(Vec3::new(0.0, 1.0, 0.0)), &Space::Model, &Options::default());
    assert!(!list.prims.is_empty());
    assert!((list.bounds.min.x + 3.0).abs() < 1e-10 && (list.bounds.max.x - 1.0).abs() < 1e-10);
    assert_eq!((list.bounds.min.y, list.bounds.max.y), (3.0, 3.0));
}

#[test]
fn nonfinite_canonical_output_never_reaches_the_display_buffers() {
    let mut drawing = circle(Vec3::Z);
    let mut entity = drawing.model.iter().next().unwrap().as_ref().clone();
    entity.kind = EntityKind::Circle(Circle { center: Vec3::new(1e308, 0.0, 0.0), radius: 1e308 });
    drawing.model.push(entity);
    let list = build(&drawing, &Space::Model, &Options::default());
    assert!(list.prims.is_empty() && list.verts.is_empty());
    assert_eq!(list.geometry_refusals.len(), 1);
}

#[test]
fn a_thickness_is_displayed_as_an_extrusion_not_as_zero_thickness() {
    let mut drawing = circle(Vec3::new(0.0, 1.0, 0.0));
    let mut entity = drawing.model.iter().next().unwrap().as_ref().clone();
    entity.common.thickness = 1.0;
    drawing.model.push(entity);
    let list = build(&drawing, &Space::Model, &Options::default());
    // The circle, its far copy and the four side edges.
    assert_eq!(list.prims.len(), 6);
    assert!(list.require_complete_geometry().is_ok());
}

fn block_drawing(child: Entity) -> Drawing {
    let mut drawing = Drawing::new_metric();
    let mut block = Block::new("B");
    block.entities.push(child);
    drawing.blocks.insert("B".into(), Arc::new(block));
    let mut root = Entity::new(
        Handle(0x190),
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    );
    root.geometry_revision = GeometryRevision::CanonicalV2;
    root.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    drawing.model.push(root);
    drawing
}

#[test]
fn canonical_block_display_composes_spatial_parents_before_projection() {
    let mut child = Entity::new(Handle(0x191), EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 3.0), b: Vec3::new(4.0, 5.0, 6.0) }));
    child.geometry_revision = GeometryRevision::CanonicalV2;
    let list = build(&block_drawing(child), &Space::Model, &Options::default());
    list.require_complete_geometry().unwrap();
    assert_eq!(list.verts, vec![cadcraft_geom::Vec2::new(-1.0, 3.0), cadcraft_geom::Vec2::new(-4.0, 6.0)]);
}

#[test]
fn a_refused_block_does_not_leave_its_earlier_child_on_screen() {
    let bad = circle(Vec3::ZERO).model.iter().next().unwrap().as_ref().clone();
    let mut drawing = block_drawing(bad);
    let mut good = Entity::new(Handle(0x191), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) }));
    good.geometry_revision = GeometryRevision::CanonicalV2;
    let block = Arc::make_mut(drawing.blocks.get_mut("B").unwrap());
    let bad = block.entities.iter().next().unwrap().as_ref().clone();
    block.entities = Default::default();
    block.entities.push(good);
    block.entities.push(bad);
    let original = drawing.clone();
    let list = build(&drawing, &Space::Model, &Options::default());
    assert!(list.prims.is_empty() && list.verts.is_empty());
    assert_eq!(list.geometry_refusals.first().unwrap().handle, Handle(0x190));
    assert_eq!(drawing, original);
}

#[test]
fn canonical_text_uses_the_world_glyph_frame_once() {
    let mut drawing = Drawing::new_metric();
    let mut entity = Entity::new(
        Handle(0x192),
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
    );
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    drawing.model.push(entity);
    let list = build(&drawing, &Space::Model, &Options::default());
    list.require_complete_geometry().unwrap();
    assert!(!list.prims.is_empty());
    assert_eq!((list.bounds.min.y, list.bounds.max.y), (3.0, 3.0));
}

#[test]
fn an_invalid_canonical_plane_reports_a_refusal_without_legacy_fallback() {
    let list = build(&circle(Vec3::ZERO), &Space::Model, &Options::default());
    assert!(list.prims.is_empty());
    assert_eq!(list.geometry_refusals.len(), 1);
    assert_eq!(list.geometry_refusals.first().unwrap().handle, Handle(0x180));
    assert!(list.require_complete_geometry().is_err());
}

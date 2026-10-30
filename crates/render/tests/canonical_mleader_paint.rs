use std::sync::Arc;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{Attrib, Block, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Line, MLeader, Space, Text, WorldPrimitive};
use cadcraft_geom::{Mat4, Vec2, Vec3};
use cadcraft_render::{Options, build, entity_world_geometry};

fn line(handle: u64, a: Vec3, b: Vec3, color: Color) -> Entity {
    let mut entity = Entity::new(Handle(handle), EntityKind::Line(Line { a, b }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.color = color;
    entity
}

fn drawing(attributes: Vec<Attrib>, mode: i64) -> Drawing {
    let mut drawing = Drawing::new_metric();
    drawing.header.set_i64("ATTMODE", mode);
    let mut block = Block::new("B");
    block.base = Vec3::new(1.0, 2.0, 3.0);
    block.entities.push(line(0x201, Vec3::new(2.0, 4.0, 6.0), Vec3::new(3.0, 5.0, 7.0), Color::ByBlock));
    block.entities.push(line(0x202, Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0), Color::Index(3)));
    drawing.blocks.insert("B".into(), Arc::new(block));
    let mut leader = MLeader::single(Vec3::ZERO, 0.0, false, Vec::new(), None);
    leader.context.geometry_revision = GeometryRevision::CanonicalV2;
    leader.context.block_normal = Vec3::new(0.0, 1.0, 0.0);
    leader.block = Some(Insert {
        block: "B".into(),
        insert: Vec3::new(10.0, 20.0, 30.0),
        scale: Vec3::new(2.0, 3.0, 4.0),
        rotation: 0.0,
        attribs: attributes,
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    });
    let mut entity = Entity::new(Handle(0x200), EntityKind::MLeader(leader));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.color = Color::Index(1);
    drawing.model.push(entity);
    drawing
}

#[test]
fn mleader_block_content_keeps_the_complete_placement_and_child_colors() {
    let drawing = drawing(Vec::new(), 1);
    let before = drawing.model.clone();
    let display = build(&drawing, &Space::Model, &Options::default());
    display.require_complete_geometry().unwrap();
    assert_eq!(display.prims.len(), 2);
    let red = display.prims.iter().find(|primitive| primitive.color == Rgb(255, 0, 0)).unwrap();
    let green = display.prims.iter().find(|primitive| primitive.color == Rgb(0, 255, 0)).unwrap();
    assert_eq!(display.points(red), &[Vec2::new(8.0, 32.0), Vec2::new(6.0, 36.0)]);
    assert_eq!(display.points(green), &[Vec2::new(12.0, 8.0), Vec2::new(10.0, 12.0)]);
    assert_eq!(drawing.model, before);
}

#[test]
fn nested_mleader_block_content_uses_the_complete_parent_once() {
    let mut drawing = drawing(Vec::new(), 1);
    let child = drawing.model.iter().next().unwrap().as_ref().clone();
    drawing.model = Default::default();
    let mut block = Block::new("Outer");
    block.entities.push(child);
    drawing.blocks.insert("Outer".into(), Arc::new(block));
    let mut root = Entity::new(
        Handle(0x210),
        EntityKind::Insert(Insert {
            block: "Outer".into(),
            insert: Vec3::new(5.0, 7.0, 11.0),
            scale: Vec3::new(2.0, 3.0, 4.0),
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
    let display = build(&drawing, &Space::Model, &Options::default());
    display.require_complete_geometry().unwrap();
    let red = display.prims.iter().find(|primitive| primitive.color == Rgb(255, 0, 0)).unwrap();
    assert_eq!(display.points(red), &[Vec2::new(-21.0, 155.0), Vec2::new(-17.0, 167.0)]);
}

#[test]
fn mleader_attribute_planes_properties_and_visibility_match_the_world_query() {
    let text = Text {
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
    };
    for (mode, invisible, shown) in [(0, false, false), (1, false, true), (1, true, false), (2, true, true)] {
        let mut attribute = Attrib::new("A", text.clone());
        attribute.geometry_revision = GeometryRevision::CanonicalV2;
        attribute.handle = Some(Handle(0x204));
        attribute.plane.normal = Vec3::new(0.0, 0.0, -2.0);
        attribute.props.color = Some(Color::Index(5));
        attribute.invisible = invisible;
        let drawing = drawing(vec![attribute], mode);
        let display = build(&drawing, &Space::Model, &Options::default());
        display.require_complete_geometry().unwrap();
        assert_eq!(display.prims.iter().any(|primitive| primitive.color == Rgb(0, 0, 255)), shown, "mode {mode}, invisible {invisible}");
        let entity = drawing.model.iter().next().unwrap();
        let world = entity_world_geometry(&drawing, entity, &Mat4::IDENTITY).unwrap();
        let text = world.primitives().iter().find_map(|primitive| if let WorldPrimitive::Text(text) = primitive { Some(text) } else { None });
        assert_eq!(text.is_some(), shown, "mode {mode}, invisible {invisible}");
        if let Some(text) = text {
            assert_eq!(text.frame.origin, Vec3::new(14.0, -4.0, 30.0));
        }
    }
}

#[test]
fn a_refused_mleader_block_does_not_leave_earlier_children_on_screen() {
    let mut drawing = drawing(Vec::new(), 1);
    let mut block = Block::new("B");
    block.entities.push(line(0x205, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Color::Index(3)));
    let mut legacy = line(0x206, Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), Color::Index(4));
    legacy.geometry_revision = GeometryRevision::LegacyProjectedV1;
    block.entities.push(legacy);
    drawing.blocks.insert("B".into(), Arc::new(block));
    let display = build(&drawing, &Space::Model, &Options::default());
    assert!(display.prims.is_empty());
    assert!(display.verts.is_empty());
    assert!(display.tris.is_empty());
    assert_eq!(display.geometry_refusals.len(), 1);
    assert_eq!(display.geometry_refusals[0].handle, Handle(0x200));
    assert!(display.geometry_refusals[0].reason.contains("legacy"));
}

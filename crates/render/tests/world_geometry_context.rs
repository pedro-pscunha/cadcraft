use std::sync::Arc;

use cadcraft_doc::{Attrib, Block, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Line, Table, TableCell, Text, WorldPrimitive};
use cadcraft_geom::{Mat4, Vec3};
use cadcraft_render::entity_world_geometry;

fn table() -> Entity {
    let table = Table {
        insert: Vec3::new(1.0, 2.0, 3.0),
        col_widths: vec![2.0],
        row_heights: vec![1.0],
        cells: vec![vec![TableCell { text: "A".into(), ..TableCell::default() }]],
        title: false,
        header: false,
        ..Table::default()
    };
    let mut entity = Entity::new(Handle(0x130), EntityKind::Table(table));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    entity
}

#[test]
fn resolved_table_layout_uses_the_shared_world_frame_before_projection() {
    let drawing = Drawing::new_metric();
    let parent = Mat4 { m: [[2.0, 0.5, 1.0, 4.0], [0.0, 1.0, 0.0, 5.0], [0.0, 0.0, 1.0, 6.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity_world_geometry(&drawing, &table(), &parent).unwrap();
    let points: Vec<Vec3> = geometry
        .primitives()
        .iter()
        .flat_map(|primitive| match primitive {
            WorldPrimitive::Line { a, b } => vec![*a, *b],
            _ => Vec::new(),
        })
        .collect();
    assert!(!points.is_empty());
    assert!(points.iter().all(|p| p.y == 7.0 && (9.0..=10.0).contains(&p.z)));
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    assert_eq!((min_x, max_x), (10.0, 15.0));
    assert!(geometry.primitives().iter().any(|p| matches!(p, WorldPrimitive::Text(_))));
}

fn canonical(handle: u64, kind: EntityKind, normal: Vec3) -> Entity {
    let mut entity = Entity::new(Handle(handle), kind);
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = normal;
    entity
}

fn reference(block: &str) -> Insert {
    Insert {
        block: block.into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    }
}

#[test]
fn nested_minsert_geometry_retains_the_complete_xyz_product() {
    let mut drawing = Drawing::new_metric();
    let mut inner = Block::new("Inner");
    inner.base = Vec3::new(1.0, 2.0, 3.0);
    inner.entities.push(canonical(0x140, EntityKind::Line(Line { a: Vec3::new(2.0, 4.0, 6.0), b: Vec3::new(3.0, 5.0, 7.0) }), Vec3::Z));
    drawing.blocks.insert(inner.name.clone(), Arc::new(inner));
    let instance = Insert {
        insert: Vec3::new(10.0, 20.0, 30.0),
        scale: Vec3::new(-2.0, 3.0, -4.0),
        rotation: std::f64::consts::FRAC_PI_2,
        cols: 2,
        rows: 2,
        col_spacing: 5.0,
        row_spacing: 7.0,
        ..reference("Inner")
    };
    let mut outer = Block::new("Outer");
    outer.entities.push(canonical(0x141, EntityKind::Insert(instance), Vec3::new(0.0, 2.0, 0.0)));
    drawing.blocks.insert(outer.name.clone(), Arc::new(outer));
    let root = canonical(0x142, EntityKind::Insert(reference("Outer")), Vec3::Z);
    let parent = Mat4 { m: [[1.0, 0.0, 2.0, 5.0], [0.0, -1.0, 0.0, 7.0], [0.5, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity_world_geometry(&drawing, &root, &parent).unwrap();
    let starts: Vec<Vec3> = geometry
        .primitives()
        .iter()
        .filter_map(|p| match p {
            WorldPrimitive::Line { a, .. } => Some(*a),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 4);
    assert!((starts.first().unwrap().to_owned() - Vec3::new(37.0, -11.0, 16.0)).len() < 1e-10);
    assert!((starts.last().unwrap().to_owned() - Vec3::new(54.0, -11.0, 24.5)).len() < 1e-10);
}

#[test]
fn an_insert_attribute_uses_its_own_plane_and_only_the_enclosing_parent() {
    let mut drawing = Drawing::new_metric();
    drawing.blocks.insert("B".into(), Arc::new(Block::new("B")));
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
    let mut attribute = Attrib::new("A", text);
    attribute.geometry_revision = GeometryRevision::CanonicalV2;
    let instance = Insert { insert: Vec3::new(10.0, 20.0, 30.0), scale: Vec3::new(2.0, 3.0, 4.0), attribs: vec![attribute], ..reference("B") };
    let root = canonical(0x150, EntityKind::Insert(instance), Vec3::new(0.0, 1.0, 0.0));
    let parent = Mat4 { m: [[1.0, 0.0, 2.0, 5.0], [0.0, -1.0, 0.0, 7.0], [0.5, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity_world_geometry(&drawing, &root, &parent).unwrap();
    let [WorldPrimitive::Text(text)] = geometry.primitives() else { panic!("not attribute text") };
    assert_eq!(text.frame.origin, Vec3::new(12.0, 5.0, 3.5));
    let mut root = root;
    let EntityKind::Insert(instance) = &mut root.kind else { panic!("not insert") };
    let attribute = instance.attribs.first_mut().unwrap();
    attribute.text.value = "B".into();
    attribute.multiline = Some(cadcraft_doc::MultilineAttrib {
        geometry_revision: GeometryRevision::CanonicalV2,
        normal: Vec3::new(0.0, 1.0, 0.0),
        read_as: "A".into(),
        mtext: cadcraft_doc::MText {
            insert: Vec3::new(4.0, 5.0, 6.0),
            height: 1.0,
            width: 2.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "Old".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: Some(Vec3::Z),
            drawing_direction: None,
        },
        embedded_46: None,
        type_72: None,
    });
    let geometry = entity_world_geometry(&drawing, &root, &parent).unwrap();
    let [WorldPrimitive::Text(text)] = geometry.primitives() else { panic!("not one multiline attribute") };
    assert_eq!(text.frame.origin, Vec3::new(21.0, 2.0, 8.0));
    let cadcraft_doc::WorldTextContent::MText(text) = &text.content else { panic!("not multiline text") };
    assert_eq!(text.contents, "B");
    let EntityKind::Insert(instance) = &root.kind else { panic!("not insert") };
    let mut definition = Entity::new(Handle(0x151), EntityKind::AttDef(instance.attribs.first().unwrap().clone()));
    definition.common.visible = false;
    Arc::make_mut(drawing.blocks.get_mut("B").unwrap()).entities.push(definition);
    assert!(entity_world_geometry(&drawing, &root, &parent).unwrap().primitives().is_empty());
}

#[test]
fn multileader_block_content_uses_the_builders_complete_block_matrix() {
    let mut drawing = Drawing::new_metric();
    let mut block = Block::new("B");
    block.base = Vec3::new(1.0, 2.0, 3.0);
    block.entities.push(canonical(0x172, EntityKind::Line(Line { a: Vec3::new(2.0, 4.0, 6.0), b: Vec3::new(3.0, 5.0, 7.0) }), Vec3::Z));
    drawing.blocks.insert("B".into(), Arc::new(block));
    let mut leader = cadcraft_doc::MLeader::single(Vec3::ZERO, 0.0, false, Vec::new(), None);
    leader.context.geometry_revision = GeometryRevision::CanonicalV2;
    leader.context.block_normal = Vec3::new(0.0, 1.0, 0.0);
    leader.block = Some(Insert { insert: Vec3::new(10.0, 20.0, 30.0), scale: Vec3::new(2.0, 3.0, 4.0), ..reference("B") });
    let entity = canonical(0x173, EntityKind::MLeader(leader), Vec3::Z);
    let parent = Mat4 { m: [[1.0, 0.0, 2.0, 5.0], [0.0, -1.0, 0.0, 7.0], [0.5, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let geometry = entity_world_geometry(&drawing, &entity, &parent).unwrap();
    let [WorldPrimitive::Line { a, b }] = geometry.primitives() else { panic!("not the content line") };
    assert!((*a - Vec3::new(85.0, -25.0, 40.0)).len() < 1e-10);
    assert!((*b - Vec3::new(89.0, -29.0, 42.0)).len() < 1e-10);
}

#[test]
fn canonical_block_traversal_refuses_cycles_and_legacy_children() {
    let mut drawing = Drawing::new_metric();
    let root = canonical(0x160, EntityKind::Insert(reference("B")), Vec3::Z);
    let mut block = Block::new("B");
    block.entities.push(root.clone());
    drawing.blocks.insert("B".into(), Arc::new(block));
    assert!(entity_world_geometry(&drawing, &root, &Mat4::IDENTITY).is_err());
    let mut block = Block::new("B");
    block.entities.push(Entity::new(Handle(0x161), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) })));
    drawing.blocks.insert("B".into(), Arc::new(block));
    let error = entity_world_geometry(&drawing, &root, &Mat4::IDENTITY).unwrap_err();
    assert!(error.to_string().contains("legacy"), "{error}");
}

#[test]
fn layout_resolution_does_not_promote_a_legacy_entity() {
    let mut entity = table();
    entity.geometry_revision = GeometryRevision::LegacyProjectedV1;
    assert!(entity_world_geometry(&Drawing::new_metric(), &entity, &Mat4::IDENTITY).is_err());
}

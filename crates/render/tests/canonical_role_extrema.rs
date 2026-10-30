use cadcraft_doc::{Block, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, LwPolyline, Point, Space};
use cadcraft_geom::{Mat4, PolyVertex, Vec2, Vec3};
use cadcraft_render::{Options, build, entity_world_bounds};

#[test]
fn a_wide_bulge_bounds_its_between_sample_extremum_after_an_affine_parent() {
    let mut drawing = Drawing::new_metric();
    let mut entity = Entity::new(
        Handle(0x200),
        EntityKind::LwPolyline(LwPolyline {
            records: Default::default(),
            vertices: vec![PolyVertex { bulge: 1.0, ..PolyVertex::new(Vec2::ZERO) }, PolyVertex::new(Vec2::new(20.0, 0.0))],
            closed: false,
            const_width: 2.0,
            elevation: 0.0,
            plinegen: true,
            heavy: false,
            fit: None,
        }),
    );
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 0.6, 0.8);
    drawing.model.push(entity.clone());
    let parent = Mat4 { m: [[1.0, 0.123, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let bounds = entity_world_bounds(&drawing, &entity, &parent).unwrap();
    let expected = -10.0 + 11.0 * (1.0_f64 + (0.123_f64 * 0.8).powi(2)).sqrt();
    assert!(bounds.max.x >= expected - 1e-9, "{} excludes the outer edge at {expected}", bounds.max.x);
}

#[test]
fn nested_insert_bounds_include_a_wide_bulge_child_under_an_affine_parent() {
    let mut drawing = Drawing::new_metric();
    let mut curve = Entity::new(
        Handle(0x200),
        EntityKind::LwPolyline(LwPolyline {
            records: Default::default(),
            vertices: vec![PolyVertex { bulge: 1.0, ..PolyVertex::new(Vec2::ZERO) }, PolyVertex::new(Vec2::new(20.0, 0.0))],
            closed: false,
            const_width: 2.0,
            elevation: 0.0,
            plinegen: true,
            heavy: false,
            fit: None,
        }),
    );
    curve.geometry_revision = GeometryRevision::CanonicalV2;
    curve.common.extrusion = Vec3::new(0.0, 0.6, 0.8);
    let mut block = Block::new("Curve");
    block.base = Vec3::new(1.0, 2.0, 3.0);
    block.entities.push(curve);
    drawing.blocks.insert("Curve".into(), std::sync::Arc::new(block));
    let insert = |handle, block: &str, at, scale| {
        let mut entity = Entity::new(
            Handle(handle),
            EntityKind::Insert(Insert {
                block: block.into(),
                insert: at,
                scale,
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        );
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity
    };
    let mut outer = Block::new("Outer");
    outer.entities.push(insert(0x201, "Curve", Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
    drawing.blocks.insert("Outer".into(), std::sync::Arc::new(outer));
    let root = insert(0x202, "Outer", Vec3::new(5.0, 7.0, 11.0), Vec3::new(2.0, 3.0, 4.0));
    drawing.model.push(root.clone());
    let parent = Mat4 { m: [[1.0, 0.123, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let bounds = entity_world_bounds(&drawing, &root, &parent).unwrap();
    let expected = -17.0 + 0.123 + 11.0 * (4.0_f64 + (0.123_f64 * 0.8 * 3.0).powi(2)).sqrt();
    assert!(bounds.max.x >= expected - 1e-9, "{} excludes the nested outer edge at {expected}", bounds.max.x);
}

#[test]
fn an_edge_on_parent_keeps_a_point_markers_relative_size() {
    let mut drawing = Drawing::new_metric();
    drawing.header.set_i64("PDMODE", 2);
    drawing.header.set_f64("PDSIZE", -5.0);
    let mut point = Entity::new(Handle(0x201), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }));
    point.geometry_revision = GeometryRevision::CanonicalV2;
    let mut block = Block::new("Marker");
    block.entities.push(point);
    drawing.blocks.insert("Marker".into(), std::sync::Arc::new(block));
    let mut parent = Entity::new(
        Handle(0x202),
        EntityKind::Insert(Insert {
            block: "Marker".into(),
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
    parent.geometry_revision = GeometryRevision::CanonicalV2;
    parent.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    drawing.model.push(parent);
    let display = build(&drawing, &Space::Model, &Options { view_height: 100.0, tolerance: 1e-6, ..Options::default() });
    display.require_complete_geometry().unwrap();
    assert!((display.bounds.width() - 5.0).abs() < 1e-9, "{:?}", display.bounds);
}

#[test]
fn an_edge_on_point_marker_keeps_its_relative_size() {
    let mut drawing = Drawing::new_metric();
    drawing.header.set_i64("PDMODE", 2);
    drawing.header.set_f64("PDSIZE", -5.0);
    let mut entity = Entity::new(Handle(0x201), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    drawing.model.push(entity);
    let display = build(&drawing, &Space::Model, &Options { view_height: 100.0, tolerance: 1e-6, ..Options::default() });
    display.require_complete_geometry().unwrap();
    assert!((display.bounds.width() - 5.0).abs() < 1e-9, "{:?}", display.bounds);
}

use cadcraft_doc::{Drawing, Entity, EntityKind, GeometryRevision, Handle, Table, TableCell, WorldPrimitive};
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

#[test]
fn layout_resolution_does_not_promote_a_legacy_entity() {
    let mut entity = table();
    entity.geometry_revision = GeometryRevision::LegacyProjectedV1;
    assert!(entity_world_geometry(&Drawing::new_metric(), &entity, &Mat4::IDENTITY).is_err());
}

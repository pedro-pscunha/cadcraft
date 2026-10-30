use std::sync::Arc;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{
    Block, CellStyle, DimKind, Dimension, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Space, Table, TableCell, Tolerance,
};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_render::{DisplayList, Options, build};

fn entity(kind: EntityKind, revision: GeometryRevision, normal: Vec3) -> Entity {
    let mut entity = Entity::new(Handle(0x200), kind);
    entity.geometry_revision = revision;
    entity.common.extrusion = normal;
    entity.common.color = Color::Index(1);
    entity
}

fn styles() -> Drawing {
    let mut drawing = Drawing::new_metric();
    let style = drawing.dim_styles.iter_mut().find(|style| style.name == "Standard").unwrap();
    style.dim_line_color = Color::Index(3);
    style.ext_line_color = Color::Index(4);
    style.text_color = Color::Index(5);
    drawing
}

fn display(drawing: &Drawing) -> DisplayList {
    let list = build(drawing, &Space::Model, &Options::default());
    list.require_complete_geometry().unwrap();
    list
}

fn check_placement_and_appearance(actual: &DisplayList, local: &DisplayList) {
    assert_eq!(actual.prims.len(), local.prims.len());
    for (actual, expected) in actual.prims.iter().zip(&local.prims) {
        assert_eq!((actual.kind, actual.color, actual.lw), (expected.kind, expected.color, expected.lw));
    }
    assert_eq!(actual.verts.len(), local.verts.len());
    assert_eq!(actual.tris.len(), local.tris.len());
    for (actual, source) in actual.verts.iter().chain(&actual.tris).zip(local.verts.iter().chain(&local.tris)) {
        let expected = Vec2::new(10.0 + source.x, 20.0 + 0.8 * source.y);
        assert!((*actual - expected).len() < 1e-9, "{actual:?} != {expected:?}");
    }
}

#[test]
fn canonical_table_preserves_cell_paint_roles_through_its_world_plane() {
    let mut reference = styles();
    let mut cell_style = CellStyle { fill: Some(Color::Index(3)), text_color: Color::Index(5), ..CellStyle::default() };
    for border in cell_style.borders.as_array_mut() {
        border.color = Color::Index(4);
    }
    reference.table_styles.first_mut().unwrap().cell_styles = vec![cell_style];
    let table = Table {
        insert: Vec3::ZERO,
        col_widths: vec![2.0],
        row_heights: vec![1.0],
        cells: vec![vec![TableCell { text: "A".into(), ..TableCell::default() }]],
        title: false,
        header: false,
        ..Table::default()
    };
    let mut actual = reference.clone();
    reference.model.push(entity(EntityKind::Table(table.clone()), GeometryRevision::LegacyProjectedV1, Vec3::Z));
    actual.model.push(entity(
        EntityKind::Table(Table { insert: Vec3::new(10.0, 20.0, 30.0), ..table }),
        GeometryRevision::CanonicalV2,
        Vec3::new(0.0, 0.6, 0.8),
    ));
    let local = display(&reference);
    for color in [Rgb(0, 255, 0), Rgb(0, 255, 255), Rgb(0, 0, 255)] {
        assert!(local.prims.iter().any(|primitive| primitive.color == color));
    }
    check_placement_and_appearance(&display(&actual), &local);
}

#[test]
fn canonical_tolerance_preserves_frame_and_text_colors_through_its_world_plane() {
    let mut reference = styles();
    let tolerance = Tolerance { insert: Vec3::ZERO, dir: Vec3::new(1.0, 0.0, 0.0), style: "Standard".into(), text: "0.1".into() };
    let mut actual = reference.clone();
    reference.model.push(entity(EntityKind::Tolerance(tolerance.clone()), GeometryRevision::LegacyProjectedV1, Vec3::Z));
    actual.model.push(entity(
        EntityKind::Tolerance(Tolerance { insert: Vec3::new(10.0, 20.0, 30.0), ..tolerance }),
        GeometryRevision::CanonicalV2,
        Vec3::new(0.0, 0.6, 0.8),
    ));
    let local = display(&reference);
    assert!(local.prims.iter().any(|primitive| primitive.color == Rgb(0, 255, 0)));
    assert!(local.prims.iter().any(|primitive| primitive.color == Rgb(0, 0, 255)));
    check_placement_and_appearance(&display(&actual), &local);
}

#[test]
fn canonical_dimension_preserves_line_roles_under_a_full_spatial_parent() {
    let mut reference = styles();
    let dimension = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(10.0, 3.0, 0.0),
        text_mid: Vec3::new(5.0, 3.0, 0.0),
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 10.0,
        text_rotation: 0.0,
        user_text_pos: true,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    };
    let mut actual = reference.clone();
    reference.model.push(entity(EntityKind::Dimension(dimension.clone()), GeometryRevision::LegacyProjectedV1, Vec3::Z));
    let mut block = Block::new("B");
    block.entities.push(entity(EntityKind::Dimension(dimension), GeometryRevision::CanonicalV2, Vec3::Z));
    actual.blocks.insert("B".into(), Arc::new(block));
    actual.model.push(entity(
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::new(-10.0, 2.0, 36.0),
            scale: Vec3::new(-1.0, -1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
        GeometryRevision::CanonicalV2,
        Vec3::new(0.0, 0.6, 0.8),
    ));
    let local = display(&reference);
    for color in [Rgb(0, 255, 0), Rgb(0, 255, 255), Rgb(0, 0, 255)] {
        assert!(local.prims.iter().any(|primitive| primitive.color == color));
    }
    check_placement_and_appearance(&display(&actual), &local);
}

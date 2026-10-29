//! AutoCAD distinguishes a MINSERT from a single reference by its subclass marker.

use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Insert, Point, Space};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn drawing() -> Drawing {
    let mut drawing = Drawing::new_metric();
    let mut block = Block::new("PointBlock");
    block.entities.push(Entity::new(Handle(0x80), EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 3.0), angle: 0.0 })));
    drawing.blocks.insert(block.name.clone(), std::sync::Arc::new(block));
    drawing
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::Insert(Insert {
                block: "PointBlock".into(),
                insert: Vec3::new(10.0, 20.0, 7.0),
                scale: Vec3::new(-2.0, 3.0, 0.5),
                rotation: 0.5,
                attribs: Vec::new(),
                cols: 3,
                rows: 2,
                col_spacing: 5.0,
                row_spacing: -4.0,
            }),
        )
        .unwrap();
    drawing
}

fn check_grid(drawing: &Drawing) {
    let EntityKind::Insert(insert) = &drawing.model.iter().next().unwrap().kind else { panic!("INSERT") };
    assert_eq!((insert.cols, insert.rows), (3, 2));
    assert_eq!(insert.col_spacing.to_bits(), 5.0_f64.to_bits());
    assert_eq!(insert.row_spacing.to_bits(), (-4.0_f64).to_bits());
}

fn insert_record(bytes: &[u8]) -> Vec<cadcraft_dxf::Tag> {
    cadcraft_dxf::sections(&cadcraft_dxf::parse(bytes).unwrap())
        .into_iter()
        .find(|section| section.name == "ENTITIES")
        .and_then(|section| cadcraft_dxf::records(&section.tags).into_iter().find(|(kind, _)| kind == "INSERT"))
        .unwrap()
        .1
}

#[test]
fn user_dxf_marks_a_grid_as_acdb_m_insert_block() {
    let bytes = cadcraft_io::write_with(&drawing(), "grid.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let record = insert_record(&bytes);
    let classes: Vec<_> = record.iter().filter(|tag| tag.code == 100).map(cadcraft_dxf::Tag::str).collect();
    assert_eq!(classes, ["AcDbEntity", "AcDbMInsertBlock"]);
    check_grid(&cadcraft_io::read_dxf(&bytes).unwrap());
}

#[test]
fn either_array_axis_uses_the_grid_subclass_but_single_references_do_not() {
    for (cols, rows, expected) in [(1, 1, "AcDbBlockReference"), (3, 1, "AcDbMInsertBlock"), (1, 2, "AcDbMInsertBlock")] {
        let mut drawing = drawing();
        let handle = drawing.model.iter().next().unwrap().handle;
        drawing
            .modify_entity(handle, |entity| {
                let EntityKind::Insert(insert) = &mut entity.kind else { panic!("INSERT") };
                insert.cols = cols;
                insert.rows = rows;
            })
            .unwrap();
        let record = insert_record(cadcraft_io::write_dxf(&drawing).as_bytes());
        assert_eq!(record.iter().rfind(|tag| tag.code == 100).unwrap().str(), expected);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_dwg_bridge_keeps_grid_counts_and_spacing() {
    let bytes = cadcraft_io::write(&drawing(), "grid.dwg").unwrap();
    check_grid(&cadcraft_io::read(&bytes, "grid.dwg").unwrap());
}

#[test]
fn a_source_minsert_keeps_its_plane_and_signed_geometry_on_a_property_edit() {
    let bytes = cadcraft_io::write_with(&drawing(), "grid.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let mut tags = cadcraft_dxf::parse(&bytes).unwrap();
    let mut inside = false;
    let mut changed = false;
    let mut end = None;
    for (index, tag) in tags.iter_mut().enumerate() {
        if tag.code == 0 {
            if inside {
                end = Some(index);
                break;
            }
            inside = tag.str() == "INSERT";
        } else if inside && tag.code == 100 && tag.str() == "AcDbBlockReference" {
            *tag = cadcraft_dxf::Tag::s(100, "AcDbMInsertBlock");
            changed = true;
        }
    }
    assert!(changed || insert_record(&bytes).iter().any(|tag| tag.code == 100 && tag.str() == "AcDbMInsertBlock"));
    let end = end.unwrap();
    tags.splice(end..end, [cadcraft_dxf::Tag::f(210, 0.0), cadcraft_dxf::Tag::f(220, 0.8), cadcraft_dxf::Tag::f(230, 0.6)]);
    let source = cadcraft_dxf::write_ascii(&tags);
    let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let handle = drawing.model.iter().next().unwrap().handle;
    drawing.modify_entity(handle, |entity| entity.common.layer = "changed".into()).unwrap();
    let saved = cadcraft_io::write_with(&drawing, "grid.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let before = insert_record(source.as_bytes());
    let after = insert_record(&saved);
    for code in [10, 20, 30, 41, 42, 43, 50, 70, 71, 44, 45, 210, 220, 230] {
        let before = before.iter().find(|tag| tag.code == code).unwrap().f64();
        let values: Vec<_> = after.iter().filter(|tag| tag.code == code).collect();
        assert_eq!(values.len(), 1, "group {code}");
        assert_eq!(values[0].f64().to_bits(), before.to_bits(), "group {code}");
    }
    assert_eq!(after.iter().find(|tag| tag.code == 8).unwrap().str(), "changed");
}

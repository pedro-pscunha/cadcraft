//! Group 70 as AutoCAD writes it: a 3DFACE has it only while an edge is invisible; a DIMENSION has bit 32 (its block is its own) in
//! model space and paper space, not inside a block definition; the group stays at 0 there (AutoCAD refuses a file without it).

use std::sync::Arc;

use cadcraft_doc::{Block, Common, DimKind, Dimension, Drawing, Entity, EntityKind, Face3d, Handle, Space};
use cadcraft_geom::Vec3;

use super::foreign_passthrough::{section, val};

fn dimension(kind: DimKind, user_text_pos: bool) -> Dimension {
    Dimension {
        kind,
        defpt: Vec3::new(10.0, 0.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn face(hidden_edges: u8) -> EntityKind {
    EntityKind::Face3d(Face3d { corners: [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0)], hidden_edges })
}

/// The group 70 values of the records of `kind` in a section, `""` for none (the one of the AcDbDimension subclass for a dimension).
fn flags(text: &str, sec: &str, kind: &str) -> Vec<String> {
    section(text, sec).into_iter().filter(|(k, _)| k == kind).map(|(_, t)| val(&t, 70)).collect()
}

#[test]
fn a_face_has_group_70_only_with_an_invisible_edge() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), face(0)).unwrap();
    d.add(&Space::Model, Common::default(), face(2)).unwrap();
    assert_eq!(flags(&cadcraft_io::write_dxf(&d), "ENTITIES", "3DFACE"), ["", "2"]);
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let hidden: Vec<u8> = back.model.iter().filter_map(|e| if let EntityKind::Face3d(f) = &e.kind { Some(f.hidden_edges) } else { None }).collect();
    assert_eq!(hidden, [0, 2]);
}

#[test]
fn bit_32_is_set_outside_blocks_only() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(DimKind::Linear { rotation: 0.0 }, false))).unwrap();
    let mut b = Block::new("Plan");
    for (n, kind, moved) in [(1u64, DimKind::Linear { rotation: 0.0 }, false), (2, DimKind::Aligned, false), (3, DimKind::Aligned, true)] {
        b.entities.push(Entity::new(Handle(0x900 + n), EntityKind::Dimension(dimension(kind, moved))));
    }
    d.blocks.insert("Plan".into(), Arc::new(b));
    d.bump_handseed(Handle(0x903));
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(flags(&text, "ENTITIES", "DIMENSION"), ["32"], "model space");
    let in_block = flags(&text, "BLOCKS", "DIMENSION");
    // The dimensions of the block are the ones with a handle of the block; the anonymous blocks of the others may come first.
    assert!(in_block.ends_with(&["0".to_string(), "1".to_string(), "129".to_string()]), "{in_block:?}");
}

#[test]
fn a_dwg_has_bit_32_outside_blocks_only_too() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(DimKind::Linear { rotation: 0.0 }, false))).unwrap();
    let mut b = Block::new("Plan");
    b.entities.push(Entity::new(Handle(0x901), EntityKind::Dimension(dimension(DimKind::Aligned, false))));
    d.blocks.insert("Plan".into(), Arc::new(b));
    d.bump_handseed(Handle(0x901));
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let doc = acadrust::DwgReader::from_stream(std::io::Cursor::new(&dwg)).read().unwrap();
    // The flags byte of the DWG dimension holds group 70's bit 32 as its bit 1.
    let mut seen: Vec<(String, u8)> = Vec::new();
    for e in doc.entities() {
        let acadrust::entities::EntityType::Dimension(dim) = e else { continue };
        let owner = doc.block_records.iter().find(|br| br.handle == dim.base().common.owner_handle).map(|br| br.name.clone());
        seen.push((owner.unwrap_or_default(), dim.base().dwg_flags_byte & 0x02));
    }
    assert!(seen.contains(&("*Model_Space".to_string(), 2)), "{seen:?}");
    assert!(seen.contains(&("Plan".to_string(), 0)), "{seen:?}");
}

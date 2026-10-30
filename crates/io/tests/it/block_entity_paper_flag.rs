//! An entity inside a block definition keeps its paper space flag (group 67): AutoCAD writes it on
//! the content of a block made from objects drawn in paper space. A DWG has no such flag on block content.

use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Line};
use cadcraft_geom::Vec3;

use super::foreign_passthrough::{section, val};

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut b = Block::new("Sheet");
    for (n, paper) in [(1u64, true), (2, false)] {
        let common = Common { paper_space: paper, ..Common::default() };
        let line = EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) });
        b.entities.push(Entity { common, ..Entity::new(Handle(0x900 + n), line) });
    }
    d.blocks.insert("Sheet".into(), Arc::new(b));
    d.bump_handseed(Handle(0x902));
    d
}

/// The group 67 of each LINE of the block, in order.
fn flags(text: &str) -> Vec<String> {
    section(text, "BLOCKS").into_iter().filter(|(k, _)| k == "LINE").map(|(_, t)| val(&t, 67)).collect()
}

#[test]
fn the_paper_space_flag_of_block_content_is_written() {
    assert_eq!(flags(&cadcraft_io::write_dxf(&drawing())), ["1", ""]);
}

#[test]
fn a_read_keeps_it() {
    let read = cadcraft_io::read(cadcraft_io::write_dxf(&drawing()).as_bytes(), "x.dxf").unwrap();
    let flags: Vec<bool> = read.block("Sheet").unwrap().entities.iter().map(|e| e.common.paper_space).collect();
    assert_eq!(flags, [true, false]);
    assert_eq!(super::block_entity_paper_flag::flags(&cadcraft_io::write_dxf(&read)), ["1", ""]);
}

//! Two objects with one handle: a save keeps both. The later holder (model space first, then the layouts, then the blocks) gets
//! a new handle and the save notes it. Before, the DXF repeated the handle and the DWG conversion dropped one object without
//! a word.

use std::sync::Arc;

use cadcraft_doc::*;
use cadcraft_geom::Vec3;

fn line(h: u64, x: f64) -> Entity {
    Entity::new(Handle(h), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(x, 1.0, 0.0) }))
}

fn multileader(h: u64) -> Entity {
    let lines = vec![vec![Vec3::ZERO, Vec3::new(2.0, 3.0, 0.0)]];
    Entity::new(Handle(h), EntityKind::MLeader(MLeader::single(Vec3::new(5.0, 4.0, 0.0), 0.75, false, lines, None)))
}

/// A model line and a block that holds a multileader and a line, all with handle 0x100.
fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut b = Block::new("B");
    b.entities.push(multileader(0x100));
    d.blocks.insert("B".into(), Arc::new(b));
    d.model.push(line(0x100, 5.0));
    d
}

fn block_multileaders(d: &Drawing) -> usize {
    d.block("B").map_or(0, |b| b.entities.iter().filter(|e| matches!(e.kind, EntityKind::MLeader(_))).count())
}

fn model_lines(d: &Drawing) -> usize {
    d.model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count()
}

#[test]
fn a_repeated_handle_keeps_both_objects_in_the_dxf_and_the_dwg() {
    let d = drawing();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&d, name, &cadcraft_io::SaveOptions { version: None, binary: false }).unwrap();
        let back = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!((model_lines(&back), block_multileaders(&back)), (1, 1), "{name}");
        assert!(
            notes.iter().any(|n| n.contains("Handle 100") && n.contains("model space") && n.contains("block B")),
            "{name}: the note names the handle and both places: {notes:?}"
        );
    }
}

#[test]
fn the_first_holder_keeps_the_handle() {
    let d = drawing();
    let (fixed, repairs) = d.with_unique_handles().expect("a handle is held twice");
    assert_eq!(fixed.model.iter().next().map(|e| e.handle), Some(Handle(0x100)));
    assert_eq!(repairs.len(), 1);
    assert_eq!((repairs[0].old, repairs[0].kept_in.as_str(), repairs[0].moved_in.as_str()), (Handle(0x100), "model space", "block B"));
    let in_block = fixed.block("B").and_then(|b| b.entities.iter().next().map(|e| e.handle)).unwrap();
    assert_ne!(in_block, Handle(0x100));
    assert!(fixed.with_unique_handles().is_none(), "the repaired drawing has no repeated handle");
}

#[test]
fn a_drawing_with_unique_handles_is_untouched() {
    let mut d = Drawing::new_metric();
    d.model.push(line(0x100, 1.0));
    d.model.push(line(0x101, 2.0));
    assert!(d.with_unique_handles().is_none());
}

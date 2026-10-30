//! A save gives the tables, symbol records, blocks and layouts of a file the handles the file gave
//! them, and keeps every owner pointer on them. A record made since gets a new handle above them.

use std::collections::BTreeMap;
use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, EntityKind, Handle, Layer, Point, SourceSymbols, Space};
use cadcraft_dxf::Tag;
use cadcraft_geom::Vec3;

use super::foreign_passthrough::{saved_twice, section, val};

fn handles(pairs: &[(&str, &str, u64)]) -> BTreeMap<(String, String), Handle> {
    pairs.iter().map(|(kind, name, h)| ((kind.to_string(), name.to_string()), Handle(*h))).collect()
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Walls"));
    d.blocks.insert("Alpha".into(), Arc::new(Block::new("Alpha")));
    let source = SourceSymbols {
        handles: handles(&[
            ("TABLE", "BLOCK_RECORD", 0x70),
            ("LAYER", "WALLS", 0x71),
            ("BLOCK_RECORD", "ALPHA", 0x72),
            ("BLOCK", "ALPHA", 0x73),
            ("ENDBLK", "ALPHA", 0x74),
        ]),
        ..SourceSymbols::default()
    };
    d.source_symbols = Arc::new(source);
    d
}

/// The handle (group 5) of the record of `kind` named `name` in a section.
fn handle_of(text: &str, sec: &str, kind: &str, name: &str) -> String {
    let (_, tags) = section(text, sec).into_iter().find(|(k, t)| k == kind && val(t, 2) == name).unwrap_or_else(|| panic!("no {kind} {name}"));
    val(&tags, 5)
}

fn tags_of(text: &str, sec: &str, kind: &str, name: &str) -> Vec<Tag> {
    section(text, sec).into_iter().find(|(k, t)| k == kind && val(t, 2) == name).map(|(_, t)| t).unwrap_or_default()
}

#[test]
fn records_keep_the_handles_of_the_file() {
    let d = drawing();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        assert_eq!(handle_of(&text, "TABLES", "LAYER", "Walls"), "71", "{what}: layer");
        assert_eq!(handle_of(&text, "TABLES", "BLOCK_RECORD", "Alpha"), "72", "{what}: block record");
        assert_eq!(handle_of(&text, "BLOCKS", "BLOCK", "Alpha"), "73", "{what}: block");
    }
}

#[test]
fn owner_pointers_follow_the_kept_handles() {
    let d = drawing();
    let text = cadcraft_io::write_dxf(&d);
    let table = section(&text, "TABLES").into_iter().find(|(k, t)| k == "TABLE" && val(t, 2) == "BLOCK_RECORD").map(|(_, t)| val(&t, 5)).unwrap();
    assert_eq!(table, "70");
    let record = tags_of(&text, "TABLES", "BLOCK_RECORD", "Alpha");
    assert_eq!(val(&record, 330), "70", "the record is owned by its table");
    let block = tags_of(&text, "BLOCKS", "BLOCK", "Alpha");
    assert_eq!(val(&block, 330), "72", "the block belongs to its record");
    let end = section(&text, "BLOCKS").into_iter().find(|(k, t)| k == "ENDBLK" && val(t, 5) == "74").map(|(_, t)| val(&t, 330));
    assert_eq!(end.as_deref(), Some("72"), "the block end belongs to its record");
}

#[test]
fn a_record_made_since_gets_a_new_handle_above_the_old_ones() {
    let mut d = drawing();
    d.layers.push(Layer::new("Doors"));
    d.blocks.insert("Beta".into(), Arc::new(Block::new("Beta")));
    let text = cadcraft_io::write_dxf(&d);
    for (sec, kind, name) in [("TABLES", "LAYER", "Doors"), ("TABLES", "BLOCK_RECORD", "Beta"), ("BLOCKS", "BLOCK", "Beta")] {
        let h = u64::from_str_radix(&handle_of(&text, sec, kind, name), 16).unwrap();
        assert!(h > 0x1000, "{kind} {name}: {h:X} is a new handle");
    }
}

#[test]
fn a_handle_an_entity_has_is_not_given_to_a_record() {
    let mut d = drawing();
    let point = d.add(&Space::Model, Common::default(), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 })).unwrap();
    // The file says Walls was 0x71, but this entity has that handle now.
    let mut source = (*d.source_symbols).clone();
    source.handles.insert(("LAYER".into(), "WALLS".into()), point);
    d.source_symbols = Arc::new(source);
    let text = cadcraft_io::write_dxf(&d);
    assert_ne!(handle_of(&text, "TABLES", "LAYER", "Walls"), point.hex());
    let entity = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "POINT").map(|(_, t)| val(&t, 5));
    assert_eq!(entity, Some(point.hex()));
}

#[test]
fn the_handles_survive_a_read_and_a_second_save() {
    let d = drawing();
    let read = cadcraft_io::read(cadcraft_io::write_dxf(&d).as_bytes(), "x.dxf").unwrap();
    for ((kind, name), h) in &drawing().source_symbols.handles {
        assert_eq!(read.source_symbols.handles.get(&(kind.clone(), name.clone())), Some(h), "{kind} {name}");
    }
    let text = cadcraft_io::write_dxf(&read);
    assert_eq!(handle_of(&text, "TABLES", "LAYER", "Walls"), "71");
    assert_eq!(handle_of(&text, "TABLES", "BLOCK_RECORD", "Alpha"), "72");
}

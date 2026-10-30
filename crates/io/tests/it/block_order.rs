//! A file keeps its block records in the order it had them, and its layouts under the block names it gave
//! them: an anonymous block's number (`*U3`, `*D12`) is its place among the block records, and a
//! layout block's name (`*Paper_Space5`) is the name other files and tools know it by.

use std::collections::BTreeMap;
use std::sync::Arc;

use cadcraft_doc::{Block, Drawing, SourceSymbols};

use super::foreign_passthrough::{saved_twice, section, val};

/// The names of the BLOCK_RECORD records of a DXF text, in file order, and those of its BLOCKS section.
fn block_names(text: &str) -> (Vec<String>, Vec<String>) {
    let records = section(text, "TABLES").into_iter().filter(|(k, _)| k == "BLOCK_RECORD").map(|(_, t)| val(&t, 2)).collect();
    let blocks = section(text, "BLOCKS").into_iter().filter(|(k, _)| k == "BLOCK").map(|(_, t)| val(&t, 2)).collect();
    (records, blocks)
}

fn drawing_with_order(order: &[&str]) -> Drawing {
    let mut d = Drawing::new_metric();
    for name in order {
        let mut b = Block::new(name);
        b.anonymous = name.starts_with('*');
        d.blocks.insert((*name).into(), Arc::new(b));
    }
    let block_order: BTreeMap<String, usize> = order.iter().enumerate().map(|(i, n)| (n.to_ascii_uppercase(), i)).collect();
    d.source_symbols = Arc::new(SourceSymbols { block_order, ..SourceSymbols::default() });
    d
}

fn user_blocks(names: Vec<String>) -> Vec<String> {
    names.into_iter().filter(|n| !n.starts_with("*Model_Space") && !n.starts_with("*Paper_Space")).collect()
}

#[test]
fn blocks_are_written_in_the_order_of_the_file_they_came_from() {
    let d = drawing_with_order(&["Beta", "*U2", "Alpha", "*U1"]);
    let (records, blocks) = block_names(&cadcraft_io::write_dxf(&d));
    assert_eq!(user_blocks(records), ["Beta", "*U2", "Alpha", "*U1"], "BLOCK_RECORD");
    assert_eq!(user_blocks(blocks), ["Beta", "*U2", "Alpha", "*U1"], "BLOCKS");
}

#[test]
fn a_block_the_file_did_not_have_follows_the_ones_it_had() {
    let mut d = drawing_with_order(&["Zulu", "Alpha"]);
    d.blocks.insert("Brand_New".into(), Arc::new(Block::new("Brand_New")));
    let (records, _) = block_names(&cadcraft_io::write_dxf(&d));
    assert_eq!(user_blocks(records), ["Zulu", "Alpha", "Brand_New"]);
}

#[test]
fn the_order_survives_a_read_and_a_second_save() {
    let d = drawing_with_order(&["Beta", "*U2", "Alpha", "*U1"]);
    let read = cadcraft_io::read(cadcraft_io::write_dxf(&d).as_bytes(), "x.dxf").unwrap();
    let (records, _) = block_names(&cadcraft_io::write_dxf(&read));
    assert_eq!(user_blocks(records), ["Beta", "*U2", "Alpha", "*U1"], "read and saved again");
}

#[test]
fn layouts_keep_the_block_names_the_file_gave_them() {
    let mut d = Drawing::new_metric();
    d.layouts = vec![
        cadcraft_doc::Layout::new("A", 1),
        cadcraft_doc::Layout::new("B", 2),
        cadcraft_doc::Layout::new("C", 3),
        cadcraft_doc::Layout::new("D", 4),
    ];
    let layout_blocks = [("C", "*Paper_Space0"), ("B", "*Paper_Space5")].map(|(l, b)| (l.to_string(), b.to_string())).into_iter().collect();
    d.source_symbols = Arc::new(SourceSymbols { layout_blocks, ..SourceSymbols::default() });
    let names: Vec<(String, String)> = d.paper_space_blocks().into_iter().map(|(n, l)| (l.name.clone(), n)).collect();
    // A has no block of its own yet and is first: `*Paper_Space`. D is new: the first name nobody has.
    let expected = [("A", "*Paper_Space"), ("B", "*Paper_Space5"), ("C", "*Paper_Space0"), ("D", "*Paper_Space1")];
    assert_eq!(names, expected.map(|(l, b)| (l.to_string(), b.to_string())));

    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (records, blocks) = block_names(&text);
        for want in ["*Paper_Space", "*Paper_Space5", "*Paper_Space0", "*Paper_Space1"] {
            assert!(records.iter().any(|n| n == want), "{what}: record {want} in {records:?}");
            assert!(blocks.iter().any(|n| n == want), "{what}: block {want} in {blocks:?}");
        }
    }
    // Read back, the layouts name the same blocks, and a second save writes the same names.
    let read = cadcraft_io::read(cadcraft_io::write_dxf(&d).as_bytes(), "x.dxf").unwrap();
    let again: Vec<(String, String)> = read.paper_space_blocks().into_iter().map(|(n, l)| (l.name.clone(), n)).collect();
    assert_eq!(again, expected.map(|(l, b)| (l.to_string(), b.to_string())));
}

#[test]
fn a_layout_lists_its_dictionary_as_a_reactor_as_well_as_its_owner() {
    let text = cadcraft_io::write_dxf(&Drawing::new_metric());
    let layouts: Vec<_> = section(&text, "OBJECTS").into_iter().filter(|(k, _)| k == "LAYOUT").collect();
    assert!(layouts.len() >= 3, "Model and two layouts");
    for (_, tags) in layouts {
        let open = tags.iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
        let owner =
            tags[open..].iter().skip_while(|t| !(t.code == 102 && t.str() == "}")).find(|t| t.code == 330).map(|t| t.str()).unwrap_or_default();
        assert_eq!(tags[open + 1].code, 330);
        assert_eq!(tags[open + 1].str(), owner);
    }
}

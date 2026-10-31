//! An ACAD_TABLE the file drew a graphics block (`*T1`) for keeps that block, with its records and their handles, until the table is edited;
//! then the block is drawn again from the table. AutoCAD's own save keeps the block of an unedited table as it is.

use cadcraft_doc::EntityKind;
use cadcraft_dxf::{records, sections};

const APARTMENT: &[u8] = include_bytes!("../../../../examples/apartment.dxf");

/// The kinds and handles of the records of the anonymous `*T` block of a DXF text, in order.
fn table_block(text: &str) -> Vec<(String, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let blocks = sections(&tags).into_iter().find(|s| s.name == "BLOCKS").unwrap();
    let mut inside = false;
    let mut out = Vec::new();
    for (kind, body) in records(&blocks.tags) {
        match kind.as_str() {
            "BLOCK" => inside = body.iter().any(|t| t.code == 2 && t.str().starts_with("*T")),
            "ENDBLK" => inside = false,
            _ if inside => out.push((kind, body.iter().find(|t| t.code == 5).map(|t| t.str()).unwrap_or_default())),
            _ => {}
        }
    }
    out
}

#[test]
fn an_unedited_table_keeps_its_graphics_block_and_an_edited_one_draws_it_again() {
    let source = table_block(std::str::from_utf8(APARTMENT).unwrap());
    assert!(source.len() > 20, "the sample has a table block of many records: {}", source.len());
    let mut d = cadcraft_io::read_dxf(APARTMENT).unwrap();
    assert_eq!(table_block(&cadcraft_io::write_dxf(&d)), source, "unedited");

    let handle = d.model.iter().find(|e| matches!(e.kind, EntityKind::Table(_))).map(|e| e.handle).expect("a table");
    d.modify_entity(handle, |e| {
        if let EntityKind::Table(t) = &mut e.kind {
            t.cells[2][0].text = "Edited".into();
        }
    })
    .unwrap();
    let edited = table_block(&cadcraft_io::write_dxf(&d));
    assert_ne!(edited, source, "an edited table draws its block again");
    assert!(!edited.is_empty());
}

/// The text height (group 40) of the MTEXT of the `*T` block whose text starts with `text`.
fn mtext_height(dxf: &str, text: &str) -> f64 {
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let blocks = sections(&tags).into_iter().find(|s| s.name == "BLOCKS").unwrap();
    let mut inside = false;
    for (kind, body) in records(&blocks.tags) {
        match kind.as_str() {
            "BLOCK" => inside = body.iter().any(|t| t.code == 2 && t.str().starts_with("*T")),
            "MTEXT" if inside && body.iter().any(|t| matches!(t.code, 1 | 3) && t.str().starts_with(text)) => {
                return body.iter().find(|t| t.code == 40).map(|t| t.str().parse().unwrap()).unwrap();
            }
            _ => {}
        }
    }
    panic!("no MTEXT starting with {text:?} in a *T block")
}

#[test]
fn the_sample_table_draws_its_title_larger_than_its_headings_before_and_after_an_edit() {
    let source = std::str::from_utf8(APARTMENT).unwrap();
    let (title, heading) = (mtext_height(source, "ROOM SCHEDULE"), mtext_height(source, "Room"));
    assert!(title > heading, "the file draws the title ({title}) larger than a heading ({heading})");

    let mut d = cadcraft_io::read_dxf(APARTMENT).unwrap();
    let handle = d.model.iter().find(|e| matches!(e.kind, EntityKind::Table(_))).map(|e| e.handle).expect("a table");
    d.modify_entity(handle, |e| {
        if let EntityKind::Table(t) = &mut e.kind {
            t.cells[2][0].text = "Edited".into();
        }
    })
    .unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    let saved = saved.as_str();
    assert_eq!((mtext_height(saved, "ROOM SCHEDULE"), mtext_height(saved, "Room")), (title, heading), "the edited table keeps the heights of the file");
}

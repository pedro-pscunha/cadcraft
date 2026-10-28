//! AutoCAD uses handle order without ACAD_SORTENTS, not the order of DXF entity records.

use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Cursor;

use cadcraft_doc::{Common, Drawing, EntityKind, EntityStore, Handle, Layout, Line, Space, Viewport};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

fn line(handle: &str, owner: &str) -> String {
    format!("0\nLINE\n5\n{handle}\n330\n{owner}\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n10\n21\n0\n31\n0\n")
}

fn source() -> String {
    let mut text =
        String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n5\n1\n70\n2\n");
    for (owner, name, dictionary) in [("1F", "*Model_Space", "B1"), ("20", "Tree", "B3")] {
        text.push_str(&format!("0\nBLOCK_RECORD\n5\n{owner}\n102\n{{ACAD_XDICTIONARY\n360\n{dictionary}\n102\n}}\n330\n1\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\n{name}\n"));
    }
    text.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n21\n330\n20\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\nTree\n70\n0\n10\n0\n20\n0\n30\n0\n");
    text.push_str(&line("A3", "20"));
    text.push_str(&line("A4", "20"));
    text.push_str("0\nENDBLK\n5\n22\n330\n20\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockEnd\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    text.push_str(&line("A1", "1F"));
    text.push_str(&line("A2", "1F"));
    text.push_str("0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nB5\n330\n0\n100\nAcDbDictionary\n");
    for (owner, dictionary, table, raw, low, high) in [("1F", "B1", "B0", "B6", "A1", "A2"), ("20", "B3", "B2", "B7", "A3", "A4")] {
        text.push_str(&format!("0\nDICTIONARY\n5\n{dictionary}\n330\n{owner}\n100\nAcDbDictionary\n280\n1\n281\n1\n3\nACAD_SORTENTS\n360\n{table}\n3\nARTERRA_NOTE\n360\n{raw}\n0\nXRECORD\n5\n{raw}\n330\n{dictionary}\n100\nAcDbXrecord\n280\n1\n1\nkept\n0\nSORTENTSTABLE\n5\n{table}\n330\n{dictionary}\n100\nAcDbSortentsTable\n330\n{owner}\n331\n{high}\n5\n{low}\n331\n{low}\n5\n{high}\n"));
    }
    text.push_str("0\nENDSEC\n0\nEOF\n");
    text
}

fn order(store: &EntityStore) -> Vec<u64> {
    store.iter().map(|e| e.handle.0).collect()
}

fn assert_order(drawing: &Drawing) {
    assert_eq!(order(&drawing.model), [0xA2, 0xA1]);
    assert_eq!(order(&drawing.block("Tree").unwrap().entities), [0xA4, 0xA3]);
}

fn all_records(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags).into_iter().filter(|s| s.name != "HEADER").flat_map(|s| records(&s.tags)).collect()
}

fn value(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).unwrap().str()
}

fn assert_tables(text: &str) {
    let records = all_records(text);
    let tables: Vec<_> = records.iter().filter(|(kind, _)| kind == "SORTENTSTABLE").collect();
    assert_eq!(tables.len(), 2, "model space and Tree both need explicit draw order");
    let class = &records.iter().find(|(k, t)| k == "CLASS" && value(t, 1) == "SORTENTSTABLE").expect("DXF class declaration").1;
    assert_eq!(value(class, 2), "AcDbSortentsTable");
    assert_eq!(value(class, 91), "2");
    for (name, expected) in [("*Model_Space", vec![(0xA2, 0xA1), (0xA1, 0xA2)]), ("Tree", vec![(0xA4, 0xA3), (0xA3, 0xA4)])] {
        let block = &records.iter().find(|(k, t)| k == "BLOCK_RECORD" && value(t, 2) == name).unwrap().1;
        let owner = value(block, 5);
        assert_eq!(block.iter().filter(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY").count(), 1);
        let dictionary_handle = value(block, 360);
        let dictionary = &records.iter().find(|(k, t)| cadcraft_doc::foreign::is_dictionary(k) && value(t, 5) == dictionary_handle).unwrap().1;
        assert_eq!(value(dictionary, 330), owner);
        assert_eq!(dictionary.iter().filter(|t| t.code == 3 && t.str() == "ACAD_SORTENTS").count(), 1);
        let table_handle = dictionary.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ACAD_SORTENTS").unwrap()[1].str();
        let table = &tables.iter().find(|(_, t)| value(t, 5) == table_handle).unwrap().1;
        assert_eq!(cadcraft_doc::foreign::object_owner(table.iter().map(|t| (t.code, t.str()))).unwrap().hex(), dictionary_handle);
        assert_eq!(table.iter().rev().find(|t| t.code == 330).unwrap().str(), owner);
        assert!(table.iter().any(|t| t.code == 100 && t.str() == "AcDbSortentsTable"));
        let pairs: Vec<_> = table
            .windows(2)
            .filter(|p| p[0].code == 331 && p[1].code == 5)
            .map(|p| (Handle::parse_hex(&p[0].str()).unwrap().0, Handle::parse_hex(&p[1].str()).unwrap().0))
            .collect();
        assert_eq!(pairs, expected);
        let raw_handle = dictionary.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ARTERRA_NOTE").unwrap()[1].str();
        let raw = &records.iter().find(|(k, t)| k == "XRECORD" && value(t, 5) == raw_handle).unwrap().1;
        assert_eq!(value(raw, 330), dictionary_handle);
        assert_eq!(value(raw, 1), "kept");
    }
}

#[test]
fn dxf_keeps_model_and_block_draw_order_with_one_merged_extension_dictionary() {
    let mut drawing = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert_order(&drawing);
    for _ in 0..3 {
        let text = cadcraft_io::write_dxf(&drawing);
        assert_tables(&text);
        drawing = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_order(&drawing);
    }
}

#[test]
fn a_dictionary_with_default_is_reused_without_losing_raw_keys() {
    let source = source().replace("0\nDICTIONARY\n5\nB1\n", "0\nACDBDICTIONARYWDFLT\n5\nB1\n");
    assert!(source.contains("ACDBDICTIONARYWDFLT"));
    let drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert_order(&drawing);
    let text = cadcraft_io::write_dxf(&drawing);
    assert_tables(&text);
    assert_order(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dwg_contains_sort_handle_pairs_and_restores_model_and_block_draw_order() {
    let drawing = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert_order(&drawing);
    let bytes = cadcraft_io::write(&drawing, "x.dwg").unwrap();
    let document = acadrust::DwgReader::from_stream(Cursor::new(bytes.clone())).read().unwrap();
    let tables: Vec<_> = document
        .objects
        .values()
        .filter_map(|o| match o {
            acadrust::objects::ObjectType::SortEntitiesTable(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tables.len(), 2, "DXF bridge must emit OBJ_SORTENTSTABLE into DWG");
    for (name, expected) in [("*Model_Space", vec![(0xA2, 0xA1), (0xA1, 0xA2)]), ("Tree", vec![(0xA4, 0xA3), (0xA3, 0xA4)])] {
        let block = document.block_records.iter().find(|b| b.name == name).unwrap();
        let table = tables.iter().find(|t| t.block_owner_handle == block.handle).unwrap();
        assert_eq!(table.entries().map(|e| (e.entity_handle.value(), e.sort_handle.value())).collect::<Vec<_>>(), expected);
    }
    assert_order(&cadcraft_io::read(&bytes, "x.dwg").unwrap());
}

#[test]
fn handle_order_spaces_need_no_table_even_beside_a_reordered_block() {
    let mut drawing = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    drawing.model.sort_by_key(|e| e.handle);
    let text = cadcraft_io::write_dxf(&drawing);
    let records = all_records(&text);
    let tables: Vec<_> = records.iter().filter(|(k, _)| k == "SORTENTSTABLE").collect();
    assert_eq!(tables.len(), 1, "only Tree needs a table");
    let block = &records.iter().find(|(k, t)| k == "BLOCK_RECORD" && value(t, 2) == "Tree").unwrap().1;
    assert_eq!(tables[0].1.iter().rev().find(|t| t.code == 330).unwrap().str(), value(block, 5));
    let ordered =
        cadcraft_io::read_dxf(source().replace("331\nA2\n5\nA1\n331\nA1\n5\nA2\n", "").replace("331\nA4\n5\nA3\n331\nA3\n5\nA4\n", "").as_bytes())
            .unwrap();
    assert!(!all_records(&cadcraft_io::write_dxf(&ordered)).iter().any(|(k, _)| k == "SORTENTSTABLE"));
}

#[test]
fn every_paper_layout_keeps_draw_order_even_when_the_sheet_viewport_is_emitted_first() {
    let mut drawing = Drawing::new_metric();
    drawing.layouts.push(Layout::new("Second", 2));
    let names: Vec<_> = drawing.layouts.iter().map(|l| l.name.clone()).collect();
    let mut expected = Vec::new();
    for name in &names {
        let space = Space::Paper(name.clone());
        let line = || EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) });
        let low = drawing.add(&space, Common::default(), line()).unwrap();
        let high = drawing.add(&space, Common::default(), line()).unwrap();
        let sheet = drawing.add(&space, Common::default(), EntityKind::Viewport(Viewport { id: 1, ..Viewport::default() })).unwrap();
        drawing.space_mut(&space).unwrap().send_to_back(high);
        expected.push(vec![high.0, low.0, sheet.0]);
    }
    let text = cadcraft_io::write_dxf(&drawing);
    let records = all_records(&text);
    assert_eq!(records.iter().filter(|(k, _)| k == "SORTENTSTABLE").count(), names.len());
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    for (name, order_expected) in names.iter().zip(&expected) {
        assert_eq!(order(&back.layouts.iter().find(|l| &l.name == name).unwrap().entities), *order_expected);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let bytes = cadcraft_io::write(&drawing, "layouts.dwg").unwrap();
        let back = cadcraft_io::read(&bytes, "layouts.dwg").unwrap();
        for (name, order_expected) in names.iter().zip(&expected) {
            assert_eq!(order(&back.layouts.iter().find(|l| &l.name == name).unwrap().entities), *order_expected);
        }
    }
}

#[test]
fn sort_handles_are_references_not_duplicate_object_identities_and_seed_bounds_every_object() {
    let drawing = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&drawing);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let seed = tags.windows(2).find(|p| p[0].code == 9 && p[0].str() == "$HANDSEED").unwrap()[1].str();
    let seed = Handle::parse_hex(&seed).unwrap();
    let mut used = HashSet::new();
    for (kind, body) in all_records(&text) {
        // Only the first 5/105 identifies the object; later 5s in SORTENTSTABLE are sort keys.
        if let Some(tag) = body.iter().find(|t| matches!(t.code, 5 | 105)) {
            let handle = Handle::parse_hex(&tag.str()).unwrap();
            assert!(used.insert(handle), "duplicate object handle {handle:?} on {kind}");
            assert!(handle.0 != 0 && handle.0 < seed.0, "{kind} handle {handle:?} outside HANDSEED {seed:?}");
        }
    }
    assert_tables(&text);
}

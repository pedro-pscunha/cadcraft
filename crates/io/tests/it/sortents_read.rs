//! Read AutoCAD's effective order, not incidental record order or orphan table objects.

use cadcraft_doc::{Drawing, EntityStore};
use cadcraft_dxf::{records, sections};

fn line(handle: &str, owner: &str) -> String {
    format!("0\nLINE\n5\n{handle}\n330\n{owner}\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n")
}

fn source(objects: &str) -> String {
    let mut text = String::from(
        "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n5\n1\n0\nBLOCK_RECORD\n5\n1F\n102\n{ACAD_XDICTIONARY\n360\nB1\n102\n}\n2\n*Model_Space\n0\nBLOCK_RECORD\n5\n20\n2\nTree\n0\nBLOCK_RECORD\n5\n21\n2\n*Paper_Space\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n22\n330\n20\n2\nTree\n70\n0\n10\n0\n20\n0\n",
    );
    text.push_str(&line("A4", "20"));
    text.push_str(&line("A3", "20"));
    text.push_str("0\nENDBLK\n5\n23\n330\n20\n0\nBLOCK\n5\n24\n330\n21\n2\n*Paper_Space\n70\n0\n10\n0\n20\n0\n");
    text.push_str(&line("A6", "21"));
    text.push_str(&line("A5", "21"));
    text.push_str("0\nENDBLK\n5\n25\n330\n21\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    text.push_str(&line("A2", "1F"));
    text.push_str(&line("A1", "1F"));
    text.push_str("0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n");
    text.push_str(objects);
    text.push_str("0\nENDSEC\n0\nEOF\n");
    text
}

fn order(store: &EntityStore) -> Vec<u64> {
    store.iter().map(|e| e.handle.0).collect()
}

fn assert_no_override(drawing: &Drawing) {
    let text = cadcraft_io::write_dxf(drawing);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    assert!(!sections(&tags).into_iter().flat_map(|s| records(&s.tags)).any(|(kind, _)| kind == "SORTENTSTABLE"));
}

#[test]
fn model_without_a_source_table_defaults_to_handle_order() {
    let drawing = cadcraft_io::read_dxf(source("").as_bytes()).unwrap();
    assert_eq!(order(&drawing.model), [0xA1, 0xA2]);
    assert_no_override(&drawing);
}

#[test]
fn paper_without_a_source_table_defaults_to_handle_order() {
    let drawing = cadcraft_io::read_dxf(source("").as_bytes()).unwrap();
    assert_eq!(order(&drawing.layouts[0].entities), [0xA5, 0xA6]);
    assert_no_override(&drawing);
}

#[test]
fn block_without_a_source_table_defaults_to_handle_order() {
    let drawing = cadcraft_io::read_dxf(source("").as_bytes()).unwrap();
    assert_eq!(order(&drawing.block("Tree").unwrap().entities), [0xA3, 0xA4]);
    assert_no_override(&drawing);
}

#[test]
fn damaged_linked_table_recovers_newest_complete_same_dictionary_candidate() {
    let dictionary = "0\nDICTIONARY\n5\nB1\n330\n1F\n100\nAcDbDictionary\n3\nACAD_SORTENTS\n360\nB0\n";
    let damaged = "0\nSORTENTSTABLE\n5\nB0\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n331\nA1\n5\n10\n331\nA2\n5\n10\n";
    let recovered = "0\nSORTENTSTABLE\n5\nB2\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n331\nA1\n5\nA2\n331\nFFFF\n5\n1\n331\nA2\n5\nA1\n";
    for objects in [format!("{dictionary}{damaged}{recovered}"), format!("{dictionary}{recovered}{damaged}")] {
        let drawing = cadcraft_io::read_dxf(source(&objects).as_bytes()).unwrap();
        assert_eq!(order(&drawing.model), [0xA2, 0xA1]);
        assert!(
            drawing.read_notes.iter().any(|note| note.contains("SORTENTSTABLE") && note.contains("B0") && note.contains("B2")),
            "repair must be reported"
        );
    }
}

#[test]
fn damaged_duplicate_sort_keys_keep_first_entry_and_drop_later_entries() {
    let objects = "0\nDICTIONARY\n5\nB1\n330\n1F\n100\nAcDbDictionary\n3\nACAD_SORTENTS\n360\nB0\n0\nSORTENTSTABLE\n5\nB0\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n331\nA2\n5\n10\n331\nA1\n5\n10\n";
    let drawing = cadcraft_io::read_dxf(source(objects).as_bytes()).unwrap();
    assert_eq!(order(&drawing.model), [0xA2, 0xA1]);
    assert!(drawing.read_notes.iter().any(|note| note.contains("SORTENTSTABLE")));
}

#[test]
fn only_the_table_named_by_the_block_extension_dictionary_controls_draw_order() {
    let dictionary = "0\nDICTIONARY\n5\nB1\n330\n1F\n100\nAcDbDictionary\n3\nACAD_SORTENTS\n360\nB0\n";
    let active = "0\nSORTENTSTABLE\n5\nB0\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n331\nA1\n5\nA2\n331\nA2\n5\nA1\n";
    let orphan = "0\nSORTENTSTABLE\n5\nB2\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n331\nA1\n5\nA1\n331\nA2\n5\nA2\n";
    for objects in [format!("{dictionary}{active}{orphan}"), format!("{dictionary}{orphan}{active}")] {
        let drawing = cadcraft_io::read_dxf(source(&objects).as_bytes()).unwrap();
        assert_eq!(order(&drawing.model), [0xA2, 0xA1], "orphan table must be ignored regardless of object record order");
    }
    let drawing = cadcraft_io::read_dxf(source(orphan).as_bytes()).unwrap();
    assert_eq!(order(&drawing.model), [0xA1, 0xA2], "an unreferenced table cannot create an override");
}

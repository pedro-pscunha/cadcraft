//! A save keeps the SORTENTSTABLE of every space the way AutoCAD does: the source table as it was (an
//! empty one too, listed by ascending entity handle) while the space draws in the order it gives,
//! and a table of its own, listed the same way, once the order changed.

use cadcraft_doc::{Drawing, Handle};
use cadcraft_dxf::{Tag, records, sections};

fn line(h: &str) -> String {
    format!("0\nLINE\n5\n{h}\n330\n1F\n8\n0\n10\n0\n20\n0\n30\n0\n11\n10\n21\n0\n31\n0\n")
}

/// A drawing whose model space holds A1..A4 and, when `pairs` is given, a SORTENTSTABLE with them.
fn source(pairs: Option<&str>) -> String {
    let mut s = String::from(
        "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n0\nBLOCK_RECORD\n5\n1F\n102\n{ACAD_XDICTIONARY\n360\nB1\n102\n}\n2\n*Model_Space\n0\nENDTAB\n0\nENDSEC\n",
    );
    s.push_str("0\nSECTION\n2\nENTITIES\n");
    for h in ["A1", "A2", "A3", "A4"] {
        s.push_str(&line(h));
    }
    s.push_str("0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n");
    s.push_str("0\nDICTIONARY\n5\nB1\n102\n{ACAD_REACTORS\n330\n1F\n102\n}\n330\n1F\n100\nAcDbDictionary\n280\n1\n281\n1\n");
    match pairs {
        Some(pairs) => {
            s.push_str("3\nACAD_SORTENTS\n360\nB0\n");
            s.push_str(&format!("0\nSORTENTSTABLE\n5\nB0\n102\n{{ACAD_REACTORS\n330\nB1\n102\n}}\n330\nB1\n100\nAcDbSortentsTable\n330\n1F\n{pairs}"));
        }
        None => {}
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

fn tables(text: &str) -> Vec<Vec<(u64, u64)>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "OBJECTS")
        .flat_map(|s| records(&s.tags))
        .filter(|(kind, _)| kind == "SORTENTSTABLE")
        .map(|(_, body): (String, Vec<Tag>)| {
            body.windows(2)
                .filter(|p| p[0].code == 331 && p[1].code == 5)
                .map(|p| (Handle::parse_hex(&p[0].str()).unwrap().0, Handle::parse_hex(&p[1].str()).unwrap().0))
                .collect()
        })
        .collect()
}

fn order(d: &Drawing) -> Vec<u64> {
    d.model.iter().map(|e| e.handle.0).collect()
}

#[test]
fn an_empty_source_table_is_kept() {
    let d = cadcraft_io::read_dxf(source(Some("")).as_bytes()).unwrap();
    assert_eq!(order(&d), [0xA1, 0xA2, 0xA3, 0xA4]);
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(tables(&text), [vec![]], "the empty table AutoCAD keeps");
    assert_eq!(order(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()), [0xA1, 0xA2, 0xA3, 0xA4]);
}

#[test]
fn a_drawing_with_no_source_table_gets_none_while_it_draws_by_handle() {
    let d = cadcraft_io::read_dxf(source(None).as_bytes()).unwrap();
    assert!(tables(&cadcraft_io::write_dxf(&d)).is_empty());
}

#[test]
fn a_source_table_is_written_as_it_was_with_the_entries_it_listed() {
    // A2 is drawn by A1's sort handle, A3 by A2's, A1 by A3's; A4 is not listed.
    let pairs = "331\nA1\n5\nA3\n331\nA2\n5\nA1\n331\nA3\n5\nA2\n";
    let d = cadcraft_io::read_dxf(source(Some(pairs)).as_bytes()).unwrap();
    assert_eq!(order(&d), [0xA2, 0xA3, 0xA1, 0xA4]);
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(tables(&text), [vec![(0xA1, 0xA3), (0xA2, 0xA1), (0xA3, 0xA2)]]);
    assert_eq!(order(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()), [0xA2, 0xA3, 0xA1, 0xA4]);
}

#[test]
fn a_table_follows_the_new_order_and_lists_the_entities_by_ascending_handle() {
    let pairs = "331\nA1\n5\nA3\n331\nA2\n5\nA1\n331\nA3\n5\nA2\n";
    let mut d = cadcraft_io::read_dxf(source(Some(pairs)).as_bytes()).unwrap();
    d.model.send_to_back(Handle(0xA4));
    assert_eq!(order(&d), [0xA4, 0xA2, 0xA3, 0xA1]);
    let text = cadcraft_io::write_dxf(&d);
    let [table] = &tables(&text)[..] else { panic!("one table") };
    let entities: Vec<u64> = table.iter().map(|p| p.0).collect();
    assert!(entities.is_sorted(), "listed by entity handle: {entities:X?}");
    assert_eq!(order(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()), [0xA4, 0xA2, 0xA3, 0xA1]);
}

#[test]
fn a_deleted_entity_leaves_the_rest_of_its_table() {
    let pairs = "331\nA1\n5\nA3\n331\nA2\n5\nA1\n331\nA3\n5\nA2\n";
    let mut d = cadcraft_io::read_dxf(source(Some(pairs)).as_bytes()).unwrap();
    d.model.remove(Handle(0xA4));
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(tables(&text), [vec![(0xA1, 0xA3), (0xA2, 0xA1), (0xA3, 0xA2)]]);
}

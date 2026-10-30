//! Group 70 of a table counts its records as AutoCAD does: LTYPE without ByBlock and ByLayer, BLOCK_RECORD without *Model_Space and
//! *Paper_Space (AutoCAD's own files: 31 for 33 linetype records, 1648 for 1650 block records).

use super::foreign_passthrough::section;

fn counts(text: &str, table: &str) -> (usize, i64) {
    let tables = section(text, "TABLES");
    let (_, head) = tables.iter().find(|(k, t)| k == "TABLE" && t.iter().any(|g| g.code == 2 && g.str() == table)).unwrap();
    let declared = head.iter().find(|g| g.code == 70).map(|g| g.i64()).unwrap();
    (tables.iter().filter(|(k, _)| k == table).count(), declared)
}

#[test]
fn ltype_and_block_record_counts_leave_out_the_records_of_the_table_control() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    d.blocks.insert("Plan".into(), std::sync::Arc::new(cadcraft_doc::Block::new("Plan")));
    let text = cadcraft_io::write_dxf(&d);
    let (records, declared) = counts(&text, "LTYPE");
    assert_eq!(declared, records as i64 - 2, "LTYPE");
    let (records, declared) = counts(&text, "BLOCK_RECORD");
    assert_eq!(declared, records as i64 - 2, "BLOCK_RECORD");
    for table in ["LAYER", "STYLE", "DIMSTYLE"] {
        let (records, declared) = counts(&text, table);
        assert_eq!(declared, records as i64, "{table}");
    }
}

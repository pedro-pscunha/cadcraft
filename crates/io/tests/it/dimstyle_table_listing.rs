//! The DIMSTYLE table lists some of its styles in the 71 / 340 groups of its header; a save lists the ones the file listed, as long as they exist.

use super::foreign_passthrough::{dxf, val};

fn source(listed: &[&str]) -> String {
    let mut tags: Vec<(i32, &str)> =
        vec![(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "DIMSTYLE"), (5, "A"), (100, "AcDbSymbolTable"), (70, "2"), (100, "AcDbDimStyleTable")];
    let count = listed.len().to_string();
    tags.push((71, &count));
    tags.extend(listed.iter().map(|h| (340, *h)));
    for (handle, name) in [("70", "Standard"), ("71", "Detail")] {
        tags.extend([
            (0, "DIMSTYLE"),
            (105, handle),
            (330, "A"),
            (100, "AcDbSymbolTableRecord"),
            (100, "AcDbDimStyleTableRecord"),
            (2, name),
            (70, "0"),
        ]);
    }
    tags.extend([(0, "ENDTAB"), (0, "ENDSEC"), (0, "EOF")]);
    dxf(&tags)
}

fn listing(text: &str) -> (String, Vec<String>) {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let tables = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "TABLES").unwrap();
    let table = cadcraft_dxf::records(&tables.tags).into_iter().find(|(k, t)| k == "TABLE" && val(t, 2) == "DIMSTYLE").unwrap().1;
    let count = val(&table, 71);
    (count, table.iter().filter(|t| t.code == 340).map(|t| t.str()).collect())
}

#[test]
fn the_dimstyle_table_lists_the_styles_the_file_listed_in_its_order() {
    let d = cadcraft_io::read_dxf(source(&["71"]).as_bytes()).unwrap();
    assert_eq!(listing(&cadcraft_io::write_dxf(&d)), ("1".to_string(), vec!["71".to_string()]));
}

#[test]
fn a_style_that_is_gone_is_not_listed() {
    let mut d = cadcraft_io::read_dxf(source(&["71"]).as_bytes()).unwrap();
    d.dim_styles.retain(|s| s.name != "Detail");
    assert_eq!(listing(&cadcraft_io::write_dxf(&d)), ("0".to_string(), Vec::new()));
}

#[test]
fn a_file_that_listed_none_lists_none() {
    let d = cadcraft_io::read_dxf(source(&[]).as_bytes()).unwrap();
    assert_eq!(listing(&cadcraft_io::write_dxf(&d)), ("0".to_string(), Vec::new()));
}

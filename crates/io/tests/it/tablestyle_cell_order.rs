//! The cell types of a TABLESTYLE are in the order AutoCAD writes them: data, title, header (the
//! TABLESTYLE of an AutoCAD file, `tests/datatable/point_object_id.dxf` of acadrust, has 0.18,
//! 0.25 and 0.18 for them, and acadrust reads and writes that order). A save keeps the title and
//! header styles apart, as user DXF and as DWG.

use cadcraft_color::Color;
use cadcraft_doc::{CellStyle, Drawing};

use super::foreign_passthrough::{dxf, entry, section, val};

/// One cell type as AutoCAD writes it: text style, height, alignment, text colour and a top
/// border colour of its own.
fn cell(height: &'static str, align: &'static str, color: &'static str, border: &'static str) -> Vec<(i32, &'static str)> {
    vec![
        (7, "Standard"),
        (140, height),
        (170, align),
        (62, color),
        (63, "7"),
        (283, "0"),
        (90, "512"),
        (91, "0"),
        (1, ""),
        (274, "-2"),
        (284, "1"),
        (64, border),
        (275, "-2"),
        (285, "1"),
        (65, "0"),
        (276, "-2"),
        (286, "1"),
        (66, "0"),
        (277, "-2"),
        (287, "1"),
        (67, "0"),
        (278, "-2"),
        (288, "1"),
        (68, "0"),
        (279, "-2"),
        (289, "1"),
        (69, "0"),
    ]
}

/// A drawing with the table style `Plan`, written the way AutoCAD does: data (0.18, top centre,
/// colour 3), title (0.25, middle centre, colour 1), header (0.2, middle right, colour 2).
fn autocad_file() -> String {
    let mut g = vec![
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (3, "ACAD_TABLESTYLE"),
        (350, "20"),
        (0, "DICTIONARY"),
        (5, "20"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (3, "Plan"),
        (350, "21"),
        (0, "TABLESTYLE"),
        (5, "21"),
        (330, "20"),
        (100, "AcDbTableStyle"),
        (3, "Plan"),
        (70, "0"),
        (71, "0"),
        (40, "0.06"),
        (41, "0.06"),
        (280, "0"),
        (281, "0"),
    ];
    g.extend(cell("0.18", "2", "3", "13"));
    g.extend(cell("0.25", "5", "1", "11"));
    g.extend(cell("0.2", "6", "2", "12"));
    g.extend([(0, "ENDSEC"), (0, "EOF")]);
    dxf(&g)
}

/// (text height, alignment, text colour, bottom border colour) of cell style `name` of `Plan`.
fn seen(d: &Drawing, name: &str) -> (f64, u8, Color, Color) {
    let cs = d.table_styles.iter().find(|s| s.name == "Plan").expect("the table style Plan").cell_style(name);
    (cs.text_height, cs.align, cs.text_color, cs.borders.top.color)
}

fn assert_styles(d: &Drawing, what: &str) {
    assert_eq!(seen(d, CellStyle::DATA), (0.18, 2, Color::Index(3), Color::Index(13)), "{what}: data");
    assert_eq!(seen(d, CellStyle::TITLE), (0.25, 5, Color::Index(1), Color::Index(11)), "{what}: title");
    assert_eq!(seen(d, CellStyle::HEADER), (0.2, 6, Color::Index(2), Color::Index(12)), "{what}: header");
}

/// The heights (group 140) of the cell types of the TABLESTYLE `Plan` in `text`, in file order.
fn heights(text: &str) -> Vec<String> {
    let objects = section(text, "OBJECTS");
    let handle = objects.iter().map(|(_, t)| entry(t, "Plan")).find(|h| !h.is_empty()).expect("the dictionary names Plan");
    let (_, style) = objects.iter().find(|(k, t)| k == "TABLESTYLE" && val(t, 5) == handle).expect("the table style Plan is saved");
    style.iter().take_while(|t| t.code < 1000).filter(|t| t.code == 140).map(|t| t.str().trim().to_string()).collect()
}

#[test]
fn an_autocad_table_style_keeps_its_title_and_header_apart() {
    let d = cadcraft_io::read_dxf(autocad_file().as_bytes()).unwrap();
    assert_styles(&d, "read");
    // User DXF: AutoCAD's order, and read back the same.
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(heights(&text), ["0.18", "0.25", "0.2"], "data, title, header: {text}");
    assert_styles(&cadcraft_io::read_dxf(text.as_bytes()).unwrap(), "DXF");
    // The documented groups alone (without CADCraft's xdata) say the same.
    let foreign = text.replace("CELLSTYLES", "SOMETHING_ELSE");
    assert_styles(&cadcraft_io::read_dxf(foreign.as_bytes()).unwrap(), "DXF without xdata");
}

#[test]
fn a_dwg_save_keeps_the_title_and_header_of_a_table_style_apart() {
    let d = cadcraft_io::read_dxf(autocad_file().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    // What acadrust holds in the DWG, in AutoCAD's order.
    let bridge = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    assert_eq!(heights(&bridge), ["0.18", "0.25", "0.2"], "data, title, header in the DWG");
    assert_styles(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "DWG");
    let again = cadcraft_io::write(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_styles(&cadcraft_io::read(&again, "x.dwg").unwrap(), "DWG, second save");
}

/// A DWG of a table style that CADCraft did not write (no CADCraft xdata) reads with the right
/// title and header styles.
#[test]
fn a_dwg_table_style_without_cadcraft_xdata_reads_title_and_header_right() {
    let d = cadcraft_io::read_dxf(autocad_file().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let bridge = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    let plain = bridge.replace("CELLSTYLES", "SOMETHING_ELSE");
    assert_styles(&cadcraft_io::read_dxf(plain.as_bytes()).unwrap(), "DWG without xdata");
}

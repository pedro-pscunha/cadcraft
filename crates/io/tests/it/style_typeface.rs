//! A TrueType text style as AutoCAD writes it has an empty font file (group 3) and its typeface in the ACAD xdata; the save keeps the
//! group empty while the font is still that typeface, and names the new font when it changes.

use super::foreign_passthrough::{dxf, section, val};

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "STYLE"),
        (5, "3"),
        (70, "1"),
        (0, "STYLE"),
        (5, "639E8"),
        (330, "3"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbTextStyleTableRecord"),
        (2, "R80-50"),
        (70, "0"),
        (40, "0.1015"),
        (41, "1.0"),
        (50, "0.0"),
        (71, "0"),
        (42, "0.1015"),
        (3, ""),
        (4, ""),
        (1001, "ACAD"),
        (1000, "Hind Regular"),
        (1071, "2"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

fn font_file(text: &str) -> String {
    let (_, tags) = section(text, "TABLES").into_iter().find(|(k, t)| k == "STYLE" && val(t, 2) == "R80-50").unwrap();
    val(&tags, 3)
}

#[test]
fn the_font_file_stays_empty_while_the_font_is_the_typeface() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert_eq!(d.text_style("R80-50").map(|s| s.font.clone()).as_deref(), Some("Hind Regular"));
    assert_eq!(font_file(&cadcraft_io::write_dxf(&d)), "");
    if let Some(s) = d.text_styles.iter_mut().find(|s| s.name == "R80-50") {
        s.font = "arial.ttf".into();
    }
    assert_eq!(font_file(&cadcraft_io::write_dxf(&d)), "arial.ttf");
}

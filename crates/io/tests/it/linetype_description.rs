//! The description of a linetype every drawing has (Continuous) is the file's, also when it is empty.

use super::foreign_passthrough::{dxf, section, val};

#[test]
fn continuous_keeps_the_description_of_its_file() {
    let source = dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LTYPE"),
        (5, "5"),
        (100, "AcDbSymbolTable"),
        (70, "1"),
        (0, "LTYPE"),
        (5, "16"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbLinetypeTableRecord"),
        (2, "Continuous"),
        (70, "0"),
        (3, ""),
        (72, "65"),
        (73, "0"),
        (40, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let (_, ltype) = section(&text, "TABLES").into_iter().find(|(k, t)| k == "LTYPE" && val(t, 2) == "Continuous").unwrap();
    assert_eq!(val(&ltype, 3), "");
}

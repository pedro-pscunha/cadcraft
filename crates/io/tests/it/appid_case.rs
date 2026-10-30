//! A registered application keeps the spelling of its name when a kept object points at its record: the save wrote the upper-case
//! name of the role (`ACADANNOAV` for `AcadAnnoAV`).

use super::foreign_passthrough::{dxf, section, val};

#[test]
fn a_registered_application_keeps_its_case() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "APPID"),
        (5, "9"),
        (70, "1"),
        (0, "APPID"),
        (5, "12"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbRegAppTableRecord"),
        (2, "AcadAnnoAV"),
        (70, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (3, "KEEPME"),
        (350, "50"),
        (0, "XRECORD"),
        (5, "50"),
        (330, "C"),
        (100, "AcDbXrecord"),
        (280, "1"),
        (340, "12"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    let names: Vec<String> = section(&saved, "TABLES").into_iter().filter(|(k, _)| k == "APPID").map(|(_, t)| val(&t, 2)).collect();
    assert!(names.iter().any(|n| n == "AcadAnnoAV"), "{names:?}");
    assert!(!names.iter().any(|n| n == "ACADANNOAV"), "{names:?}");
}

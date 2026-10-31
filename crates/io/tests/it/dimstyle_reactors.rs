//! A DIMSTYLE lists the dimensions that use it among its persistent reactors, and a save keeps them.

use super::foreign_passthrough::{dxf, section, val};

#[test]
fn a_dimstyle_keeps_the_dimension_that_is_its_reactor() {
    let source = dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "DIMSTYLE"),
        (5, "A"),
        (100, "AcDbSymbolTable"),
        (70, "1"),
        (100, "AcDbDimStyleTable"),
        (71, "0"),
        (0, "DIMSTYLE"),
        (105, "70"),
        (102, "{ACAD_REACTORS"),
        (330, "A"),
        (330, "A8"),
        (102, "}"),
        (330, "A"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbDimStyleTableRecord"),
        (2, "Standard"),
        (70, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "DIMENSION"),
        (5, "A8"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbDimension"),
        (2, "*D1"),
        (10, "20"),
        (20, "10"),
        (30, "0"),
        (11, "15"),
        (21, "10"),
        (31, "0"),
        (70, "33"),
        (3, "Standard"),
        (100, "AcDbAlignedDimension"),
        (13, "10"),
        (23, "0"),
        (33, "0"),
        (14, "20"),
        (24, "0"),
        (34, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    {
        let (what, text) = ("DXF", cadcraft_io::write_dxf(&d));
        let (_, style) = section(&text, "TABLES").into_iter().find(|(k, t)| k == "DIMSTYLE" && val(t, 2) == "Standard").unwrap();
        let reactors: Vec<String> = style
            .iter()
            .skip_while(|t| t.code != 102)
            .take_while(|t| !(t.code == 102 && t.str() == "}"))
            .filter(|t| t.code == 330)
            .map(|t| t.str())
            .collect();
        assert!(reactors.iter().any(|r| r == "A8"), "{what}: {reactors:?}");
    }
}

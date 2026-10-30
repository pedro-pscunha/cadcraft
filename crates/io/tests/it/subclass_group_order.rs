//! AutoCAD writes the groups of a subclass in a fixed order and its reader depends on it. Records read from AutoCAD's own order are
//! written in it again: the properties of `AcDbEntity` (transparency 440 before lineweight 370), a text's content (1) before its angle
//! and style, a multiline text's reference column height (46) after its width and its background fill after the line spacing (44),
//! a text style's oblique angle (42) after its vertical flag, a leader's annotation link (340) after its vertices.

use cadcraft_io::{SaveOptions, Version};

use super::foreign_passthrough::dxf;

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "STYLE"),
        (0, "STYLE"),
        (5, "11"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbTextStyleTableRecord"),
        (2, "Note"),
        (70, "0"),
        (40, "0"),
        (41, "1"),
        (50, "0"),
        (71, "0"),
        (42, "2.5"),
        (3, "arial.ttf"),
        (4, ""),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (5, "30"),
        (100, "AcDbEntity"),
        (8, "0"),
        (62, "1"),
        (440, "33554496"),
        (370, "50"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "5"),
        (21, "5"),
        (31, "0"),
        (0, "TEXT"),
        (5, "31"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbText"),
        (10, "1"),
        (20, "1"),
        (30, "0"),
        (40, "2.5"),
        (1, "NOTE"),
        (50, "30"),
        (7, "Note"),
        (100, "AcDbText"),
        (0, "MTEXT"),
        (5, "32"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbMText"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (40, "2.5"),
        (41, "20"),
        (46, "4"),
        (71, "1"),
        (72, "5"),
        (1, "NOTE"),
        (7, "Note"),
        (73, "1"),
        (44, "1"),
        (90, "3"),
        (63, "9"),
        (421, "13158600"),
        (45, "1.1"),
        (441, "0"),
        (0, "LEADER"),
        (5, "33"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLeader"),
        (3, "Standard"),
        (71, "1"),
        (72, "0"),
        (73, "3"),
        (74, "0"),
        (75, "0"),
        (76, "2"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (10, "5"),
        (20, "5"),
        (30, "0"),
        (340, "32"),
        (211, "1"),
        (221, "0"),
        (231, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

/// The group codes of the subclass `marker` of the first record `kind` of a DXF (the section it is in does not matter).
fn codes(bytes: &[u8], kind: &str, marker: &str) -> Vec<i32> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let record = tags.iter().position(|t| t.code == 0 && t.str() == kind).unwrap();
    let start = record + tags[record..].iter().position(|t| t.code == 100 && t.str() == marker).unwrap() + 1;
    tags[start..].iter().take_while(|t| t.code != 100 && t.code != 0 && t.code < 1000).map(|t| t.code).collect()
}

#[test]
fn a_record_keeps_autocads_group_order_in_its_subclasses() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for version in [Version::R2010, Version::R2018] {
        let bytes = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
        // The groups of the source the writer also writes, in the source's order.
        for (kind, marker, want) in [
            ("LINE", "AcDbEntity", vec![8, 62, 440, 370]),
            ("TEXT", "AcDbText", vec![10, 20, 30, 40, 1, 50, 7]),
            ("MTEXT", "AcDbMText", vec![10, 20, 30, 40, 41, 46, 71, 72, 1, 7, 73, 44, 90, 63, 421, 45, 441]),
            ("STYLE", "AcDbTextStyleTableRecord", vec![2, 70, 40, 41, 50, 71, 42, 3, 4]),
            ("LEADER", "AcDbLeader", vec![3, 73, 74, 75, 76, 10, 10, 340, 211]),
        ] {
            let written = codes(&bytes, kind, marker);
            let ordered: Vec<i32> = written.iter().copied().filter(|c| want.contains(c)).collect();
            assert_eq!(ordered, want, "{version:?} {kind} [{marker}]: {written:?}");
        }
    }
}

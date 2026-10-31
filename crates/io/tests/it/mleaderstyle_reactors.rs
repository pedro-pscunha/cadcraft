//! An MLEADERSTYLE lists the multileaders that use it among its persistent reactors, and a save keeps them.

use super::foreign_passthrough::{dxf, section, val};

#[test]
fn an_mleaderstyle_keeps_the_entity_that_is_its_reactor() {
    let source = dxf(&[
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (100, "AcDbDictionary"),
        (3, "ACAD_MLEADERSTYLE"),
        (350, "D"),
        (0, "DICTIONARY"),
        (5, "D"),
        (102, "{ACAD_REACTORS"),
        (330, "C"),
        (102, "}"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (3, "Standard"),
        (350, "E"),
        (0, "MLEADERSTYLE"),
        (5, "E"),
        (102, "{ACAD_REACTORS"),
        (330, "D"),
        (330, "A8"),
        (102, "}"),
        (330, "D"),
        (100, "AcDbMLeaderStyle"),
        (170, "2"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (5, "A8"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (11, "5"),
        (21, "0"),
        (31, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let (_, style) =
        section(&text, "OBJECTS").into_iter().find(|(k, t)| k == "MLEADERSTYLE" && val(t, 5) == "E").expect("the style is saved under its handle");
    let reactors: Vec<String> = style
        .iter()
        .skip_while(|t| t.code != 102)
        .take_while(|t| !(t.code == 102 && t.str() == "}"))
        .filter(|t| t.code == 330)
        .map(|t| t.str())
        .collect();
    assert!(reactors.iter().any(|r| r == "A8"), "{reactors:?}");
}

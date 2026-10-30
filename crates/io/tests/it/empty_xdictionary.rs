//! An extension dictionary with no entry (AutoCAD keeps one on a text, a layer or a block record that once had data in it) stays on its
//! owner through a save, as DXF and as DWG.

use super::foreign_passthrough::{dxf, saved_twice, section, val};

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "MTEXT"),
        (5, "30"),
        (102, "{ACAD_XDICTIONARY"),
        (360, "50"),
        (102, "}"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbMText"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (40, "2"),
        (41, "10"),
        (71, "1"),
        (72, "5"),
        (1, "Text"),
        (0, "LINE"),
        (5, "31"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (281, "1"),
        (0, "DICTIONARY"),
        (5, "50"),
        (330, "30"),
        (100, "AcDbDictionary"),
        (281, "1"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

#[test]
fn an_empty_extension_dictionary_stays_on_its_text_and_a_line_gets_none() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let entities = section(&text, "ENTITIES");
        let mtext = &entities.iter().find(|(k, _)| k == "MTEXT").expect("the MTEXT").1;
        let at =
            mtext.iter().position(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY").unwrap_or_else(|| panic!("{what}: no extension dictionary"));
        let dictionary = mtext[at + 1].str();
        let objects = section(&text, "OBJECTS");
        let (_, own) = objects
            .iter()
            .find(|(k, t)| k == "DICTIONARY" && val(t, 5) == dictionary)
            .unwrap_or_else(|| panic!("{what}: dictionary {dictionary} missing"));
        assert!(own.iter().all(|t| t.code != 3), "{what}: it has no entry");
        let line = &entities.iter().find(|(k, _)| k == "LINE").expect("the LINE").1;
        assert!(line.iter().all(|t| t.code != 360), "{what}: a LINE with no dictionary got one");
    }
}

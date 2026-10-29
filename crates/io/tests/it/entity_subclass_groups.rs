//! A VIEWPORT and a LEADER keep the properties of their `AcDbEntity` subclass (material 347,
//! shadow mode 284, colour name 430) there, and the groups of their own subclass (a viewport's
//! view and UCS, a leader's hook line and annotation link) in `AcDbViewport` and `AcDbLeader`,
//! after a save as DXF and as DWG. The Autodesk DXF reference puts each group in one subclass.

use cadcraft_dxf::Tag;

use super::foreign_passthrough::{dxf, saved_twice, section, subclass, val};

/// A paper space viewport and a model space leader, each with a material (the MATERIAL `Concrete`,
/// handle 3A), a shadow mode, a true colour with its colour name, and groups of its own subclass.
fn fixture() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "VIEWPORT"),
        (5, "40"),
        (100, "AcDbEntity"),
        (67, "1"),
        (8, "0"),
        (420, "16711680"),
        (430, "DIC 226"),
        (347, "3A"),
        (284, "1"),
        (100, "AcDbViewport"),
        (10, "50"),
        (20, "40"),
        (30, "0"),
        (40, "80"),
        (41, "60"),
        (68, "2"),
        (69, "2"),
        (12, "10"),
        (22, "10"),
        (16, "1"),
        (26, "0"),
        (36, "0"),
        (42, "75"),
        (45, "50"),
        (79, "5"),
        (90, "32768"),
        (110, "1"),
        (120, "2"),
        (130, "3"),
        (0, "LEADER"),
        (5, "60"),
        (100, "AcDbEntity"),
        (8, "0"),
        (420, "255"),
        (430, "DIC 254"),
        (347, "3A"),
        (284, "2"),
        (100, "AcDbLeader"),
        (3, "Standard"),
        (71, "1"),
        (72, "0"),
        (73, "3"),
        (74, "1"),
        (75, "1"),
        (40, "2.5"),
        (41, "10"),
        (76, "2"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (10, "5"),
        (20, "5"),
        (30, "0"),
        (211, "1"),
        (221, "0"),
        (231, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (3, "ACAD_MATERIAL"),
        (350, "3B"),
        (0, "DICTIONARY"),
        (5, "3B"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (3, "Concrete"),
        (350, "3A"),
        (0, "MATERIAL"),
        (5, "3A"),
        (330, "3B"),
        (100, "AcDbMaterial"),
        (1, "Concrete"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

/// The groups of `entity` (VIEWPORT or LEADER) in subclass `name`, as codes.
fn codes(entity: &[Tag], name: &str) -> Vec<i32> {
    subclass(entity, name).iter().map(|(c, _)| *c).collect()
}

fn assert_subclasses(text: &str, what: &str) {
    let ents = section(text, "ENTITIES");
    let objects = section(text, "OBJECTS");
    let material = |entity: &[Tag]| {
        let h = val(entity, 347);
        objects.iter().find(|(k, t)| k == "MATERIAL" && val(t, 5) == h).map(|(_, t)| val(t, 1))
    };
    for (kind, own, view) in [("VIEWPORT", "AcDbViewport", [16, 42, 79, 110]), ("LEADER", "AcDbLeader", [73, 74, 75, 211])] {
        // The writer adds a viewport of its own for the sheet, which has no colour name.
        let entity = ents.iter().filter(|(k, t)| k == kind && t.iter().any(|t| t.code == 430)).map(|(_, t)| t.as_slice()).next();
        let entity = entity.unwrap_or_else(|| panic!("{what}: {kind} with its colour name is saved: {text}"));
        let (common, own_codes) = (codes(entity, "AcDbEntity"), codes(entity, own));
        for code in [347, 284, 430] {
            assert!(common.contains(&code), "{what}: {kind} keeps {code} in AcDbEntity: {common:?}");
            assert!(!own_codes.contains(&code), "{what}: {kind} has {code} in {own}: {own_codes:?}");
        }
        for code in view {
            assert!(own_codes.contains(&code), "{what}: {kind} keeps {code} in {own}: {own_codes:?}");
            assert!(!common.contains(&code), "{what}: {kind} has {code} in AcDbEntity: {common:?}");
        }
        assert_eq!(material(entity).as_deref(), Some("Concrete"), "{what}: {kind}'s material");
    }
}

#[test]
fn viewport_and_leader_keep_each_group_in_its_subclass() {
    let d = cadcraft_io::read_dxf(fixture().as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let what = ["DXF", "DWG"][i];
        assert_subclasses(&text, what);
        let again = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_subclasses(&cadcraft_io::write_dxf(&again), &format!("{what}, second save"));
    }
}

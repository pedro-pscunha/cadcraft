//! An INSERT's attributes have fresh handles on save, but keep their field dictionaries,
//! reactors and other applications' xdata by their role within that INSERT.

use cadcraft_doc::EntityKind;
use cadcraft_dxf::{Tag, records, sections};

fn field_dxf() -> String {
    let groups: &[(i32, &str)] = &[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1032"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "CLASSES"),
        (0, "CLASS"),
        (1, "FIELD"),
        (2, "AcDbField"),
        (3, "ObjectDBX Classes"),
        (90, "0"),
        (91, "2"),
        (280, "0"),
        (281, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (5, "20"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockBegin"),
        (2, "LABEL"),
        (70, "2"),
        (10, "0"),
        (20, "0"),
        (3, "LABEL"),
        (0, "ATTDEF"),
        (5, "21"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbText"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (1, "default"),
        (100, "AcDbAttributeDefinition"),
        (3, "Length"),
        (2, "LENGTH"),
        (70, "0"),
        (0, "ENDBLK"),
        (5, "22"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (5, "30"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "0"),
        (0, "INSERT"),
        (5, "31"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockReference"),
        (66, "1"),
        (2, "LABEL"),
        (10, "0"),
        (20, "0"),
        // A preceding attribute with the same tag must not steal the live one's extras.
        (0, "ATTRIB"),
        (5, "32"),
        (330, "31"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbText"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (1, "plain"),
        (100, "AcDbAttribute"),
        (2, "LENGTH"),
        (70, "0"),
        (0, "ATTRIB"),
        (5, "33"),
        (102, "{ACAD_REACTORS"),
        (330, "43"),
        (102, "}"),
        (102, "{ACAD_XDICTIONARY"),
        (360, "40"),
        (102, "}"),
        (330, "31"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbText"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (1, "live"),
        (100, "AcDbAttribute"),
        (2, "length"),
        (70, "0"),
        (1001, "FOREIGN_FIELDS"),
        (1000, "attribute data"),
        (1005, "33"),
        (0, "SEQEND"),
        (5, "34"),
        (330, "31"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (0, "DICTIONARY"),
        (5, "40"),
        (330, "33"),
        (100, "AcDbDictionary"),
        (280, "1"),
        (3, "ACAD_FIELD"),
        (360, "41"),
        (0, "DICTIONARY"),
        (5, "41"),
        (330, "40"),
        (100, "AcDbDictionary"),
        (280, "1"),
        (3, "TEXT"),
        (360, "42"),
        (0, "FIELD"),
        (5, "42"),
        (330, "41"),
        (100, "AcDbField"),
        (1, "_text"),
        (2, "c=%<\\_FldIdx 0>%m / i=Y%"),
        (90, "1"),
        (360, "43"),
        (97, "0"),
        (91, "0"),
        (92, "0"),
        (94, "0"),
        (95, "0"),
        (93, "0"),
        (0, "FIELD"),
        (5, "43"),
        (330, "42"),
        (100, "AcDbField"),
        (1, "AcObjProp"),
        (2, "\\AcObjProp Object(%<\\_ObjIdx 0>%).Length"),
        (90, "0"),
        (97, "1"),
        (331, "30"),
        (91, "0"),
        (92, "0"),
        (94, "0"),
        (95, "0"),
        (93, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

fn section(dxf: &str, name: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == name).unwrap();
    records(&section.tags)
}

fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

fn assert_fields(dxf: &str) {
    let ents = section(dxf, "ENTITIES");
    let find_attr = |text: &str| &ents.iter().find(|(k, t)| k == "ATTRIB" && val(t, 1) == text).unwrap().1;
    let plain = find_attr("plain");
    let attr = find_attr("live");
    assert!(!plain.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"));
    assert!(attr.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "ATTRIB lost its extension dictionary");
    let objs = section(dxf, "OBJECTS");
    let object = |h: String| &objs.iter().find(|(_, t)| val(t, 5) == h).unwrap().1;
    let xdict = object(val(attr, 360));
    assert_eq!(val(xdict, 330), val(attr, 5));
    assert_eq!(val(xdict, 3), "ACAD_FIELD");
    let fields = object(val(xdict, 360));
    assert_eq!(val(fields, 330), val(xdict, 5));
    assert_eq!(val(fields, 3), "TEXT", "{dxf}");
    assert_eq!(objs.iter().filter(|(k, _)| k == "FIELD").count(), 2);
    let parent = object(val(fields, 360));
    assert_eq!(val(parent, 330), val(fields, 5));
    assert_eq!(val(parent, 1), "_text");
    assert_eq!(val(parent, 2), "c=%<\\_FldIdx 0>%m / i=Y%");
    let child = object(val(parent, 360));
    assert_eq!(val(child, 330), val(parent, 5));
    assert_eq!(val(child, 1), "AcObjProp");
    assert_eq!(val(child, 2), "\\AcObjProp Object(%<\\_ObjIdx 0>%).Length");
    let line = &ents.iter().find(|(k, _)| k == "LINE").unwrap().1;
    assert_eq!(val(child, 331), val(line, 5));
    assert!(attr.iter().any(|t| t.code == 102 && t.str() == "{ACAD_REACTORS"));
    assert!(attr.iter().any(|t| t.code == 330 && t.str() == val(child, 5)));
    assert_eq!(val(attr, 1001), "FOREIGN_FIELDS");
    assert_eq!(val(attr, 1000), "attribute data");
    assert_eq!(val(attr, 1005), val(attr, 5));
}

#[test]
fn attrib_fields_survive_dxf_save() {
    let d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    assert_fields(&saved);
    let back = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_fields(&cadcraft_io::write_dxf(&back));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn attrib_fields_survive_dwg_save() {
    let d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let dxf = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
    assert_fields(&String::from_utf8_lossy(&dxf));
}

#[test]
fn removed_attrib_or_insert_drops_its_fields() {
    for remove_insert in [false, true] {
        let mut d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
        let h = d.model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
        if remove_insert {
            d.model.remove(h);
        } else {
            d.model.modify(h, |e| {
                if let EntityKind::Insert(i) = &mut e.kind {
                    i.attribs.retain(|a| a.text.value != "live");
                }
            });
        }
        let saved = cadcraft_io::write_dxf(&d);
        assert!(section(&saved, "OBJECTS").iter().all(|(k, _)| k != "FIELD"));
        assert!(section(&saved, "ENTITIES").iter().filter(|(k, _)| k == "ATTRIB").all(|(_, t)| !t.iter().any(|g| g.code == 1001 || g.code == 102)));
    }
}

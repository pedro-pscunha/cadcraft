//! What AutoCAD stores on objects CADCraft writes from its model (entities, layouts, styles,
//! layers) but does not model stays through a save, as DXF and as DWG: annotative state,
//! extension dictionaries, xdata, the unused plot style names, and the layout groups CADCraft
//! has no field for. Synthetic files only; the groups follow the DXF Reference.

use cadcraft_doc::Drawing;
use cadcraft_dxf::{Tag, records, sections};

/// A DXF text from (code, value) groups.
fn dxf(groups: &[(i32, &str)]) -> String {
    groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// The records of section `name` in a DXF text.
fn section(text: &str, name: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags).into_iter().find(|s| s.name == name).map(|s| records(&s.tags)).unwrap_or_default()
}

fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

/// The handle a dictionary names `key` (a soft or a hard owner entry).
fn entry(dictionary: &[Tag], key: &str) -> String {
    let at = dictionary.iter().position(|t| t.code == 3 && t.str() == key);
    at.and_then(|i| dictionary.get(i + 1)).filter(|t| matches!(t.code, 350 | 360)).map(Tag::str).unwrap_or_default()
}

/// The `1001` application names of a record, in order.
fn apps(tags: &[Tag]) -> Vec<String> {
    tags.iter().filter(|t| t.code == 1001).map(Tag::str).collect()
}

/// The text of a drawing's DXF after a save to DWG and a reopen: what a DWG keeps.
fn through_dwg(d: &Drawing) -> String {
    let dwg = cadcraft_io::write(d, "x.dwg").unwrap();
    cadcraft_io::write_dxf(&cadcraft_io::read(&dwg, "x.dwg").unwrap())
}

/// Every text a drawing is checked in: its DXF save and the DXF of its DWG save.
fn saved_twice(d: &Drawing) -> [String; 2] {
    [cadcraft_io::write_dxf(d), through_dwg(d)]
}

/// An annotative LINE the way AutoCAD stores it: `AcadAnnotative` xdata and an extension
/// dictionary whose `AcDbContextDataManager` holds the scale representations (an XRECORD stands
/// for them here).
fn annotative_line() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (5, "30"),
        (102, "{ACAD_XDICTIONARY"),
        (360, "50"),
        (102, "}"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLine"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "0"),
        (1001, "AcadAnnotative"),
        (1000, "AnnotativeData"),
        (1002, "{"),
        (1070, "1"),
        (1070, "1"),
        (1002, "}"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (0, "DICTIONARY"),
        (5, "50"),
        (330, "30"),
        (100, "AcDbDictionary"),
        (3, "AcDbContextDataManager"),
        (360, "51"),
        (0, "DICTIONARY"),
        (5, "51"),
        (330, "50"),
        (100, "AcDbDictionary"),
        (3, "ACDB_ANNOTATIONSCALES"),
        (350, "52"),
        (0, "XRECORD"),
        (5, "52"),
        (330, "51"),
        (100, "AcDbXrecord"),
        (280, "1"),
        (1, "scale context"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

/// The annotative LINE of a saved text keeps its xdata once, and its extension dictionary still
/// leads to the context data manager.
fn assert_annotative_kept(text: &str) {
    let ents = section(text, "ENTITIES");
    let (_, line) = ents.iter().find(|(k, _)| k == "LINE").expect("the line is saved");
    assert_eq!(apps(line), ["AcadAnnotative"], "{text}");
    let ones: Vec<i64> = line.iter().filter(|t| t.code == 1070).map(Tag::i64).collect();
    assert_eq!(ones, [1, 1]);
    let objs = section(text, "OBJECTS");
    let by_handle = |h: &str| objs.iter().find(|(_, t)| val(t, 5) == h).map(|(_, t)| t.clone());
    let xdict = by_handle(&val(line, 360)).expect("the line's extension dictionary is saved");
    assert_eq!(val(&xdict, 330), val(line, 5));
    let manager = by_handle(&entry(&xdict, "AcDbContextDataManager")).expect("the context data manager is saved");
    let scales = by_handle(&entry(&manager, "ACDB_ANNOTATIONSCALES")).expect("the scale representations are saved");
    assert_eq!(val(&scales, 1), "scale context");
}

#[test]
fn annotative_entity_keeps_its_state_and_scale_dictionary() {
    let d = cadcraft_io::read_dxf(annotative_line().as_bytes()).unwrap();
    for text in saved_twice(&d) {
        assert_annotative_kept(&text);
    }
}

/// Annotative text styles are modelled (written once); an annotative block definition is not, and
/// keeps its flag as read.
#[test]
fn annotative_style_is_written_once_and_annotative_block_keeps_its_flag() {
    let annotative = [(1001, "AcadAnnotative"), (1000, "AnnotativeData"), (1002, "{"), (1070, "1"), (1070, "1"), (1002, "}")];
    let mut groups: Vec<(i32, &str)> = vec![
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "STYLE"),
        (0, "STYLE"),
        (5, "40"),
        (2, "Note"),
        (70, "0"),
        (40, "0"),
        (41, "1"),
        (50, "0"),
        (71, "0"),
        (42, "0.2"),
        (3, "txt"),
        (4, ""),
    ];
    groups.extend(annotative);
    groups.extend([(0, "ENDTAB"), (0, "TABLE"), (2, "BLOCK_RECORD"), (0, "BLOCK_RECORD"), (5, "41"), (2, "TAG"), (340, "0")]);
    groups.extend(annotative);
    groups.extend([
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (5, "42"),
        (8, "0"),
        (2, "TAG"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (3, "TAG"),
        (0, "LINE"),
        (5, "43"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
        (0, "ENDBLK"),
        (5, "44"),
        (8, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(dxf(&groups).as_bytes()).unwrap();
    assert!(d.text_styles.iter().any(|s| s.name == "Note" && s.annotative));
    for text in saved_twice(&d) {
        let tables = section(&text, "TABLES");
        let named = |kind: &str, name: &str| tables.iter().find(|(k, t)| k == kind && val(t, 2) == name).map(|(_, t)| t.clone()).unwrap();
        assert_eq!(apps(&named("STYLE", "Note")), ["AcadAnnotative"], "{text}");
        assert_eq!(apps(&named("BLOCK_RECORD", "TAG")), ["AcadAnnotative"], "{text}");
    }
}

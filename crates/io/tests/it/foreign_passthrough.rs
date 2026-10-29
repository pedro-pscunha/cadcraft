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

/// Annotative text styles are modelled (written once); a style whose flag is off, and an annotative
/// block definition, are not, and keep their data as read.
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
    // A style marked and then switched off keeps its data with the flag 0.
    groups.extend([(0, "STYLE"), (5, "45"), (2, "Plain"), (70, "0"), (40, "0"), (41, "1"), (50, "0"), (71, "0"), (42, "0.2"), (3, "txt"), (4, "")]);
    groups.extend([(1001, "AcadAnnotative"), (1000, "AnnotativeData"), (1002, "{"), (1070, "1"), (1070, "0"), (1002, "}")]);
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
        let plain = named("STYLE", "Plain");
        assert_eq!(
            (apps(&plain), plain.iter().filter(|t| t.code == 1070).map(Tag::i64).collect::<Vec<_>>()),
            (vec!["AcadAnnotative".to_string()], vec![1, 0]),
            "{text}"
        );
        assert_eq!(apps(&named("BLOCK_RECORD", "TAG")), ["AcadAnnotative"], "{text}");
    }
}

/// A file with a layout, a multileader style, a table style and a scale as AutoCAD writes them:
/// each has an extension dictionary holding an XRECORD, and xdata CADCraft has no field for.
fn styled_objects() -> String {
    let dictionary = |h: &'static str, owner: &'static str, entries: &[(&'static str, &'static str)]| -> Vec<(i32, &'static str)> {
        let mut g = vec![(0, "DICTIONARY"), (5, h), (330, owner), (100, "AcDbDictionary"), (281, "1")];
        for (key, target) in entries {
            g.extend([(3, *key), (350, *target)]);
        }
        g
    };
    let xrecord = |h: &'static str, owner: &'static str, text: &'static str| -> Vec<(i32, &'static str)> {
        vec![(0, "XRECORD"), (5, h), (330, owner), (100, "AcDbXrecord"), (280, "1"), (1, text)]
    };
    let xdict = [(102, "{ACAD_XDICTIONARY"), (360, ""), (102, "}")];
    let with_xdict = |mut head: Vec<(i32, &'static str)>, handle: &'static str| {
        let mut x = xdict;
        x[1].1 = handle;
        head.splice(2..2, x);
        head
    };
    let mut g: Vec<(i32, &str)> = vec![(0, "SECTION"), (2, "OBJECTS")];
    g.extend(dictionary("C", "0", &[("ACAD_LAYOUT", "20"), ("ACAD_MLEADERSTYLE", "21"), ("ACAD_TABLESTYLE", "22"), ("ACAD_SCALELIST", "23")]));
    g.extend(dictionary("20", "C", &[("A4", "31")]));
    g.extend(dictionary("21", "C", &[("Callout", "41")]));
    g.extend(dictionary("22", "C", &[("Schedule", "51")]));
    g.extend(dictionary("23", "C", &[("A0", "61")]));
    g.extend(with_xdict(
        vec![
            (0, "LAYOUT"),
            (5, "31"),
            (330, "20"),
            (100, "AcDbPlotSettings"),
            (1, ""),
            (2, "none_device"),
            (4, "ISO_A4_(297.00_x_210.00_MM)"),
            (6, ""),
            (70, "688"),
            (100, "AcDbLayout"),
            (1, "A4"),
            (70, "1"),
            (71, "1"),
            (330, "30"),
            (1001, "ACAD_PSEXT"),
            (1000, "DWG To PDF.pc3"),
            (1001, "PLOTTRANSPARENCY"),
            (1071, "1"),
        ],
        "32",
    ));
    g.extend(dictionary("32", "31", &[("ADSK_XREC_LAYOUTTHUMBNAIL", "33")]));
    g.extend(xrecord("33", "32", "thumbnail"));
    g.extend(with_xdict(vec![(0, "MLEADERSTYLE"), (5, "41"), (330, "21"), (100, "AcDbMLeaderStyle"), (1001, "ACAD_MLEADERVER"), (1070, "2")], "42"));
    g.extend(dictionary("42", "41", &[("ACAD_XREC_ROUNDTRIP", "43")]));
    g.extend(xrecord("43", "42", "roundtrip"));
    g.extend(with_xdict(vec![(0, "TABLESTYLE"), (5, "51"), (330, "22"), (100, "AcDbTableStyle")], "52"));
    g.extend(dictionary("52", "51", &[("ACAD_ROUNDTRIP_2008_TABLESTYLE_CELLSTYLEMAP", "53")]));
    g.extend(xrecord("53", "52", "cellstylemap"));
    g.extend([
        (0, "SCALE"),
        (5, "61"),
        (330, "23"),
        (100, "AcDbScale"),
        (70, "0"),
        (300, "A0"),
        (140, "1"),
        (141, "50"),
        (290, "0"),
        (1001, "ACAD"),
        (1070, "1"),
    ]);
    g.extend([(0, "ENDSEC"), (0, "EOF")]);
    dxf(&g)
}

/// The object a saved text's named object dictionary leads to by `dictionary` and `name`
/// (`ACAD_LAYOUT`, `A4`), and the XRECORD text its extension dictionary holds under `key`.
fn object_with_xrecord(text: &str, dictionary: &str, name: &str, key: &str) -> (Vec<Tag>, String) {
    let objs = section(text, "OBJECTS");
    let by_handle = |h: &str| objs.iter().find(|(_, t)| val(t, 5) == h).map(|(_, t)| t.clone());
    let root = &objs.first().expect("the named object dictionary").1;
    let dict = by_handle(&entry(root, dictionary)).unwrap_or_else(|| panic!("no {dictionary}"));
    let object = by_handle(&entry(&dict, name)).unwrap_or_else(|| panic!("no {name} in {dictionary}"));
    let xdict = by_handle(&val(&object, 360)).unwrap_or_else(|| panic!("{dictionary} {name} lost its extension dictionary:\n{text}"));
    assert_eq!(val(&xdict, 330), val(&object, 5), "the extension dictionary names its owner");
    let record = by_handle(&entry(&xdict, key)).unwrap_or_else(|| panic!("{dictionary} {name} lost {key}"));
    (object, val(&record, 1))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The saved text of a layout, a multileader style, a table style and a scale keeps the xdata
/// CADCraft has no field for, and their extension dictionaries. A DWG keeps those of a layout and
/// the scale's xdata; acadrust's DWG objects for multileader and table styles carry neither.
#[test]
fn layout_styles_and_scales_keep_their_xdata_and_extension_dictionaries() {
    let d = cadcraft_io::read_dxf(styled_objects().as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let in_dxf = i == 0;
        let (layout, thumb) = object_with_xrecord(&text, "ACAD_LAYOUT", "A4", "ADSK_XREC_LAYOUTTHUMBNAIL");
        assert_eq!(thumb, "thumbnail");
        assert_eq!(sorted(apps(&layout)), ["ACAD_PSEXT", "PLOTTRANSPARENCY"], "{text}");
        let objs = section(&text, "OBJECTS");
        let scale = objs.iter().find(|(k, t)| k == "SCALE" && val(t, 300) == "A0").map(|(_, t)| t.clone()).expect("the scale");
        assert_eq!(apps(&scale), ["ACAD"]);
        if in_dxf {
            let (style, roundtrip) = object_with_xrecord(&text, "ACAD_MLEADERSTYLE", "Callout", "ACAD_XREC_ROUNDTRIP");
            assert_eq!((apps(&style), roundtrip.as_str()), (vec!["ACAD_MLEADERVER".to_string()], "roundtrip"));
            let (_, cells) = object_with_xrecord(&text, "ACAD_TABLESTYLE", "Schedule", "ACAD_ROUNDTRIP_2008_TABLESTYLE_CELLSTYLEMAP");
            assert_eq!(cells, "cellstylemap");
        }
    }
}

/// Two layers with the xdata AutoCAD gives layers: `Opaque` has a transparency of 0 and a layer
/// standard without description (nothing the model writes), `Glass` a transparency and a description.
fn layer_xdata() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (70, "2"),
        (0, "LAYER"),
        (5, "50"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbLayerTableRecord"),
        (2, "Opaque"),
        (70, "0"),
        (62, "7"),
        (6, "Continuous"),
        (1001, "AcCmTransparency"),
        (1071, "33554687"),
        (1001, "AcAecLayerStandard"),
        (1000, ""),
        (1000, ""),
        (0, "LAYER"),
        (5, "51"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbLayerTableRecord"),
        (2, "Glass"),
        (70, "0"),
        (62, "7"),
        (6, "Continuous"),
        (1001, "AcCmTransparency"),
        (1071, "33554559"),
        (1001, "AcAecLayerStandard"),
        (1000, ""),
        (1000, "Glass note"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

/// The groups of the `app` xdata of a record (after its `1001` up to the next).
fn app_groups(tags: &[Tag], app: &str) -> Vec<(i32, String)> {
    let Some(start) = tags.iter().position(|t| t.code == 1001 && t.str() == app) else { return Vec::new() };
    tags.iter().skip(start + 1).take_while(|t| t.code != 1001).map(|t| (t.code, t.str())).collect()
}

#[test]
fn layer_xdata_the_model_does_not_reproduce_is_kept_and_the_rest_written_once() {
    let d = cadcraft_io::read_dxf(layer_xdata().as_bytes()).unwrap();
    let glass = d.layers.iter().find(|l| l.name == "Glass").unwrap();
    assert_eq!((glass.transparency, glass.description.as_str()), (50, "Glass note"));
    for text in saved_twice(&d) {
        let tables = section(&text, "TABLES");
        let layer = |name: &str| tables.iter().find(|(k, t)| k == "LAYER" && val(t, 2) == name).map(|(_, t)| t.clone()).unwrap();
        let (opaque, glass) = (layer("Opaque"), layer("Glass"));
        let strings = |tags: &[Tag], app: &str| app_groups(tags, app).into_iter().map(|(_, v)| v).collect::<Vec<_>>();
        assert_eq!(apps(&glass), ["AcCmTransparency", "AcAecLayerStandard"], "written once, from the model: {text}");
        assert_eq!(strings(&glass, "AcCmTransparency"), ["33554559"]);
        assert_eq!(strings(&glass, "AcAecLayerStandard"), ["", "Glass note"]);
        assert_eq!(apps(&opaque), ["AcCmTransparency", "AcAecLayerStandard"], "kept as read: {text}");
        assert_eq!(strings(&opaque, "AcCmTransparency"), ["33554687"]);
        assert_eq!(strings(&opaque, "AcAecLayerStandard"), ["", ""]);
    }
}

/// A file where the active VPORT and a paper space viewport each own a SUN (hard-owned objects
/// that group 361 of their owner points at).
fn suns() -> String {
    let sun = |handle: &'static str, owner: &'static str| {
        vec![
            (0, "SUN"),
            (5, handle),
            (330, owner),
            (100, "AcDbSun"),
            (90, "1"),
            (290, "0"),
            (63, "7"),
            (40, "1"),
            (291, "1"),
            (91, "2455826"),
            (92, "54000000"),
            (292, "0"),
            (70, "2"),
            (71, "256"),
            (280, "1"),
        ]
    };
    let mut g: Vec<(i32, &str)> = vec![
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "VPORT"),
        (70, "1"),
        (0, "VPORT"),
        (5, "70"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbViewportTableRecord"),
        (2, "*Active"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
        (12, "50"),
        (22, "50"),
        (40, "100"),
        (41, "1.6"),
        (361, "71"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "VIEWPORT"),
        (5, "40"),
        (100, "AcDbEntity"),
        (67, "1"),
        (8, "0"),
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
        (45, "50"),
        (90, "32768"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
    ];
    g.extend(sun("71", "70"));
    g.extend(sun("72", "40"));
    g.extend([(0, "ENDSEC"), (0, "EOF")]);
    dxf(&g)
}

/// A SUN stays with the active VPORT or the viewport that owns it, and its owner points at it. A
/// DWG keeps the VPORT's link; acadrust's DXF reader has no group 361 for a viewport entity.
#[test]
fn suns_stay_linked_to_their_vport_and_viewport() {
    let d = cadcraft_io::read_dxf(suns().as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let objs = section(&text, "OBJECTS");
        let suns: Vec<&Vec<Tag>> = objs.iter().filter(|(k, _)| k == "SUN").map(|(_, t)| t).collect();
        assert_eq!(suns.len(), 2, "{text}");
        let owned_by = |owner: &str| suns.iter().find(|t| val(t, 330) == owner).map(|t| val(t, 5));
        let vport =
            section(&text, "TABLES").into_iter().find(|(k, t)| k == "VPORT" && val(t, 2).eq_ignore_ascii_case("*Active")).map(|(_, t)| t).unwrap();
        assert_eq!(Some(val(&vport, 361)), owned_by(&val(&vport, 5)), "the VPORT points at its sun: {text}");
        if i == 0 {
            let ents = section(&text, "ENTITIES");
            let viewport = ents.iter().find(|(k, t)| k == "VIEWPORT" && val(t, 5) == "40").map(|(_, t)| t.clone()).expect("the viewport");
            assert_eq!(Some(val(&viewport, 361)), owned_by("40"), "the viewport points at its sun: {text}");
        }
    }
}

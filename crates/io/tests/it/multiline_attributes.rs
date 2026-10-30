//! Multiline attributes (DXF attribute type 2, or 4 for a definition): their text is embedded
//! MTEXT after the attribute's own groups (`101 Embedded Object`). It is read, drawn, written
//! back and survives a save; edits to the value replace it.
//!
//! The fixture is synthetic, laid out as the DXF Reference describes these attributes: block TAG
//! with a multiline definition LABEL, and a reference whose LABEL attribute has its own text.

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_io::{SaveOptions, Version, read_dxf};

/// DXF text from lines of `code value` (the value may be empty).
fn dxf(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        out.push_str(&format!("{code}\n{value}\n"));
    }
    out
}

pub(super) const FIXTURE: &str = "
0 SECTION
2 HEADER
9 $ACADVER
1 AC1032
9 $HANDSEED
5 200
0 ENDSEC
0 SECTION
2 TABLES
0 TABLE
2 BLOCK_RECORD
5 1
330 0
100 AcDbSymbolTable
70 3
0 BLOCK_RECORD
5 1F
330 1
100 AcDbSymbolTableRecord
100 AcDbBlockTableRecord
2 *Model_Space
340 0
0 BLOCK_RECORD
5 1E
330 1
100 AcDbSymbolTableRecord
100 AcDbBlockTableRecord
2 *Paper_Space
340 0
0 BLOCK_RECORD
5 1D
330 1
100 AcDbSymbolTableRecord
100 AcDbBlockTableRecord
2 TAG
340 0
0 ENDTAB
0 ENDSEC
0 SECTION
2 BLOCKS
0 BLOCK
5 20
330 1F
100 AcDbEntity
8 0
100 AcDbBlockBegin
2 *Model_Space
70 0
10 0
20 0
30 0
3 *Model_Space
1
0 ENDBLK
5 21
330 1F
100 AcDbEntity
8 0
100 AcDbBlockEnd
0 BLOCK
5 24
330 1D
100 AcDbEntity
8 0
100 AcDbBlockBegin
2 TAG
70 2
10 0
20 0
30 0
3 TAG
1
0 LINE
5 30
330 1D
100 AcDbEntity
8 0
100 AcDbLine
10 0
20 0
30 0
11 20
21 0
31 0
0 ATTDEF
5 31
330 1D
100 AcDbEntity
8 0
100 AcDbText
10 2
20 2
30 0
40 2
1 SITE NAME
7 Standard
72 1
11 10
21 3
31 0
100 AcDbAttributeDefinition
280 0
3 Site
2 LABEL
70 0
73 0
74 1
280 1
71 4
72 0
11 10
21 3
31 0
101 Embedded Object
10 10
20 3
30 0
40 2
41 0
46 0
71 8
72 5
1 SITE\\PNAME
7 Standard
73 1
44 1
0 ENDBLK
5 25
330 1D
100 AcDbEntity
8 0
100 AcDbBlockEnd
0 ENDSEC
0 SECTION
2 ENTITIES
0 INSERT
5 40
330 1F
100 AcDbEntity
8 0
100 AcDbBlockReference
66 1
2 TAG
10 100
20 50
30 0
0 ATTRIB
5 41
330 40
100 AcDbEntity
8 0
100 AcDbText
10 102
20 52
30 0
40 2
1
7 Standard
72 1
11 110
21 53
31 0
100 AcDbAttribute
280 0
2 LABEL
70 0
74 1
280 1
71 2
72 1
11 110
21 53
31 0
101 Embedded Object
10 110
20 53
30 0
40 2
41 0
46 0
71 8
72 5
1 NORTH\\PWELL 7
7 Standard
73 1
44 1
0 SEQEND
5 42
330 40
100 AcDbEntity
8 0
0 ENDSEC
0 SECTION
2 OBJECTS
0 DICTIONARY
5 C
330 0
100 AcDbDictionary
281 1
0 ENDSEC
0 EOF
";

/// The drawing saved as the app saves it (`write`: its own version, AutoCAD 2018 here).
fn save(d: &Drawing) -> String {
    String::from_utf8(cadcraft_io::write(d, "x.dxf").unwrap()).unwrap()
}

fn reference(d: &Drawing) -> &cadcraft_doc::Insert {
    match d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i) } else { None }) {
        Some(i) => i,
        None => panic!("no reference"),
    }
}

fn definition(d: &Drawing) -> &cadcraft_doc::Attrib {
    match d.block("TAG").and_then(|b| b.entities.iter().find_map(|e| if let EntityKind::AttDef(a) = &e.kind { Some(a) } else { None })) {
        Some(a) => a,
        None => panic!("no definition"),
    }
}

#[test]
fn multiline_attributes_are_read_with_their_text() {
    let d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let a = &reference(&d).attribs[0];
    assert_eq!(a.tag, "LABEL");
    assert_eq!(a.text.value, "NORTH\nWELL 7", "the value is the multiline text's plain text");
    let m = a.multiline.as_ref().expect("multiline");
    assert_eq!(m.mtext.contents, "NORTH\\PWELL 7");
    assert_eq!((m.mtext.insert.x, m.mtext.insert.y, m.mtext.attach), (110.0, 53.0, 8));
    // The attribute's own groups, not the embedded text's, give the single-line fields.
    assert_eq!(a.text.insert.x, 102.0);

    let ad = definition(&d);
    assert_eq!((ad.tag.as_str(), ad.prompt.as_str()), ("LABEL", "Site"));
    assert_eq!(ad.text.value, "SITE\nNAME");
    assert_eq!(ad.multiline.as_ref().map(|m| m.mtext.contents.as_str()), Some("SITE\\PNAME"));
}

#[test]
fn a_multiline_attribute_is_drawn() {
    let d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let opts = cadcraft_render::Options::default();
    let with = cadcraft_render::build(&d, &cadcraft_doc::Space::Model, &opts);
    let mut bare = d.clone();
    let first = bare.model.iter().next().map(|e| e.handle);
    if let Some(e) = first {
        bare.model
            .modify(e, |e| {
                if let EntityKind::Insert(i) = &mut e.kind {
                    i.attribs.clear();
                }
            })
            .unwrap();
    }
    let without = cadcraft_render::build(&bare, &cadcraft_doc::Space::Model, &opts);
    assert!(
        with.prims.len() > without.prims.len() + 4,
        "the attribute's two lines of text are drawn ({} vs {})",
        with.prims.len(),
        without.prims.len()
    );
    // Its text sits where the embedded text is: around (110, 53), bottom-centre attached.
    assert!(with.bounds.max.y > 53.5 && with.bounds.max.x > 105.0, "{:?}", with.bounds);
}

#[test]
fn multiline_attributes_survive_a_save() {
    let d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let out = save(&d);
    assert_eq!(out.matches("Embedded Object").count(), 2, "the definition's and the attribute's");
    let again = read_dxf(out.as_bytes()).unwrap();
    assert_eq!(reference(&again).attribs[0], reference(&d).attribs[0]);
    assert_eq!(definition(&again), definition(&d));
}

#[test]
fn a_left_justified_multiline_attribute_is_not_read_as_centred() {
    // Left justification has no group 72 in the text part; the 72 after the attribute type is not the justification.
    let left = FIXTURE.replacen("7 Standard\n72 1\n11 110\n21 53", "7 Standard\n11 110\n21 53", 1);
    let d = read_dxf(dxf(&left).as_bytes()).unwrap();
    assert_eq!(reference(&d).attribs[0].text.halign, cadcraft_doc::HAlign::Left, "read");
    let again = read_dxf(save(&d).as_bytes()).unwrap();
    assert_eq!(reference(&again).attribs[0].text.halign, cadcraft_doc::HAlign::Left, "after a save");
}

#[test]
fn an_edited_value_replaces_the_multiline_text() {
    let mut d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let h = d.model.iter().next().map(|e| e.handle).unwrap();
    d.model
        .modify(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.attribs[0].text.value = "SOUTH {A}\nWELL 8".into();
            }
        })
        .unwrap();
    let a = &reference(&d).attribs[0];
    assert_eq!(a.multiline_text().map(|m| m.contents), Some("SOUTH \\{A\\}\\PWELL 8".to_string()));
    let again = read_dxf(save(&d).as_bytes()).unwrap();
    assert_eq!(reference(&again).attribs[0].text.value, "SOUTH {A}\nWELL 8");
}

#[test]
fn files_before_2007_get_single_line_attributes() {
    let d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let out =
        String::from_utf8(cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2004), binary: false }).unwrap()).unwrap();
    assert!(!out.contains("Embedded Object"));
    let again = read_dxf(out.as_bytes()).unwrap();
    let a = &reference(&again).attribs[0];
    assert!(a.multiline.is_none());
    // Single-line text can't hold a line break: the text stays, on one line.
    assert_eq!(a.text.value, "NORTH WELL 7");
}

#[test]
fn damaged_embedded_text_does_not_panic() {
    for (from, to) in [
        ("101 Embedded Object\n10 110", "101 Embedded Object"),
        ("71 8\n72 5\n1 NORTH", "71 99\n72 5\n1 NORTH"),
        ("40 2\n41 0\n46 0\n71 8", "40 nan\n41 -5\n46 x\n71 8"),
    ] {
        let src = FIXTURE.replacen(from, to, 1);
        let d = read_dxf(dxf(&src).as_bytes()).unwrap();
        let _ = cadcraft_render::build(&d, &cadcraft_doc::Space::Model, &cadcraft_render::Options::default());
        let _ = read_dxf(save(&d).as_bytes()).unwrap();
    }
}

/// The numbers of the embedded MTEXT (after the `101` group) of the first ATTRIB of a DXF text, as (code, bits).
fn embedded_numbers(text: &str) -> Vec<(i32, u64)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let entities = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let (_, attrib) = cadcraft_dxf::records(&entities.tags).into_iter().find(|(k, _)| k == "ATTRIB").unwrap();
    let at = attrib.iter().position(|t| t.code == 101).expect("an embedded object");
    attrib
        .iter()
        .skip(at + 1)
        .take_while(|t| t.code < 1000)
        .filter(|t| matches!(t.code, 10 | 20 | 30 | 40 | 41 | 46 | 71 | 72 | 11 | 21 | 31 | 50 | 73 | 44 | 210 | 220 | 230))
        .map(|t| (t.code, t.str().trim().parse::<f64>().unwrap().to_bits()))
        .collect()
}

/// The embedded MTEXT of an attribute has a plane of its own, kept as it was read while its placement is.
#[test]
fn an_embedded_mtext_keeps_its_plane_and_placement() {
    let source =
        dxf(&FIXTURE.replace("NORTH\\PWELL 7\n7 Standard\n73 1\n44 1\n", "NORTH\\PWELL 7\n7 Standard\n210 0.6\n220 0\n230 0.8\n73 1\n44 1\n"));
    assert!(source.contains("210\n0.6\n"), "the fixture gets the plane");
    let want = embedded_numbers(&source);
    assert!(want.iter().any(|(c, _)| *c == 230), "{want:?}");
    let d = read_dxf(source.as_bytes()).unwrap();
    assert_eq!(embedded_numbers(&save(&d)), want);
    assert_eq!(embedded_numbers(&save(&read_dxf(save(&d).as_bytes()).unwrap())), want, "second save");
    // A text edit keeps the plane and writes the new text.
    let mut edited = d.clone();
    let h = edited.model.iter().next().unwrap().handle;
    edited
        .modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.attribs[0].text.value = "EAST".into();
            }
        })
        .unwrap();
    assert_eq!(embedded_numbers(&save(&edited)).iter().filter(|(c, _)| *c == 230).count(), 1);
}

/// The multiline text embedded in an attribute (after its 101) is written in the order of an MTEXT, as AutoCAD writes it: the text (1)
/// and its style (7) before the line spacing (73, 44).
#[test]
fn the_embedded_text_of_an_attribute_has_the_order_of_a_multiline_text() {
    let d = read_dxf(dxf(FIXTURE).as_bytes()).unwrap();
    let tags = cadcraft_dxf::parse(save(&d).as_bytes()).unwrap();
    let embedded: Vec<Vec<i32>> = tags
        .split(|t| t.code == 0)
        .filter(|record| record.iter().any(|t| t.code == 100 && t.str() == "AcDbAttribute"))
        .map(|record| record.iter().skip_while(|t| t.code != 101).skip(1).take_while(|t| t.code < 1000 && t.code != 100).map(|t| t.code).collect())
        .collect();
    assert!(!embedded.is_empty());
    let rank = |c: i32| [10, 40, 41, 46, 71, 72, 1, 7, 73, 44].iter().position(|x| *x == c);
    for codes in embedded {
        let ranks: Vec<usize> = codes.iter().filter_map(|c| rank(*c)).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "embedded text out of order: {codes:?}");
    }
}

//! The columns of an R2018 MTEXT live in its embedded object (group 101: type 71, count 72, width 44,
//! gutter 45, auto height 73, flow 74, a height 46 per manual column). They are read into the model,
//! written back from it (so an edited column text keeps them) and survive a DWG save.

use cadcraft_doc::{Drawing, EntityKind, MText, MTextColumnType};
use cadcraft_dxf::Tag;

fn dxf(entities: &str) -> String {
    let mut text = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for line in entities.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        text.push_str(&format!("{code}\n{value}\n"));
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

/// The rc3 `min1` shape: a dynamic text with one manual column height.
const MIN1: &str = "0 MTEXT\n5 A1\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 13772.5\n20 -10439.9\n30 0\n40 0.15\n41 2.055\n71 1\n72 5\n1 PA_JARDL\n7 Standard\n\
    210 0\n220 0\n230 1\n11 1\n21 0\n31 0\n42 1.348\n43 0.164\n73 1\n44 1\n\
    101 Embedded Object\n70 1\n10 1\n20 0\n30 0\n11 13772.5\n21 -10439.9\n31 0\n40 2.055\n41 0\n42 1.348\n43 0.164\n71 2\n72 1\n44 2.055\n45 0.75\n73 0\n74 0\n46 0.164";

/// AutoCAD's own MTEXT 31E46 of a real drawing (Firenze): the same layout with other numbers.
const ACAD_31E46: &str = "0 MTEXT\n5 31E46\n100 AcDbEntity\n8 ARQ-TXT3\n100 AcDbMText\n10 92.05155161946208\n20 81.18363642409599\n30 0\n40 0.25\n41 1.733477175990536\n46 0\n71 5\n72 5\n1 FINAL 11\n7 ROMANS\n\
    73 1\n44 1\n90 16\n63 256\n45 1.5\n441 0\n\
    101 Embedded Object\n70 5\n10 1\n20 0\n30 0\n11 92.05155161946208\n21 81.18363642409599\n31 0\n40 1.733477175990536\n41 0\n42 1.559523809523809\n43 0.25\n71 2\n72 1\n44 1.733477175990536\n45 46.40160604328457\n73 0\n74 0\n46 0";

/// A three-column static text, and a dynamic one with automatic height and a reversed flow.
const STATIC3: &str = "0 MTEXT\n5 A2\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 1\n20 2\n30 0\n40 0.5\n41 20\n71 1\n72 1\n1 one two three\n7 Standard\n73 1\n44 1\n\
    101 Embedded Object\n70 1\n10 1\n20 0\n30 0\n11 1\n21 2\n31 0\n40 62.5\n41 8.5\n42 0\n43 0\n71 1\n72 3\n44 20\n45 1.25\n73 0\n74 0";
const AUTO: &str = "0 MTEXT\n5 A3\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 1\n20 2\n30 0\n40 0.5\n41 20\n71 1\n72 1\n1 one two three\n7 Standard\n73 1\n44 1\n\
    101 Embedded Object\n70 1\n10 1\n20 0\n30 0\n11 1\n21 2\n31 0\n40 41.5\n41 6\n42 0\n43 0\n71 2\n72 2\n44 20\n45 1.5\n73 1\n74 1";

fn mtext(d: &Drawing) -> MText {
    d.model.iter().find_map(|e| if let EntityKind::MText(t) = &e.kind { Some(t.clone()) } else { None }).expect("an MTEXT")
}

fn r2018(d: &Drawing) -> String {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap()
}

/// The groups of the first MTEXT record of a DXF text.
fn record(text: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    cadcraft_dxf::records(&section.tags).into_iter().find(|(k, _)| k == "MTEXT").unwrap().1
}

/// The groups of the embedded object of the first MTEXT.
fn embedded(text: &str) -> Vec<Tag> {
    let record = record(text);
    let at = record.iter().position(|t| t.code == 101).expect("an embedded object");
    record.into_iter().skip(at + 1).take_while(|t| t.code < 1000).collect()
}

fn num(block: &[Tag], code: i32) -> Vec<f64> {
    block.iter().filter(|t| t.code == code).map(|t| t.str().trim().parse().unwrap()).collect()
}

/// The column layout of the embedded object, by group: type, count, auto height, flow, width, gutter, heights.
fn layout(text: &str) -> Vec<Vec<f64>> {
    let b = embedded(text);
    [71, 72, 73, 74, 44, 45, 46].iter().map(|c| num(&b, *c)).collect()
}

#[test]
fn the_columns_of_the_embedded_object_are_read() {
    let t = mtext(&cadcraft_io::read_dxf(dxf(MIN1).as_bytes()).unwrap());
    let c = t.columns.expect("columns");
    assert_eq!(
        (c.kind, c.count, c.auto_height, c.flow_reversed, c.gutter, c.heights.clone()),
        (MTextColumnType::Dynamic, 1, false, false, 0.75, vec![0.164])
    );
    assert_eq!(t.width, 2.055);
    let t = mtext(&cadcraft_io::read_dxf(dxf(ACAD_31E46).as_bytes()).unwrap());
    let c = t.columns.expect("columns");
    assert_eq!((c.kind, c.count, c.gutter), (MTextColumnType::Dynamic, 1, 46.40160604328457));
    assert_eq!(t.width, 1.733477175990536);
    let t = mtext(&cadcraft_io::read_dxf(dxf(STATIC3).as_bytes()).unwrap());
    let c = t.columns.expect("columns");
    assert_eq!((c.kind, c.count, c.gutter, c.height, t.width), (MTextColumnType::Static, 3, 1.25, 8.5, 20.0));
    let t = mtext(&cadcraft_io::read_dxf(dxf(AUTO).as_bytes()).unwrap());
    let c = t.columns.expect("columns");
    assert_eq!((c.kind, c.count, c.auto_height, c.flow_reversed, c.height), (MTextColumnType::Dynamic, 2, true, true, 6.0));
}

#[test]
fn an_r2018_save_writes_the_columns_in_the_embedded_object_as_read() {
    for source in [MIN1, ACAD_31E46, STATIC3, AUTO] {
        let want = layout(&dxf(source));
        let d = cadcraft_io::read_dxf(dxf(source).as_bytes()).unwrap();
        let first = r2018(&d);
        assert_eq!(layout(&first), want, "first save of {source}");
        // The main subclass has none of the column groups, which AutoCAD misreads in 2018.
        let rec = record(&first);
        let main = &rec[..rec.iter().position(|t| t.code == 101).unwrap()];
        assert!(!main.iter().any(|t| matches!(t.code, 75 | 76 | 78 | 79)), "{main:?}");
        let second = r2018(&cadcraft_io::read_dxf(first.as_bytes()).unwrap());
        assert_eq!(layout(&second), want, "second save");
    }
}

#[test]
fn an_edited_column_text_keeps_its_columns() {
    let mut d = cadcraft_io::read_dxf(dxf(ACAD_31E46).as_bytes()).unwrap();
    let want = layout(&dxf(ACAD_31E46));
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MText(t) = &mut e.kind {
            t.insert.x += 5.0;
        }
    })
    .unwrap();
    let text = r2018(&d);
    assert_eq!(layout(&text), want);
    assert_eq!(num(&embedded(&text), 11)[0], 97.05155161946208, "the block follows the edit");
    assert_eq!(mtext(&cadcraft_io::read_dxf(text.as_bytes()).unwrap()).columns.map(|c| c.kind), Some(MTextColumnType::Dynamic));
}

#[test]
fn the_columns_survive_a_dwg_save() {
    for source in [MIN1, ACAD_31E46, STATIC3, AUTO] {
        let d = cadcraft_io::read_dxf(dxf(source).as_bytes()).unwrap();
        let want = mtext(&d);
        let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
        let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        assert_eq!(mtext(&back).columns, want.columns, "{source}");
        assert_eq!(mtext(&back).width, want.width);
        // A second DWG save.
        let again = cadcraft_io::read(&cadcraft_io::write(&back, "x.dwg").unwrap(), "x.dwg").unwrap();
        assert_eq!(mtext(&again).columns, want.columns);
        // And the 2018 DXF of the DWG save has them in the embedded object.
        assert_eq!(layout(&r2018(&back)), layout(&dxf(source)));
    }
}

/// Columns read from the embedded object are drawn without losing text: the same glyphs as the text
/// without its columns (a single column of AutoCAD's own record), and the text of a static
/// three-column layout spread over the width of its columns.
#[test]
fn a_column_text_is_drawn_with_all_its_text() {
    let opts = cadcraft_render::Options::default();
    let prims = |d: &Drawing| cadcraft_render::build(d, &cadcraft_doc::Space::Model, &opts);
    let d = cadcraft_io::read_dxf(dxf(ACAD_31E46).as_bytes()).unwrap();
    let mut bare = d.clone();
    let h = bare.model.iter().next().unwrap().handle;
    bare.model.modify(h, |e| {
        if let EntityKind::MText(t) = &mut e.kind {
            t.columns = None;
        }
    });
    let (with, without) = (prims(&d), prims(&bare));
    assert!(!with.prims.is_empty(), "the text is drawn");
    assert_eq!(with.prims.len(), without.prims.len(), "one column draws as the text does");
    let wide = cadcraft_io::read_dxf(dxf(STATIC3).as_bytes()).unwrap();
    let b = prims(&wide).bounds;
    assert!(b.max.x - b.min.x > 5.0 && b.max.x - b.min.x <= 70.0, "{b:?}");
}

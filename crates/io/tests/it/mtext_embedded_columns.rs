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
    // The text style of the fixtures is not in a style table, which a DWG would replace by Standard.
    for source in [MIN1, ACAD_31E46, STATIC3, AUTO].map(|s| s.replace("ROMANS", "Standard")) {
        let source = source.as_str();
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

/// AutoCAD's own two-column MTEXT of DP Inc (DXFOUT 2018 of the DWG, mateus-lima): dynamic columns with
/// manual heights, a reference rectangle (41) of all the columns, and laid-out extents (42, 43).
const DPINC_TWO_COLUMNS: &str = "0 MTEXT\n5 1C2\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 -2888.7344008081\n20 481.533821574024\n30 0\n40 0.2\n41 7.866274197902384\n46 0\n71 1\n72 5\n1 TEXTO\n7 Standard\n73 1\n44 1\n\
    101 Embedded Object\n70 1\n10 1\n20 0\n30 0\n11 -2888.7344008081\n21 481.533821574024\n31 0\n40 7.866274197902384\n41 0\n42 7.866274197902384\n43 0.5333333333333333\n71 2\n72 2\n44 3.558137098951192\n45 0.75\n73 0\n74 0\n46 0.256343792633015\n46 0";

#[test]
fn a_multi_column_text_has_the_whole_width_in_41_and_keeps_autocads_extents() {
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    let d = cadcraft_io::read_dxf(dxf(DPINC_TWO_COLUMNS).as_bytes()).unwrap();
    let t = mtext(&d);
    assert_eq!(t.width, 3.558137098951192, "the text's width is one column's");
    assert_eq!(t.column_extents(), (7.866274197902384, 0.5333333333333333));
    for text in [r2018(&d), r2018(&cadcraft_io::read_dxf(r2018(&d).as_bytes()).unwrap())] {
        let rec = record(&text);
        let main = &rec[..rec.iter().position(|t| t.code == 101).unwrap()];
        // 41 of the text is all the columns and their gutter: 2 x 3.558.. + 0.75 (AutoCAD's 7.866274197902384).
        assert!((num(main, 41)[0] - 7.866274197902384).abs() < 1e-12, "{main:?}");
        let b = embedded(&text);
        assert!((num(&b, 40)[0] - 7.866274197902384).abs() < 1e-12);
        assert_eq!((bits(&num(&b, 42)), bits(&num(&b, 43))), (bits(&[7.866274197902384]), bits(&[0.5333333333333333])));
        assert_eq!(num(&b, 46), vec![0.256343792633015, 0.0]);
    }
    // A DWG keeps the extents; a move keeps them; an edit of the text drops them (AutoCAD lays it out again).
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(mtext(&back).column_extents(), (7.866274197902384, 0.5333333333333333));
    let mut moved = d.clone();
    let h = moved.model.iter().next().unwrap().handle;
    moved
        .modify_entity(h, |e| {
            if let EntityKind::MText(t) = &mut e.kind {
                t.insert.x += 3.0;
            }
        })
        .unwrap();
    assert_eq!(mtext(&moved).column_extents(), (7.866274197902384, 0.5333333333333333));
    // Written from the model (the move dropped the snapshot of the main groups), 41 is still all the columns.
    let rec = record(&r2018(&moved));
    assert!((num(&rec[..rec.iter().position(|t| t.code == 101).unwrap()], 41)[0] - 7.866274197902384).abs() < 1e-12, "{rec:?}");
    assert_eq!(num(&embedded(&r2018(&moved)), 42), vec![7.866274197902384]);
    moved
        .modify_entity(h, |e| {
            if let EntityKind::MText(t) = &mut e.kind {
                t.contents = "OTHER TEXT".into();
            }
        })
        .unwrap();
    assert_eq!(num(&embedded(&r2018(&moved)), 42), vec![0.0]);
}

/// AutoCAD's own MTEXT of Citycon in a 2010 DXF (DXFOUT 2010 of the DWG, mateus-lima): the columns are in
/// the ACAD extended data, not in the entity's groups.
const CITYCON_2010: &str = "0 MTEXT\n5 61697\n100 AcDbEntity\n8 carimbo cor1\n100 AcDbMText\n10 8.554471942766895\n20 20\n30 0\n40 5.854888252773742\n41 182.4335599239948\n46 0\n71 1\n72 5\n1 CARIMBO\n7 Standard\n73 1\n44 1\n\
    1001 ACAD\n1000 ACAD_MTEXT_COLUMN_INFO_BEGIN\n1070 75\n1070 2\n1070 79\n1070 0\n1070 76\n1070 1\n1070 78\n1070 0\n1070 48\n1040 182.4335599239948\n1070 49\n1040 11.41669000533545\n1070 50\n1070 1\n1040 0\n1000 ACAD_MTEXT_COLUMN_INFO_END";

fn r2010(d: &Drawing) -> String {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: false };
    String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap()
}

/// The values of the ACAD column record (between its markers) of the first MTEXT of a text, and how many
/// ACAD records the entity has.
fn xdata_columns(text: &str) -> (Vec<String>, usize) {
    let rec = record(text);
    let apps = rec.iter().filter(|t| t.code == 1001 && t.str().trim() == "ACAD").count();
    let begin = rec.iter().position(|t| t.code == 1000 && t.str() == "ACAD_MTEXT_COLUMN_INFO_BEGIN");
    let values = begin
        .map(|b| {
            rec[b + 1..]
                .iter()
                .take_while(|t| t.str() != "ACAD_MTEXT_COLUMN_INFO_END")
                .map(|t| t.str().trim().parse::<f64>().map_or_else(|_| t.str(), |v| format!("{:x}", v.to_bits())))
                .collect()
        })
        .unwrap_or_default();
    (values, apps)
}

#[test]
fn the_columns_in_acad_xdata_are_read_and_written_from_the_model() {
    let source = dxf(CITYCON_2010);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let t = mtext(&d);
    let c = t.columns.clone().expect("columns");
    assert_eq!(
        (c.kind, c.count, c.auto_height, c.flow_reversed, c.gutter, c.heights.clone()),
        (MTextColumnType::Dynamic, 1, false, false, 11.41669000533545, vec![0.0])
    );
    assert_eq!(t.width, 182.4335599239948);
    // The foreign copy is gone: the entity's xdata no longer holds the record.
    assert!(d.model.iter().next().unwrap().common.xdata.is_empty(), "{:?}", d.model.iter().next().unwrap().common.xdata);
    let (want, apps) = xdata_columns(&source);
    assert_eq!(apps, 1);
    // A 2010 save writes the record, once and as it was; so do a second save and an edited (moved) text.
    let first = r2010(&d);
    assert_eq!(xdata_columns(&first), (want.clone(), 1), "first save");
    assert!(!record(&first).iter().any(|t| matches!(t.code, 75 | 76 | 78 | 79 | 101)));
    assert_eq!(xdata_columns(&r2010(&cadcraft_io::read_dxf(first.as_bytes()).unwrap())), (want.clone(), 1), "second save");
    let mut moved = d.clone();
    let h = moved.model.iter().next().unwrap().handle;
    moved
        .modify_entity(h, |e| {
            if let EntityKind::MText(t) = &mut e.kind {
                t.insert.x += 1.0;
            }
        })
        .unwrap();
    assert_eq!(xdata_columns(&r2010(&moved)), (want.clone(), 1), "moved");
    // A 2018 save has the embedded object and no xdata record; the main subclass has the whole width.
    let new = r2018(&d);
    assert_eq!(xdata_columns(&new).1, 0);
    assert_eq!(layout(&new)[0], vec![2.0]);
    // Through a DWG and back to a 2010 DXF: one record.
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(mtext(&back).columns.map(|c| (c.kind, c.count, c.gutter)), Some((MTextColumnType::Dynamic, 1, 11.41669000533545)));
    assert_eq!(xdata_columns(&r2010(&back)).1, 1);
}

/// The same record with another ACAD item before it and another after it.
fn mixed_2010() -> String {
    CITYCON_2010
        .replace("1001 ACAD\n1000 ACAD_MTEXT_COLUMN_INFO_BEGIN", "1001 ACAD\n1000 ACAD_KEEP_BEFORE\n1070 11\n1000 ACAD_MTEXT_COLUMN_INFO_BEGIN")
        .replace("1000 ACAD_MTEXT_COLUMN_INFO_END", "1000 ACAD_MTEXT_COLUMN_INFO_END\n1000 ACAD_KEEP_AFTER\n1040 2.5")
}

/// Every group of the ACAD application of the first MTEXT, as (code, value) with numbers by their bits.
fn acad_chunk(text: &str) -> Vec<(i32, String)> {
    let rec = record(text);
    let Some(start) = rec.iter().position(|t| t.code == 1001 && t.str().trim() == "ACAD") else { return Vec::new() };
    rec[start..]
        .iter()
        .enumerate()
        .take_while(|(i, t)| *i == 0 || t.code != 1001)
        .map(|(_, t)| (t.code, t.str().trim().parse::<f64>().map_or_else(|_| t.str().trim().to_string(), |v| format!("{:x}", v.to_bits()))))
        .collect()
}

#[test]
fn the_column_record_beside_other_acad_data_is_read_and_the_rest_kept_in_order() {
    let source = dxf(&mixed_2010());
    let want = acad_chunk(&source);
    assert_eq!(want.iter().filter(|(_, v)| v == "ACAD_MTEXT_COLUMN_INFO_BEGIN").count(), 1);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let t = mtext(&d);
    let c = t.columns.clone().expect("the columns are read");
    assert_eq!((c.kind, c.count, c.gutter), (MTextColumnType::Dynamic, 1, 11.41669000533545));
    // The rest stays foreign xdata, without the numbers of the record.
    let kept: Vec<String> = d.model.iter().next().unwrap().common.xdata.iter().map(|r| r.value.trim().to_string()).collect();
    assert!(kept.contains(&"ACAD_KEEP_BEFORE".to_string()) && kept.contains(&"ACAD_KEEP_AFTER".to_string()), "{kept:?}");
    assert!(!kept.contains(&"182.4335599239948".to_string()), "{kept:?}");
    // A save writes the one chunk as it was, in its order; so do a second save and a moved text.
    let first = r2010(&d);
    assert_eq!(acad_chunk(&first), want, "first save");
    assert_eq!(acad_chunk(&r2010(&cadcraft_io::read_dxf(first.as_bytes()).unwrap())), want, "second save");
    let mut moved = d.clone();
    let h = moved.model.iter().next().unwrap().handle;
    moved
        .modify_entity(h, |e| {
            if let EntityKind::MText(t) = &mut e.kind {
                t.insert.x += 1.0;
            }
        })
        .unwrap();
    assert_eq!(acad_chunk(&r2010(&moved)), want, "moved");
    // The model's columns edited: the record changes, the rest does not.
    let mut edited = d.clone();
    edited
        .modify_entity(h, |e| {
            if let EntityKind::MText(t) = &mut e.kind
                && let Some(c) = &mut t.columns
            {
                c.gutter = 3.0;
            }
        })
        .unwrap();
    let chunk = acad_chunk(&r2010(&edited));
    assert_eq!(chunk.len(), want.len());
    assert_ne!(chunk, want);
    assert_eq!((&chunk[..3], &chunk[chunk.len() - 3..]), (&want[..3], &want[want.len() - 3..]));
    // 2018: no xdata copy of the record or the empty place; the rest is still there once.
    let new = r2018(&d);
    let chunk = acad_chunk(&new);
    assert!(!chunk.iter().any(|(_, v)| v.starts_with("ACAD_MTEXT_COLUMN_INFO")), "{chunk:?}");
    assert_eq!(chunk.iter().filter(|(_, v)| v == "ACAD_KEEP_BEFORE" || v == "ACAD_KEEP_AFTER").count(), 2);
    assert_eq!(layout(&new)[0], vec![2.0]);
    // A DWG keeps the columns, and its 2010 DXF has one column record and both other items.
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(mtext(&back).columns.map(|c| (c.kind, c.count, c.gutter)), Some((MTextColumnType::Dynamic, 1, 11.41669000533545)));
    let out = r2010(&back);
    let all: Vec<String> = record(&out).iter().map(|t| t.str().trim().to_string()).collect();
    assert_eq!(all.iter().filter(|v| *v == "ACAD_MTEXT_COLUMN_INFO_BEGIN").count(), 1, "{all:?}");
    assert!(all.contains(&"ACAD_KEEP_BEFORE".to_string()) && all.contains(&"ACAD_KEEP_AFTER".to_string()), "{all:?}");
}

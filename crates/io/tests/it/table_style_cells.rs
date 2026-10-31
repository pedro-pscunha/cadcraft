//! A table style is written the way AutoCAD writes it: the object's version (280) first, and each cell style with its data type (90),
//! unit type (91) and format string (1) and its borders' lineweight, visibility and colour together. The title switch is the last 280.

use super::foreign_passthrough::{dxf, saved_twice, section, val};

fn source() -> String {
    let mut g: Vec<(i32, &str)> = vec![
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (3, "ACAD_TABLESTYLE"),
        (350, "22"),
        (0, "DICTIONARY"),
        (5, "22"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (3, "Schedule"),
        (350, "51"),
        (0, "TABLESTYLE"),
        (5, "51"),
        (330, "22"),
        (100, "AcDbTableStyle"),
        (280, "0"),
        (3, ""),
        (70, "0"),
        (71, "0"),
        (40, "1.5"),
        (41, "1.5"),
        (280, "1"),
        (281, "0"),
    ];
    for _ in 0..3 {
        g.extend([(7, "Standard"), (140, "4.5"), (170, "2"), (62, "0"), (63, "7"), (283, "0"), (90, "512"), (91, "0"), (1, "")]);
        for k in 0..6 {
            let (lw, vis, col) = (
                ["274", "275", "276", "277", "278", "279"][k],
                ["284", "285", "286", "287", "288", "289"][k],
                ["64", "65", "66", "67", "68", "69"][k],
            );
            g.extend([(lw.parse().unwrap(), "-2"), (vis.parse().unwrap(), "1"), (col.parse().unwrap(), "0")]);
        }
    }
    g.extend([(0, "ENDSEC"), (0, "EOF")]);
    dxf(&g)
}

#[test]
fn cell_data_types_are_kept_per_source_cell_style() {
    let mut tags = cadcraft_dxf::parse(source().replace("3\n\n70\n", "3\nSOURCE_DATA_FORMAT\n70\n").as_bytes()).unwrap();
    let control = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    let private_payload = |text: &str| {
        section(text, "OBJECTS")
            .into_iter()
            .find(|(kind, tags)| kind == "TABLESTYLE" && tags.iter().any(|tag| tag.code == 3 && tag.str() == "SOURCE_DATA_FORMAT"))
            .unwrap()
            .1
            .into_iter()
            .filter(|tag| tag.code >= 1000)
            .map(|tag| (tag.code, tag.str()))
            .collect::<Vec<_>>()
    };
    let expected_private = private_payload(&cadcraft_io::write_dxf(&control));
    for (tag, value) in tags.iter_mut().filter(|tag| tag.code == 90).zip([4, 512, 4]) {
        *tag = cadcraft_dxf::Tag::i(90, value);
    }
    let source = cadcraft_dxf::write_ascii(&tags);
    let drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let style = drawing.table_styles.iter().find(|style| style.name == "Schedule").unwrap();
    assert_eq!(style.cell_style("Data").data_format.unwrap().data_type, 4);
    assert!(style.cell_style("Title").data_format.is_none(), "the implicit general format must stay implicit");
    assert_eq!(style.cell_style("Header").data_format.unwrap().data_type, 4);
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&drawing)) {
        assert_eq!(private_payload(&text), expected_private, "{what}: native value formats must not expand private xdata");
        let style = section(&text, "OBJECTS")
            .into_iter()
            .find(|(kind, tags)| kind == "TABLESTYLE" && tags.iter().any(|tag| tag.code == 3 && tag.str() == "SOURCE_DATA_FORMAT"))
            .unwrap()
            .1;
        assert_eq!(style.iter().filter(|tag| tag.code == 90).map(cadcraft_dxf::Tag::i64).collect::<Vec<_>>(), vec![4, 512, 4], "{what}");
        assert_eq!(
            style.iter().filter(|tag| tag.code == 91).map(cadcraft_dxf::Tag::i64).collect::<Vec<_>>(),
            vec![0, 0, 0],
            "{what}: native unit types"
        );
        assert_eq!(
            style.iter().filter(|tag| tag.code == 1).map(cadcraft_dxf::Tag::str).collect::<Vec<_>>(),
            vec!["", "", ""],
            "{what}: native format strings"
        );
        let reopened = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        let saved = cadcraft_io::write_dxf(&reopened);
        let style = section(&saved, "OBJECTS")
            .into_iter()
            .find(|(kind, tags)| kind == "TABLESTYLE" && tags.iter().any(|tag| tag.code == 3 && tag.str() == "SOURCE_DATA_FORMAT"))
            .unwrap()
            .1;
        assert_eq!(style.iter().filter(|tag| tag.code == 90).map(cadcraft_dxf::Tag::i64).collect::<Vec<_>>(), vec![4, 512, 4], "{what} reopened");
    }
}

#[test]
fn native_cell_value_formats_do_not_create_private_cell_styles() {
    let source = cadcraft_io::write_dxf(&cadcraft_doc::Drawing::new_metric());
    let mut tags = cadcraft_dxf::parse(source.as_bytes()).unwrap();
    let mut table_style = false;
    let mut count = 0;
    for tag in &mut tags {
        if tag.code == 0 {
            table_style = tag.str() == "TABLESTYLE";
        } else if table_style && tag.code == 90 {
            *tag = cadcraft_dxf::Tag::i(90, 4);
            count += 1;
        }
    }
    assert!(count > 0);
    let drawing = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    let check = |what: &str, saved: &str, expected: i64| {
        let styles: Vec<_> = section(saved, "OBJECTS").into_iter().filter(|(kind, _)| kind == "TABLESTYLE").collect();
        assert!(!styles.is_empty(), "{what}");
        for (_, style) in styles {
            assert_eq!(style.iter().filter(|tag| tag.code == 90).map(cadcraft_dxf::Tag::i64).collect::<Vec<_>>(), vec![expected; 3], "{what}");
            assert!(!style.iter().any(|tag| tag.code == 1001 && tag.str() == "CADCRAFT"), "{what}: native formats need no private xdata");
        }
    };
    let dxf = cadcraft_io::write_dxf(&drawing);
    check("DXF", &dxf, 4);
    check("DXF reopened", &cadcraft_io::write_dxf(&cadcraft_io::read_dxf(dxf.as_bytes()).unwrap()), 4);
    // R2000/R2004 DWG has no per-cell value-format fields. R2007 and later store them natively.
    for (version, expected) in
        [(cadcraft_io::Version::R2004, 512), (cadcraft_io::Version::R2007, 4), (cadcraft_io::Version::R2010, 4), (cadcraft_io::Version::R2018, 4)]
    {
        let options = cadcraft_io::SaveOptions { version: Some(version), binary: false };
        let dwg = cadcraft_io::write_with(&drawing, "value-formats.dwg", &options).unwrap();
        let native = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
        check(&format!("{version:?} native"), &native, expected);
        let reopened = cadcraft_io::write_dxf(&cadcraft_io::read(&dwg, "value-formats.dwg").unwrap());
        check(&format!("{version:?} reopened"), &reopened, expected);
    }
}

#[test]
fn a_table_style_is_written_in_autocads_form() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert!(!d.table_styles.iter().find(|s| s.name == "Schedule").unwrap().title, "the title is suppressed by the last 280");
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (_, style) = section(&text, "OBJECTS")
            .into_iter()
            .find(|(k, t)| k == "TABLESTYLE" && t.iter().any(|g| g.code == 5 && val(t, 3).is_empty()))
            .unwrap_or_else(|| panic!("{what}: no style"));
        let after = |from: usize| style.iter().skip(from).take_while(|t| t.code < 1000).map(|t| t.code).collect::<Vec<_>>();
        let start = style.iter().position(|t| t.code == 100 && t.str() == "AcDbTableStyle").unwrap() + 1;
        assert_eq!(after(start).first(), Some(&280), "{what}: the version comes first");
        let cell = style.iter().position(|t| t.code == 7).unwrap();
        assert_eq!(after(cell)[..15], [7, 140, 170, 62, 63, 283, 90, 91, 1, 274, 284, 64, 275, 285, 65], "{what}");
    }
}

/// AutoCAD's own Standard table style (DPInc, handle 93): Data 4.5 aligned 2, Title 6.0 and Header 4.5 aligned 5, margins 1.5.
fn autocad_standard_style() -> String {
    let mut g: Vec<(i32, &str)> = vec![
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (5, "C"),
        (330, "0"),
        (100, "AcDbDictionary"),
        (3, "ACAD_TABLESTYLE"),
        (350, "22"),
        (0, "DICTIONARY"),
        (5, "22"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (3, "Standard"),
        (350, "93"),
        (0, "TABLESTYLE"),
        (5, "93"),
        (330, "22"),
        (100, "AcDbTableStyle"),
        (3, "Standard"),
        (70, "0"),
        (71, "0"),
        (40, "1.5"),
        (41, "1.5"),
        (280, "0"),
        (281, "0"),
    ];
    for (height, align) in [("4.5", "2"), ("6.0", "5"), ("4.5", "5")] {
        g.extend([(7, "Standard"), (140, height), (170, align), (62, "0"), (63, "7"), (283, "0"), (90, "512"), (91, "0"), (1, "")]);
        for k in 0..6 {
            let (lw, vis, col) = (
                ["274", "275", "276", "277", "278", "279"][k],
                ["284", "285", "286", "287", "288", "289"][k],
                ["64", "65", "66", "67", "68", "69"][k],
            );
            g.extend([(lw.parse().unwrap(), "-2"), (vis.parse().unwrap(), "1"), (col.parse().unwrap(), "0")]);
        }
    }
    g.extend([(0, "ENDSEC"), (0, "EOF")]);
    dxf(&g)
}

#[test]
fn an_unedited_style_that_the_native_groups_store_writes_no_private_xdata() {
    let drawing = cadcraft_io::read_dxf(autocad_standard_style().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&drawing)) {
        let style = section(&text, "OBJECTS")
            .into_iter()
            .find(|(kind, tags)| kind == "TABLESTYLE" && tags.iter().any(|t| t.code == 3 && t.str() == "Standard"));
        let (_, style) = style.unwrap_or_else(|| panic!("{what}: the style"));
        assert!(!style.iter().any(|tag| tag.code == 1001 && tag.str() == "CADCRAFT"), "{what}: no private xdata for a native style");
        let heights: Vec<_> = style.iter().filter(|t| t.code == 140).map(cadcraft_dxf::Tag::f64).collect();
        assert_eq!(heights, [4.5, 6.0, 4.5], "{what}: the native heights stay");
    }
}

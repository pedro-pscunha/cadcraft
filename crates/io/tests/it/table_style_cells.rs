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
        let style = section(&text, "OBJECTS")
            .into_iter()
            .find(|(kind, tags)| kind == "TABLESTYLE" && tags.iter().any(|tag| tag.code == 3 && tag.str() == "SOURCE_DATA_FORMAT"))
            .unwrap()
            .1;
        assert_eq!(style.iter().filter(|tag| tag.code == 90).map(cadcraft_dxf::Tag::i64).collect::<Vec<_>>(), vec![4, 512, 4], "{what}");
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

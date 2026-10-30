//! The columns of an MTEXT reach a DWG of AutoCAD 2007 to 2013 as the ACAD extended data AutoCAD keeps them in. The DWG bridge wrote the
//! embedded object of 2018 whatever the version, which those DWGs have no place for: Citycon-2010 lost the columns of 63 texts.

use cadcraft_doc::{Common, Drawing, EntityKind, MText, MTextColumnType, MTextColumns, Space};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn drawing(columns: MTextColumns) -> Drawing {
    let text = MText {
        insert: Vec3::new(1.0, 2.0, 0.0),
        height: 0.5,
        width: 6.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "one\\Ptwo\\Pthree".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: Some(columns),
        direction: None,
        drawing_direction: None,
    };
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::MText(text)).unwrap();
    d
}

fn columns_after_dwg(d: &Drawing, version: Version) -> Option<MTextColumns> {
    let dwg = cadcraft_io::write_with(d, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    back.model.iter().find_map(|e| if let EntityKind::MText(t) = &e.kind { t.columns.clone() } else { None })
}

#[test]
fn static_and_dynamic_columns_survive_a_dwg_of_2007_to_2018() {
    let dynamic = MTextColumns {
        kind: MTextColumnType::Dynamic,
        count: 2,
        gutter: 0.5,
        height: 4.0,
        auto_height: false,
        heights: vec![4.0, 3.0],
        flow_reversed: false,
        extents: None,
        negated_heights: Vec::new(),
    };
    let fixed = MTextColumns { kind: MTextColumnType::Static, count: 3, gutter: 1.25, height: 7.5, ..Default::default() };
    for columns in [dynamic, fixed] {
        let d = drawing(columns.clone());
        for version in [Version::R2007, Version::R2010, Version::R2013, Version::R2018] {
            let got = columns_after_dwg(&d, version).unwrap_or_else(|| panic!("{version:?}: the columns are gone"));
            assert_eq!((got.kind, got.count, got.flow_reversed), (columns.kind, columns.count, columns.flow_reversed), "{version:?}");
            assert!((got.gutter - columns.gutter).abs() < 1e-9, "{version:?}");
        }
    }
}

/// The groups 46 of the embedded column objects in the DXF text of a DWG, in file order.
fn embedded_column_heights(dwg: &[u8]) -> Vec<f64> {
    let dxf = String::from_utf8(cadcraft_dwg::dwg_to_dxf(dwg).unwrap()).unwrap();
    let lines: Vec<&str> = dxf.lines().collect();
    let mut out = Vec::new();
    let mut in_object = false;
    for pair in lines.chunks(2) {
        let [code, value] = pair else { continue };
        match code.trim() {
            "0" => in_object = false,
            "101" => in_object = true,
            "46" if in_object => out.push(value.trim().parse().unwrap()),
            _ => {}
        }
    }
    out
}

/// A text of paper space whose DXF gives the column height AutoCAD's DWG has: the negative of the extents height (a DWG source's text).
fn negated_height_dxf() -> String {
    let text = "  0\nMTEXT\n  5\nE76F4\n100\nAcDbEntity\n 67\n1\n  8\n0\n100\nAcDbMText\n 10\n10\n 20\n20\n 30\n0\n 40\n0.15\n 41\n2.8\n 71\n1\n 72\n5\n  1\nvent\n  7\nStandard\n 11\n1\n 21\n0\n 31\n0\n 42\n0.87\n 43\n0.4\n 73\n1\n 44\n1\n101\nEmbedded Object\n 70\n1\n 10\n1\n 20\n0\n 30\n0\n 11\n10\n 21\n20\n 31\n0\n 40\n2.8\n 41\n0\n 42\n0.87\n 43\n0.4\n 71\n2\n 72\n2\n 44\n2.8\n 45\n1\n 73\n0\n 74\n0\n 46\n-0.4\n 46\n2.5\n";
    format!("  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1032\n  0\nENDSEC\n  0\nSECTION\n  2\nENTITIES\n{text}  0\nENDSEC\n  0\nEOF\n")
}

/// A column height the file gave as the negative extents height (AutoCAD's own DWG, outside model space too) stays negated through a DWG
/// save instead of becoming 0, and a height that was set stays as it is.
#[test]
fn a_negated_column_height_of_the_source_survives_a_dwg_save() {
    let d = cadcraft_io::read_dxf(negated_height_dxf().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    assert_eq!(embedded_column_heights(&dwg), vec![-0.4, 2.5]);
}

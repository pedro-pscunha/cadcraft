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

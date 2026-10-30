//! Named VIEW camera fields and UCS elevation are source data, not writer defaults.
use cadcraft_dxf::Tag;

const SOURCE: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nVIEW\n70\n1\n0\nVIEW\n5\n60\n100\nAcDbSymbolTableRecord\n100\nAcDbViewTableRecord\n2\nCamera\n70\n0\n40\n20\n41\n30\n10\n4\n20\n5\n11\n1\n21\n2\n31\n3\n12\n4\n22\n5\n32\n6\n42\n72\n43\n2.5\n44\n80\n50\n10\n71\n23\n281\n2\n72\n1\n110\n1\n120\n2\n130\n3\n111\n1\n121\n0\n131\n0\n112\n0\n122\n1\n132\n0\n79\n0\n146\n7\n0\nENDTAB\n\
0\nTABLE\n2\nUCS\n70\n1\n0\nUCS\n5\n70\n100\nAcDbSymbolTableRecord\n100\nAcDbUCSTableRecord\n2\nSurvey\n70\n0\n10\n7\n20\n8\n30\n9\n11\n1\n21\n0\n31\n0\n12\n0\n22\n1\n32\n0\n79\n0\n146\n12.25\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n";

fn record(bytes: &[u8], kind: &str, name: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|s| s.name == "TABLES")
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .find(|(k, t)| k == kind && t.iter().any(|t| t.code == 2 && t.str() == name))
        .unwrap()
        .1
}

fn assert_fields(bytes: &[u8]) {
    let fields = [
        (
            "VIEW",
            "Camera",
            vec![
                (11, 1.0),
                (21, 2.0),
                (31, 3.0),
                (12, 4.0),
                (22, 5.0),
                (32, 6.0),
                (42, 72.0),
                (43, 2.5),
                (44, 80.0),
                (71, 23.0),
                (281, 2.0),
                (72, 1.0),
            ],
        ),
        ("UCS", "Survey", vec![(146, 12.25)]),
    ];
    for (kind, name, expected) in fields {
        let tags = record(bytes, kind, name);
        for (code, value) in expected {
            let values: Vec<_> = tags.iter().filter(|t| t.code == code).map(Tag::f64).collect();
            assert_eq!(values.len(), 1, "{kind} {code}: {values:?}");
            assert!((values[0] - value).abs() < 1e-9, "{kind} {code}: {values:?} != {value}");
        }
    }
}

#[test]
fn camera_and_ucs_fields_survive_two_saves() {
    for format in ["saved.dxf", "saved.dwg"] {
        let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
        for _ in 0..2 {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let text = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            assert_fields(&text);
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

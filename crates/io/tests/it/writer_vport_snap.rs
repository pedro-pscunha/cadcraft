//! Snap/grid state belongs to each VPORT tile, not to the drawing header as a whole.
use cadcraft_doc::{HVal, geom::Vec3};
use cadcraft_dxf::Tag;

fn source() -> String {
    let mut text = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nVPORT\n70\n3\n".to_string();
    for (name, handle, left, right, values) in [
        ("*ACTIVE", "50", 0.0, 0.5, [1.0, 2.0, 0.25, 0.75, 5.0, 10.0, 15.0]),
        ("*ACTIVE", "51", 0.5, 1.0, [3.0, 4.0, 1.5, 2.5, 11.0, 12.0, 35.0]),
        ("CameraGrid", "52", 0.0, 1.0, [7.0, 8.0, 0.8, 0.9, 21.0, 22.0, 65.0]),
    ] {
        text.push_str(&format!("0\nVPORT\n5\n{handle}\n100\nAcDbSymbolTableRecord\n100\nAcDbViewportTableRecord\n2\n{name}\n70\n0\n10\n{left}\n20\n0\n11\n{right}\n21\n1\n12\n5\n22\n6\n40\n10\n41\n1.5\n"));
        for (code, value) in [13, 23, 14, 24, 15, 25, 50].into_iter().zip(values) {
            text.push_str(&format!("{code}\n{value}\n"));
        }
    }
    text.push_str("0\nENDTAB\n0\nENDSEC\n0\nEOF\n");
    text
}

fn values(bytes: &[u8]) -> Vec<[f64; 7]> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|s| s.name == "TABLES")
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .filter(|(k, _)| k == "VPORT")
        .map(|(_, tags)| [13, 23, 14, 24, 15, 25, 50].map(|code| tags.iter().find(|t| t.code == code).map(Tag::f64).unwrap()))
        .collect()
}

#[test]
fn each_tiles_snap_and_grid_fields_survive_two_saves() {
    let source = source();
    let expected = values(source.as_bytes());
    for format in ["saved.dxf", "saved.dwg"] {
        let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        for _ in 0..2 {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let text = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            let actual = values(&text);
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(&expected) {
                assert!(actual.iter().zip(expected).all(|(a, e)| (a - e).abs() < 1e-9), "{format}: {actual:?} != {expected:?}");
            }
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

#[test]
fn header_snap_edits_change_only_the_current_active_tile() {
    let source = source();
    let mut expected = values(source.as_bytes());
    let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    drawing.header.set("SNAPBASE", HVal::Point(Vec3::new(20.0, 30.0, 0.0)));
    drawing.header.set("SNAPUNIT", HVal::Point(Vec3::new(2.0, 3.0, 0.0)));
    drawing.header.set("GRIDUNIT", HVal::Point(Vec3::new(4.0, 6.0, 0.0)));
    drawing.header.set_f64("SNAPANG", 75.0);
    expected[0] = [20.0, 30.0, 2.0, 3.0, 4.0, 6.0, 75.0];
    assert_eq!(values(&cadcraft_io::write(&drawing, "edited.dxf").unwrap()), expected);
}

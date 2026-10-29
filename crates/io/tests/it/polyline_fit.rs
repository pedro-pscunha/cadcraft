//! Fitted heavy polylines keep their control vertices and editing data on a save.
use cadcraft_dxf::Tag;

fn fixture(spline: bool) -> String {
    let (flags, smooth, vertices) =
        if spline { (4, 6, vec![(16, 0.0), (16, 25.0), (8, 0.0), (8, 10.0), (8, 20.0)]) } else { (2, 0, vec![(0, 15.0), (1, 35.0), (0, 65.0)]) };
    let mut text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n100\nAcDb2dPolyline\n66\n1\n10\n0\n20\n0\n30\n2\n70\n{flags}\n75\n{smooth}\n"
    );
    for (i, (flags, tangent)) in vertices.iter().enumerate() {
        let x = i * 2;
        text.push_str(&format!(
            "0\nVERTEX\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n{x}\n20\n3\n30\n0\n40\n0.75\n41\n0.25\n42\n0.125\n70\n{flags}\n50\n{tangent}\n"
        ));
    }
    text.push_str("0\nSEQEND\n8\n0\n0\nENDSEC\n0\nEOF\n");
    text
}

#[derive(Debug, PartialEq)]
struct Fit {
    flags: f64,
    smooth: f64,
    vertices: Vec<[f64; 8]>,
}

fn fit(dxf: &[u8]) -> Fit {
    let tags = cadcraft_dxf::parse_ascii(std::str::from_utf8(dxf).unwrap()).unwrap();
    let mut starts: Vec<_> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    starts.push(tags.len());
    let records: Vec<_> = starts.windows(2).map(|w| &tags[w[0]..w[1]]).collect();
    let value = |record: &[Tag], code| record.iter().find(|t| t.code == code).map_or(0.0, |t| t.str().parse::<f64>().unwrap());
    let polyline = records.iter().find(|r| r.first().is_some_and(|t| t.str() == "POLYLINE")).unwrap();
    let vertices = records
        .iter()
        .filter(|r| r.first().is_some_and(|t| t.str() == "VERTEX"))
        .map(|r| [10, 20, 30, 40, 41, 42, 70, 50].map(|code| value(r, code)))
        .collect();
    Fit { flags: value(polyline, 70), smooth: value(polyline, 75), vertices }
}

#[test]
fn curve_fit_and_spline_fit_survive_two_saves_in_both_formats() {
    for spline in [false, true] {
        let source = fixture(spline);
        let expected = fit(source.as_bytes());
        for format in ["fitted.dxf", "fitted.dwg"] {
            let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
            let cadcraft_doc::EntityKind::LwPolyline(polyline) = &drawing.model.iter().next().unwrap().kind else { panic!("polyline") };
            assert_eq!(polyline.vertices.len(), 3, "only fitted vertices are displayed");
            for _ in 0..2 {
                let bytes = cadcraft_io::write(&drawing, format).unwrap();
                let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes.clone() };
                let actual = fit(&dxf);
                assert_eq!(
                    (actual.flags, actual.smooth, actual.vertices.len()),
                    (expected.flags, expected.smooth, expected.vertices.len()),
                    "spline={spline}, {format}"
                );
                for (actual, expected) in actual.vertices.iter().zip(&expected.vertices) {
                    assert!(actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-9), "spline={spline}, {format}: {actual:?} != {expected:?}");
                }
                drawing = cadcraft_io::read(&bytes, format).unwrap();
            }
        }
    }
}

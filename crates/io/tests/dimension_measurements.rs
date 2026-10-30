use cadcraft_doc::{Drawing, Handle};

fn source() -> Drawing {
    let mut text = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (handle, kind, measurement, defpt, text_mid) in [
        ("1182", 32, 10.0, [0.0, -3.0, 0.0], [1.125, 5.0, 0.0]),
        ("119A", 33, 10.0, [0.0, 3.0, 0.0], [-4.875, 5.0, 0.0]),
        ("11B2", 34, std::f64::consts::FRAC_PI_4, [5.65685424949238, 0.0, 8.485281374238571], [7.985565360580435, 1.218253535365311, 0.0]),
    ] {
        text.push_str(&format!("0\nDIMENSION\n5\n{handle}\n100\nAcDbEntity\n8\n0\n100\nAcDbDimension\n70\n{kind}\n42\n{measurement}\n210\n-0.7071067811865476\n220\n0\n230\n0.7071067811865476\n"));
        for (code, point) in [(10, defpt), (11, text_mid)] {
            for (offset, value) in [0, 10, 20].into_iter().zip(point) {
                text.push_str(&format!("{}\n{value}\n", code + offset));
            }
        }
        if kind == 34 {
            text.push_str("100\nAcDb2LineAngularDimension\n13\n-1.414213562373095\n23\n0\n33\n1.414213562373095\n14\n2.82842712474619\n24\n6\n34\n5.65685424949238\n15\n-1.414213562373095\n25\n0\n35\n1.414213562373095\n16\n-2.692582403567251\n26\n4.663689526544413\n36\n0\n");
        } else {
            text.push_str("100\nAcDbAlignedDimension\n13\n5.65685424949238\n23\n0\n33\n8.485281374238571\n14\n-1.414213562373095\n24\n0\n34\n1.414213562373095\n");
            if kind == 32 {
                text.push_str("50\n90\n100\nAcDbRotatedDimension\n");
            }
        }
    }
    text.push_str("0\nENDSEC\n0\nEOF\n");
    cadcraft_io::read(text.as_bytes(), "source.dxf").unwrap()
}

fn measurement(drawing: &Drawing, handle: u64) -> f64 {
    let cadcraft_doc::EntityKind::Dimension(dim) = &drawing.entity(Handle(handle)).unwrap().kind else { panic!("dimension") };
    dim.measurement
}

#[test]
fn unchanged_tilted_dimensions_keep_the_source_measurement_in_dxf_and_dwg() {
    let drawing = source();
    for name in ["unchanged.dxf", "unchanged.dwg"] {
        let bytes = cadcraft_io::write(&drawing, name).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        for (handle, expected) in [(0x1182, 10.0), (0x119A, 10.0), (0x11B2, std::f64::consts::FRAC_PI_4)] {
            assert_eq!(measurement(&drawing, handle), expected);
            assert_eq!(measurement(&reopened, handle), expected, "{name}: {handle:X}");
        }
    }
}

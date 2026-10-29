//! A 2D POLYLINE keeps its heavy representation through either save format.
use cadcraft_doc::EntityKind;

const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n100\nAcDbEntity\n8\n0\n100\nAcDb2dPolyline\n66\n1\n10\n0\n20\n0\n30\n2\n70\n129\n40\n0.5\n41\n0.5\n\
0\nVERTEX\n5\n101\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n1\n20\n3\n30\n0\n40\n1\n41\n0.25\n42\n0.5\n70\n0\n\
0\nVERTEX\n5\n102\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n10\n20\n3\n30\n0\n70\n0\n\
0\nSEQEND\n5\n103\n330\n100\n100\nAcDbEntity\n8\n0\n0\nENDSEC\n0\nEOF\n";

#[test]
fn a_heavy_polyline_keeps_its_type_vertices_and_properties() {
    check("saved.dxf");
}

#[test]
fn a_heavy_polyline_keeps_its_dwg_child_identities() {
    check("saved.dwg");
}

fn check(format: &str) {
    let original = cadcraft_io::read_dxf(DXF.as_bytes()).unwrap();
    let original_poly = original
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::LwPolyline(p) => Some(p),
            _ => None,
        })
        .unwrap();
    {
        let mut drawing = original.clone();
        for _ in 0..2 {
            let bytes = cadcraft_io::write(&drawing, format).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes.clone() };
            let tags = cadcraft_dxf::parse_ascii(&String::from_utf8(dxf).unwrap()).unwrap();
            assert_eq!(tags.iter().filter(|t| t.code == 0 && t.str() == "POLYLINE").count(), 1, "{format}");
            assert_eq!(tags.iter().filter(|t| t.code == 0 && t.str() == "VERTEX").count(), 2, "{format}");
            assert_eq!(tags.iter().filter(|t| t.code == 0 && t.str() == "SEQEND").count(), 1, "{format}");
            assert!(!tags.iter().any(|t| t.code == 0 && t.str() == "LWPOLYLINE"));
            drawing = cadcraft_io::read(&bytes, format).unwrap();
            let poly = drawing
                .model
                .iter()
                .find_map(|e| match &e.kind {
                    EntityKind::LwPolyline(p) => Some(p),
                    _ => None,
                })
                .unwrap();
            assert_eq!(poly, original_poly, "{format}");
        }
    }
}

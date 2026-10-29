use cadcraft_dxf::{Tag, records, sections};

fn fixture(flags: i64) -> Vec<u8> {
    let vertex_flags = if flags == 4 {
        16
    } else if flags == 8 {
        32
    } else {
        0
    };
    let fitted = if flags == 4 {
        "0\nVERTEX\n5\n103\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n0\n20\n0\n70\n8\n0\nVERTEX\n5\n104\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n5\n20\n3\n70\n8\n0\nVERTEX\n5\n105\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n10\n20\n0\n70\n8\n"
    } else {
        ""
    };
    let source = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n0\nAPPID\n2\nVENDOR\n70\n0\n0\nENDTAB\n0\nTABLE\n2\nLAYER\n0\nLAYER\n2\nVertexLayer\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nLAYER\n2\nEndLayer\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n100\nAcDbEntity\n8\n0\n100\nAcDb2dPolyline\n66\n1\n70\n{flags}\n75\n{}\n0\nVERTEX\n5\n101\n102\n{{ACAD_XDICTIONARY\n360\n200\n102\n}}\n330\n100\n100\nAcDbEntity\n8\nVertexLayer\n62\n2\n6\nCONTINUOUS\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n0\n20\n0\n30\n2\n70\n{vertex_flags}\n1001\nVENDOR\n1000\nvertex-data\n0\nVERTEX\n5\n102\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\nAcDb2dVertex\n10\n10\n20\n0\n30\n2\n70\n{vertex_flags}\n{fitted}0\nSEQEND\n5\n190\n330\n100\n100\nAcDbEntity\n8\nEndLayer\n62\n3\n1001\nVENDOR\n1000\nend-data\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nReferences\n350\nB0\n0\nXRECORD\n5\nB0\n330\nC\n100\nAcDbXrecord\n280\n1\n340\n101\n340\n190\n0\nDICTIONARY\n5\n200\n330\n101\n100\nAcDbDictionary\n281\n1\n3\nData\n350\n201\n0\nXRECORD\n5\n201\n330\n200\n100\nAcDbXrecord\n280\n1\n1\nvertex-extension\n0\nENDSEC\n0\nEOF\n",
        if flags == 4 { 6 } else { 0 }
    );
    if flags == 8 {
        source.replace("AcDb2dPolyline", "AcDb3dPolyline").replace("AcDb2dVertex", "AcDb3dPolylineVertex").into_bytes()
    } else {
        source.into_bytes()
    }
}

fn check(bytes: &[u8], stage: &str) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let recs: Vec<_> = sections(&tags).into_iter().flat_map(|section| records(&section.tags)).collect();
    let record = |kind: &str, handle: &str| {
        recs.iter().find(|(k, tags)| k == kind && tags.iter().any(|t| t.code == 5 && t.str() == handle)).unwrap_or_else(|| {
            panic!(
                "{stage}: missing {kind} {handle}: {:?}",
                recs.iter().filter(|(kind, _)| matches!(kind.as_str(), "VERTEX" | "SEQEND" | "XRECORD")).collect::<Vec<_>>()
            )
        })
    };
    let (_, vertex) = record("VERTEX", "101");
    assert!(vertex.iter().any(|t| t.code == 8 && t.str() == "VertexLayer"), "{stage}: vertex layer");
    assert!(vertex.iter().any(|t| t.code == 62 && t.i64() == 2), "{stage}: vertex color");
    // A DWG names the linetype by a handle, so it comes back with the table spelling ("Continuous").
    assert!(vertex.iter().any(|t| t.code == 6 && t.str().eq_ignore_ascii_case("CONTINUOUS")), "{stage}: vertex linetype");
    assert!(vertex.iter().any(|t| t.code == 1000 && t.str() == "vertex-data"), "{stage}: vertex xdata");
    let (_, end) = record("SEQEND", "190");
    assert!(end.iter().any(|t| t.code == 8 && t.str() == "EndLayer"), "{stage}: seqend layer");
    assert!(end.iter().any(|t| t.code == 62 && t.i64() == 3), "{stage}: seqend color");
    assert!(end.iter().any(|t| t.code == 1000 && t.str() == "end-data"), "{stage}: seqend xdata");
    let references = recs.iter().find(|(kind, tags)| kind == "XRECORD" && tags.iter().any(|t| t.code == 340)).unwrap();
    assert!(references.1.contains(&Tag::s(340, "101")), "{stage}: vertex reference");
    assert!(references.1.contains(&Tag::s(340, "190")), "{stage}: seqend reference");
    let dictionary = vertex.iter().find(|t| t.code == 360).expect("vertex extension dictionary").str();
    let (_, dict) = record("DICTIONARY", &dictionary);
    let child = dict.iter().find(|t| t.code == 350).unwrap().str();
    assert!(record("XRECORD", &child).1.iter().any(|t| t.code == 1 && t.str() == "vertex-extension"));
}

#[test]
fn polyline_children_keep_identity_in_user_dxf() {
    for flags in [0, 2, 4, 8] {
        let mut drawing = cadcraft_io::read_dxf(&fixture(flags)).unwrap();
        for cycle in 0..2 {
            let dxf = cadcraft_io::write_dxf(&drawing);
            check(dxf.as_bytes(), &format!("DXF {flags}/{cycle}"));
            drawing = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
        }
    }
}

#[test]
fn copied_polyline_children_get_distinct_identities_and_owned_extras() {
    for flags in [0, 4, 8] {
        let mut drawing = cadcraft_io::read_dxf(&fixture(flags)).unwrap();
        let original = (**drawing.entity(cadcraft_doc::Handle(0x100)).unwrap()).clone();
        for copy in drawing.duplicate_entities(vec![vec![original]], cadcraft_doc::DuplicationSource::SameDrawing) {
            drawing.model.push(copy);
        }
        let output = cadcraft_io::write_dxf(&drawing);
        let tags = cadcraft_dxf::parse(output.as_bytes()).unwrap();
        let recs: Vec<_> = sections(&tags).into_iter().flat_map(|section| records(&section.tags)).collect();
        let vertices: Vec<_> =
            recs.iter().filter(|(kind, tags)| kind == "VERTEX" && tags.iter().any(|t| t.code == 1000 && t.str() == "vertex-data")).collect();
        assert_eq!(vertices.len(), 2, "{flags}: copied vertex xdata");
        let handles: std::collections::HashSet<_> = vertices.iter().map(|(_, tags)| tags.iter().find(|t| t.code == 5).unwrap().str()).collect();
        assert_eq!(handles.len(), 2, "{flags}: distinct vertex identities");
        for (_, vertex) in vertices {
            let handle = vertex.iter().find(|t| t.code == 5).unwrap().str();
            let dictionary = vertex.iter().find(|t| t.code == 360).unwrap().str();
            assert!(
                recs.iter()
                    .any(|(kind, tags)| kind == "DICTIONARY" && tags.contains(&Tag::s(5, &dictionary)) && tags.contains(&Tag::s(330, &handle))),
                "{flags}: copied dictionary owner"
            );
        }
    }
}

#[test]
fn heavy_polyline_children_keep_identity_common_data_and_foreign_extras_in_dwg() {
    for flags in [0, 4, 8] {
        let mut drawing = cadcraft_io::read_dxf(&fixture(flags)).unwrap();
        for cycle in 0..2 {
            let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
            let dwg = cadcraft_io::write_with(&drawing, "x.dwg", &opts).unwrap();
            check(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap(), &format!("DWG {flags}/{cycle}"));
            drawing = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        }
    }
}

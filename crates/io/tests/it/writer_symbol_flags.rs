//! External dependency and resolution are source flags, not a consequence of a qualified name.
use cadcraft_dxf::Tag;

const SYMBOLS: [(&str, &str, &str); 6] = [
    ("LTYPE", "xref|DASHED", "AcDbLinetypeTableRecord"),
    ("LAYER", "xref|Walls", "AcDbLayerTableRecord"),
    ("VIEW", "xref|Camera", "AcDbViewTableRecord"),
    ("UCS", "xref|Survey", "AcDbUCSTableRecord"),
    ("VPORT", "xref|Grid", "AcDbViewportTableRecord"),
    ("APPID", "xref|App", "AcDbRegAppTableRecord"),
];

fn source(flags: i64) -> String {
    let mut text = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n".to_string();
    for (i, (kind, name, class)) in SYMBOLS.iter().enumerate() {
        text.push_str(&format!(
            "0\nTABLE\n2\n{kind}\n70\n1\n0\n{kind}\n5\n{:X}\n100\nAcDbSymbolTableRecord\n100\n{class}\n2\n{name}\n70\n{flags}\n",
            0x50 + i
        ));
        text.push_str(match *kind {
            "LTYPE" => "3\nsource\n72\n65\n73\n0\n40\n0\n",
            "LAYER" => "62\n7\n6\nxref|DASHED\n",
            "VIEW" => "40\n10\n41\n15\n10\n1\n20\n2\n",
            "UCS" => "10\n1\n20\n2\n30\n3\n11\n1\n21\n0\n31\n0\n12\n0\n22\n1\n32\n0\n",
            "VPORT" => "10\n0\n20\n0\n11\n1\n21\n1\n12\n1\n22\n2\n40\n10\n41\n1.5\n",
            _ => "",
        });
        text.push_str("0\nENDTAB\n");
    }
    text.push_str("0\nTABLE\n2\nBLOCK_RECORD\n70\n1\n0\nBLOCK_RECORD\n5\n70\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\nxref\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n71\n330\n70\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\nxref\n70\n4\n10\n0\n20\n0\n30\n0\n3\nxref\n1\nmissing.dwg\n0\nENDBLK\n5\n72\n330\n70\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockEnd\n0\nENDSEC\n0\nEOF\n");
    text
}

fn assert_flags(bytes: &[u8], flags: i64, format: &str) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let records: Vec<_> =
        cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name == "TABLES").flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
    let mut failures = Vec::new();
    for (kind, name, _) in SYMBOLS {
        let Some((_, body)) = records.iter().find(|(k, t)| k == kind && t.iter().any(|t| t.code == 2 && t.str().eq_ignore_ascii_case(name))) else {
            failures.push(format!("missing {kind} {name}"));
            continue;
        };
        let flags70: Vec<_> = body.iter().filter(|t| t.code == 70).map(Tag::i64).collect();
        assert_eq!(flags70.len(), 1, "{kind}: {flags70:?}");
        if flags70[0] & 48 != flags {
            failures.push(format!("{kind}: {} != {flags}", flags70[0] & 48));
        }
    }
    assert!(failures.is_empty(), "{format}: {failures:?}");
}

fn round_trip(format: &str, version: cadcraft_io::Version) {
    for flags in [16, 48] {
        let mut drawing = cadcraft_io::read_dxf(source(flags).as_bytes()).unwrap();
        for _ in 0..2 {
            let options = cadcraft_io::SaveOptions { version: Some(version), binary: false };
            let saved = cadcraft_io::write_with(&drawing, format, &options).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            // From R2007 DWG packs xref index/resolution into one field. Native dependent
            // entries normalize to resolved, while DXF and pre-R2007 DWG keep both bits.
            let expected = if format.ends_with("dwg") && version >= cadcraft_io::Version::R2007 { 48 } else { flags };
            assert_flags(&dxf, expected, format);
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

#[test]
fn external_dependency_and_resolution_flags_survive_user_dxf() {
    round_trip("saved.dxf", cadcraft_io::Version::R2018);
}

#[test]
fn external_dependency_and_resolution_flags_survive_dwg() {
    for version in [cadcraft_io::Version::R2004, cadcraft_io::Version::R2007, cadcraft_io::Version::R2018] {
        round_trip("saved.dwg", version);
    }
}

//! Text with accents keeps its characters in a DWG save. The DWG bridge reads our R2000 DXF, whose
//! text is in its declared Windows single-byte page: a character that page holds is written as
//! its byte, because the bridge keeps a `\U+XXXX` escape of such a character as text (acadrust
//! 0.6.8) and the DWG then stored `S\U+00E3o` instead of `São`. Other characters stay escapes,
//! which the bridge decodes.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn accented_text_is_stored_as_characters_in_dwg() {
    use std::io::Cursor;

    use acadrust::entities::EntityType;

    let values = ["Grama São Carlos", "39,4° CIRCULAÇÃO €5", "日本語 ok"];
    let mut dxf = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (i, v) in values.iter().enumerate() {
        dxf += &format!("0\nTEXT\n5\n{:X}\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n{i}\n30\n0\n40\n1\n1\n{v}\n100\nAcDbText\n", 0x30 + i);
    }
    dxf += "0\nMTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbMText\n10\n0\n20\n9\n30\n0\n40\n1\n1\nÁrvore nº 3\n0\nENDSEC\n0\nEOF\n";
    let d = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let stored = acadrust::DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
    let mut texts: Vec<String> = stored
        .entities()
        .filter_map(|e| match e {
            EntityType::Text(t) => Some(t.value.clone()),
            EntityType::MText(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    texts.sort();
    let mut want: Vec<String> = values.iter().map(|v| v.to_string()).chain(["Árvore nº 3".to_string()]).collect();
    want.sort();
    assert_eq!(texts, want);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn mtext_escaped_unicode_looking_text_survives_dwg() {
    let literal = r"\\U+00E9";
    let source = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nMTEXT\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbMText\n10\n0\n20\n0\n30\n0\n40\n1\n1\n{literal}\n0\nENDSEC\n0\nEOF\n"
    );
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let bytes = cadcraft_io::write(&d, "x.dwg").unwrap();
    let native = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes.clone())).read().unwrap();
    let values: Vec<_> =
        native.entities().filter_map(|e| if let acadrust::entities::EntityType::MText(t) = e { Some(t.value.as_str()) } else { None }).collect();
    assert_eq!(values, [literal]);
    let back = cadcraft_io::read(&bytes, "x.dwg").unwrap();
    assert!(back.model.iter().any(|e| matches!(&e.kind,cadcraft_doc::EntityKind::MText(t) if t.contents==literal)));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn unicode_dwg_targets_retain_multibyte_and_other_source_code_pages() {
    let text = "日本 中文 한글 Ł Café";
    let source = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\n{text}\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n"
    );
    for (page, canonical) in [
        ("ANSI_932", "ANSI_932"),
        ("ANSI_936", "GB2312"),
        ("ANSI_949", "KOREAN"),
        ("ANSI_950", "BIG5"),
        ("DOS437", "DOS437"),
        ("ISO8859-2", "ISO8859-2"),
        ("UTF-8", "UTF-8"),
    ] {
        for version in [cadcraft_dxf::Version::R2010, cadcraft_dxf::Version::R2013, cadcraft_dxf::Version::R2018] {
            let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
            d.header.set_str("DWGCODEPAGE", page);
            let bytes = cadcraft_io::write_with(&d, "x.dwg", &cadcraft_io::SaveOptions { version: Some(version), binary: false }).unwrap();
            let native = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes)).read().unwrap();
            assert_eq!(native.header.code_page, canonical, "{page} / {version:?}: source code-page metadata was replaced");
            let values: Vec<_> = native
                .entities()
                .filter_map(|e| if let acadrust::entities::EntityType::Text(t) = e { Some(t.value.as_str()) } else { None })
                .collect();
            assert_eq!(values, [text], "{page} / {version:?}: header preservation must not corrupt Unicode text");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn r2007_backend_keeps_unicode_text_but_uses_1252_metadata() {
    let mut d=cadcraft_io::read_dxf("0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\n日本\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n".as_bytes()).unwrap();
    d.header.set_str("DWGCODEPAGE", "ANSI_932");
    let bytes =
        cadcraft_io::write_with(&d, "x.dwg", &cadcraft_io::SaveOptions { version: Some(cadcraft_dxf::Version::R2007), binary: false }).unwrap();
    let native = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes)).read().unwrap();
    assert_eq!(native.header.code_page, "ANSI_1252", "update the save note policy when the backend gains metadata support");
    assert!(native.entities().any(|e| matches!(e,acadrust::entities::EntityType::Text(t) if t.value=="日本")));
}

/// A drawing read with another supported single-byte page keeps that page in the DWG bridge.
/// Representable Cyrillic is encoded as bytes; the unrepresentable `é` remains an escape.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn text_of_a_drawing_with_another_code_page_survives_dwg() {
    use std::io::Cursor;

    use acadrust::entities::EntityType;

    let dxf = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DWGCODEPAGE\n3\nANSI_1251\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
               0\nTEXT\n5\n30\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\nCaf\\U+00E9 \\U+041F\\U+0440\\U+0438\n100\nAcDbText\n\
               0\nENDSEC\n0\nEOF\n";
    let d = cadcraft_io::read_dxf(dxf.as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let roundtrip = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    assert_eq!(roundtrip.header.str("DWGCODEPAGE", ""), "ANSI_1251", "a supported single-byte drawing page must not be replaced with 1252");
    let stored = acadrust::DwgReader::from_stream(Cursor::new(dwg)).read().unwrap();
    let texts: Vec<String> = stored.entities().filter_map(|e| if let EntityType::Text(t) = e { Some(t.value.clone()) } else { None }).collect();
    assert_eq!(texts, ["Café При"]);
}

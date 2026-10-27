//! Text with accents keeps its characters in a DWG save. The DWG bridge reads our R2000 DXF, whose
//! text is in its declared code page (ANSI_1252): a character that code page holds is written as
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

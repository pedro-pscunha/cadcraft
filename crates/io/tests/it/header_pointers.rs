//! Header variables that name an object by handle ($CMATERIAL, $INTERFEREOBJVS…) point at the
//! same object after a save. The save gives kept objects new handles, so the value is remapped,
//! never copied. Through DWG it also needs acadrust to carry these variables.

/// (code, value) pairs of an ASCII DXF.
fn pairs(dxf: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = dxf.lines().collect();
    lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim().to_string()))).collect()
}

/// The value of header variable `$name`.
fn header(p: &[(i32, String)], name: &str) -> Option<String> {
    let i = p.iter().position(|(c, v)| *c == 9 && v == name)?;
    p.get(i + 1).map(|(_, v)| v.clone())
}

/// The name (group 1) of the MATERIAL whose handle is `h`.
fn material_name(p: &[(i32, String)], h: &str) -> Option<String> {
    let i = p.windows(2).position(|w| w[0] == (0, "MATERIAL".to_string()) && w[1] == (5, h.to_string()))?;
    p.iter().skip(i + 2).take_while(|(c, _)| *c != 0).find(|(c, _)| *c == 1).map(|(_, v)| v.clone())
}

/// A header that names its current material, and that MATERIAL in the named object
/// dictionary, as AutoCAD writes them. The save numbers kept objects anew.
const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$CMATERIAL\n347\n3A\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nACAD_MATERIAL\n350\n3B\n\
0\nDICTIONARY\n5\n3B\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nByLayer\n350\n3A\n\
0\nMATERIAL\n5\n3A\n102\n{ACAD_REACTORS\n330\n3B\n102\n}\n330\n3B\n100\nAcDbMaterial\n1\nByLayer\n0\nENDSEC\n0\nEOF\n";

/// Saved as 2018 DXF: the variable is newer than the R2000 default.
fn save(d: &cadcraft_doc::Drawing) -> String {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap()
}

#[test]
fn the_current_material_points_at_the_saved_material() {
    let d = cadcraft_io::read_dxf(DXF.as_bytes()).unwrap();
    let dxf = pairs(&save(&d));
    let h = header(&dxf, "$CMATERIAL").expect("$CMATERIAL is written");
    assert_eq!(material_name(&dxf, &h).as_deref(), Some("ByLayer"), "$CMATERIAL {h}");
    // A second save points at it again.
    let again = pairs(&save(&cadcraft_io::read_dxf(save(&d).as_bytes()).unwrap()));
    let h = header(&again, "$CMATERIAL").expect("second save: $CMATERIAL is written");
    assert_eq!(material_name(&again, &h).as_deref(), Some("ByLayer"), "second save: $CMATERIAL {h}");
}

#[test]
fn a_header_pointer_whose_object_is_gone_becomes_null() {
    let d = cadcraft_io::read_dxf(DXF.replace("\n3\nByLayer\n350\n3A", "").replace("0\nMATERIAL\n5\n3A", "0\nXRECORD\n5\n3A").as_bytes()).unwrap();
    let dxf = pairs(&save(&d));
    assert_ne!(header(&dxf, "$CMATERIAL").as_deref(), Some("3A"), "the read handle is never copied");
}

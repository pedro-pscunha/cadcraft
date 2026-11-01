//! The ACIS geometry of 3D solids and regions survives a save. From AutoCAD 2013 on those entities
//! don't carry it themselves: it lives in the file's data store section (ACDSDATA), one record per
//! entity bound by a 320 handle. CADCraft doesn't model solids, so it keeps the section as read and
//! writes it back (and the DWG bridge reads it), without the records of deleted entities.

use cadcraft_doc::EntityKind;

/// Stand-in modeler data, carried as opaque bytes: the binary ACIS magic, some bytes, and the
/// end-of-data marker DWG readers delimit each body with.
const SAB_HEX: &str = "414349532042696E61727946696C65000102030405060708090A0B0C0D0E0F456E642D6F662D414349532D64617461";

/// A 2018 DXF with two 3D solids (2A, 2B) and their data store records.
fn dxf() -> String {
    let solid = |h: &str| {
        format!(
            "0\n3DSOLID\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n290\n0\n2\n{{00000000-0000-0000-0000-000000000000}}\n100\nAcDb3dSolid\n350\n0\n"
        )
    };
    let record = |h: &str| {
        format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{SAB_HEX}\n", SAB_HEX.len() / 2)
    };
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{}{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n2\n71\n2\n\
         0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n\
         {}{}0\nENDSEC\n0\nEOF\n",
        solid("2A"),
        solid("2B"),
        record("2A"),
        record("2B")
    )
}

/// The handles the ACDSDATA section of DXF text binds ASM_Data to, with each one's hex data.
fn asm_data(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let Some(start) = lines.windows(2).position(|w| w == ["2", "ACDSDATA"]) else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    let (mut handle, mut asm, mut hex) = (None::<String>, false, String::new());
    for pair in lines[start..].chunks(2) {
        let [code, value] = pair else { break };
        if *code == "0" {
            if let (Some(h), true, false) = (handle.take(), asm, hex.is_empty()) {
                out.push((h.to_ascii_uppercase(), std::mem::take(&mut hex)));
            }
            asm = false;
            hex.clear();
            if *value == "ENDSEC" {
                break;
            }
        }
        match *code {
            "320" => handle = Some(value.to_string()),
            "2" => asm = *value == "ASM_Data",
            "310" if asm => hex.push_str(value),
            _ => {}
        }
    }
    out
}

#[test]
fn dxf_keeps_the_data_store_and_drops_records_of_deleted_entities() {
    let mut d = cadcraft_io::read_dxf(dxf().as_bytes()).unwrap();
    assert_eq!(d.foreign.data_store_entities().len(), 2);
    let out = String::from_utf8(cadcraft_io::write(&d, "x.dxf").unwrap()).unwrap();
    assert_eq!(asm_data(&out), vec![("2A".to_string(), SAB_HEX.to_string()), ("2B".to_string(), SAB_HEX.to_string())]);

    // Deleting a solid drops its record; the other stays.
    let h = d.model.iter().find(|e| e.handle.hex() == "2B").map(|e| e.handle).unwrap();
    d.remove_entity(h).unwrap();
    let out = String::from_utf8(cadcraft_io::write(&d, "x.dxf").unwrap()).unwrap();
    assert_eq!(asm_data(&out), vec![("2A".to_string(), SAB_HEX.to_string())]);

    // An R2000 file has no data store (its solids would carry SAT text instead).
    let r2000 = String::from_utf8(
        cadcraft_io::write_with(&d, "x.dxf", &cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2000), binary: false }).unwrap(),
    )
    .unwrap();
    assert!(!r2000.contains("ACDSDATA"));

    // No solid left: no section.
    let h = d.model.iter().find(|e| matches!(e.kind, EntityKind::Unknown(_))).map(|e| e.handle).unwrap();
    d.remove_entity(h).unwrap();
    let out = String::from_utf8(cadcraft_io::write(&d, "x.dxf").unwrap()).unwrap();
    assert!(!out.contains("ACDSDATA"));
}

#[test]
fn dwg_keeps_the_solids_geometry() {
    let d = cadcraft_io::read_dxf(dxf().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    // The DWG read back (as the bridge's DXF) binds the same bytes to both solids.
    let back = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    let mut got = asm_data(&back);
    got.sort();
    assert_eq!(got, vec![("2A".to_string(), SAB_HEX.to_string()), ("2B".to_string(), SAB_HEX.to_string())]);
    // And reopening it in CADCraft keeps them for the next save.
    let again = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    assert_eq!(again.foreign.data_store_entities().len(), 2);
}

/// A record the data store binds to a layout (its thumbnail, say) stays: the layout is written under the handle the file gave it, which the
/// record names.
#[test]
fn a_record_bound_to_a_layout_is_kept() {
    let objects = super::foreign_passthrough::styled_objects();
    let source = objects.replace(
        "0\nEOF\n",
        &format!(
            "0\nSECTION\n2\nACDSDATA\n70\n2\n71\n1\n0\nACDSSCHEMA\n90\n1\n1\nAcDbThumbnail\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nThumb\n280\n15\n91\n0\n\
             0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n31\n2\nThumb\n280\n15\n94\n{}\n310\n{SAB_HEX}\n0\nENDSEC\n0\nEOF\n",
            SAB_HEX.len() / 2
        ),
    );
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let out = String::from_utf8(cadcraft_io::write_with(&d, "x.dxf", &options).unwrap()).unwrap();
    let tags = cadcraft_dxf::parse(out.as_bytes()).unwrap();
    let bound: Vec<String> = tags.iter().skip_while(|t| !(t.code == 2 && t.str() == "ACDSDATA")).filter(|t| t.code == 320).map(|t| t.str()).collect();
    assert_eq!(bound, ["31"], "{out}");
}

/// A real body (a unit cube) as SAB, in hex: what a file of R2013 or later keeps in its data store.
fn cube_sab_hex() -> String {
    let vertices =
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]];
    let faces = vec![vec![0, 3, 2, 1], vec![4, 5, 6, 7], vec![0, 1, 5, 4], vec![1, 2, 6, 5], vec![2, 3, 7, 6], vec![3, 0, 4, 7]];
    let doc = acadrust::entities::acis::primitives::build_planar_body(&vertices, &faces).expect("cube builds");
    acadrust::entities::acis::SabWriter::write(&doc).iter().map(|b| format!("{b:02X}")).collect()
}

/// Before R2013 a solid holds its ACIS data itself, as SAT text (group 70 = 1, then groups 1 and 3); AutoCAD LT 2025 discards a
/// DXF of those releases whose solid has the newer groups 290 and 2 and no data (Eztec Firenze as an R2000 DXF). The data store's
/// data is written into the solid for a DXF and for a DWG of those releases.
#[test]
fn a_file_before_2013_holds_the_data_store_data_in_the_solid() {
    let sab = cube_sab_hex();
    let source = dxf().replace(SAB_HEX, &sab).replace(&format!("94\n{}\n", SAB_HEX.len() / 2), &format!("94\n{}\n", sab.len() / 2));
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for version in [cadcraft_io::Version::R2000, cadcraft_io::Version::R2010] {
        let options = cadcraft_io::SaveOptions { version: Some(version), binary: false };
        let (bytes, notes) = cadcraft_io::write_with_noted(&d, "x.dxf", &options).unwrap();
        assert!(notes.iter().all(|n| !n.contains("ACIS")), "{version:?}: {notes:?}");
        let tags = cadcraft_dxf::parse(&bytes).unwrap();
        let solids: Vec<Vec<cadcraft_dxf::Tag>> = cadcraft_dxf::records(&tags).into_iter().filter(|(k, _)| k == "3DSOLID").map(|(_, t)| t).collect();
        assert_eq!(solids.len(), 2, "{version:?}");
        for solid in &solids {
            assert!(solid.iter().any(|t| t.code == 70 && t.i64() == 1), "{version:?}: {solid:?}");
            assert!(!solid.iter().any(|t| matches!(t.code, 290 | 2)), "{version:?}: the R2013 groups stay out");
            let text: Vec<String> = solid.iter().filter(|t| t.code == 1).map(|t| acadrust::entities::AcisData::decode_sat(&t.str())).collect();
            assert!(text.iter().any(|line| line.contains("body ")), "{version:?}: {text:?}");
            assert_eq!(text.last().map(String::as_str), Some("End-of-ACIS-data"), "{version:?}");
        }
        assert!(!String::from_utf8_lossy(&bytes).contains("ACDSDATA"), "{version:?}");
    }
    // The save reports no loss for them (the engine's not-saved list reads the same rule).
    let both: std::collections::BTreeSet<_> = d.model.iter().map(|e| e.handle).collect();
    assert_eq!(cadcraft_io::modeler_geometry_kept(&d, cadcraft_io::Version::R2010), both);
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: false };
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &options).unwrap();
    let stored = acadrust::DwgReader::from_stream(std::io::Cursor::new(dwg)).read().unwrap();
    let solids: Vec<_> = stored.entities().filter_map(|e| if let acadrust::EntityType::Solid3D(s) = e { Some(s) } else { None }).collect();
    assert_eq!(solids.len(), 2);
    assert!(solids.iter().all(|s| s.acis_data.has_data()), "an R2010 DWG keeps the solids' geometry");
}

/// Data that does not read as ACIS cannot become SAT text: the solid is written without it, and the save says so.
#[test]
fn data_that_does_not_convert_is_named_in_a_save_note() {
    let mut d = cadcraft_io::read_dxf(dxf().as_bytes()).unwrap();
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2000), binary: false };
    let (_, notes) = cadcraft_io::write_with_noted(&d, "x.dxf", &options).unwrap();
    assert_eq!(notes.iter().filter(|n| n.contains("ACIS data could not be converted")).count(), 2, "{notes:?}");
    // A deleted solid is not written, so no note names it (its data store record stays until the save leaves it out).
    let h = d.model.iter().find(|e| e.handle.hex() == "2B").map(|e| e.handle).unwrap();
    d.remove_entity(h).unwrap();
    let (_, notes) = cadcraft_io::write_with_noted(&d, "x.dxf", &options).unwrap();
    let named: Vec<&String> = notes.iter().filter(|n| n.contains("ACIS data could not be converted")).collect();
    assert_eq!(named.len(), 1, "{notes:?}");
    assert!(named[0].contains("Entity 2A"), "{named:?}");
}

/// A binary DXF writes the SAT cipher without the space an ASCII DXF puts after each ciphered `A` (a caret there escapes the next
/// character): the binary reader takes every byte as it is.
#[test]
fn a_binary_dxf_before_2013_holds_the_binary_cipher_form() {
    let sab = cube_sab_hex();
    let source = dxf().replace(SAB_HEX, &sab).replace(&format!("94\n{}\n", SAB_HEX.len() / 2), &format!("94\n{}\n", sab.len() / 2));
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: true };
    let bytes = cadcraft_io::write_with(&d, "x.dxf", &options).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let solid = cadcraft_dxf::records(&tags).into_iter().find(|(k, _)| k == "3DSOLID").map(|(_, t)| t).unwrap();
    let ascii =
        cadcraft_io::write_with(&d, "x.dxf", &cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: false }).unwrap();
    let ascii_solid = cadcraft_dxf::records(&cadcraft_dxf::parse(&ascii).unwrap()).into_iter().find(|(k, _)| k == "3DSOLID").map(|(_, t)| t).unwrap();
    let last = |t: &[cadcraft_dxf::Tag]| t.iter().rfind(|t| t.code == 1).map(|t| t.str()).unwrap();
    // The terminator End-of-ACIS-data has one A: ciphered `^` with the protective space in ASCII, without it in binary.
    assert_eq!(last(&ascii_solid).matches("^ ").count(), 1);
    assert_eq!(last(&solid).matches("^ ").count(), 0);
    assert_eq!(last(&solid).len() + 1, last(&ascii_solid).len());
}

/// A record that holds a placeholder (group 70 with an empty group 1, what the release reform of a surface writes without the data
/// store) gets the data store's SAT text; one with SAT text of its own keeps it.
#[test]
fn a_placeholder_gets_the_data_store_data_and_own_sat_text_stays() {
    let sab = cube_sab_hex();
    let solid = |h: &str, sat: &str| format!("0\nREGION\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n70\n1\n1\n{sat}\n");
    let record =
        |h: &str| format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{sab}\n", sab.len() / 2);
    let source = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1024\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n2\n71\n2\n0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n{}{}0\nENDSEC\n0\nEOF\n",
        solid("2A", ""),
        solid("2B", "own text"),
        record("2A"),
        record("2B")
    );
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2010), binary: false };
    let tags = cadcraft_dxf::parse(&cadcraft_io::write_with(&d, "x.dxf", &options).unwrap()).unwrap();
    let lines = |handle: &str| -> Vec<String> {
        cadcraft_dxf::records(&tags)
            .into_iter()
            .find(|(k, t)| k == "REGION" && t.iter().any(|t| t.code == 5 && t.str() == handle))
            .map(|(_, t)| t.iter().filter(|t| t.code == 1).map(|t| t.str()).collect())
            .unwrap()
    };
    let decoded: Vec<String> = lines("2A").iter().map(|l| acadrust::entities::AcisData::decode_sat(l)).collect();
    assert_eq!(decoded.last().map(String::as_str), Some("End-of-ACIS-data"), "{decoded:?}");
    assert_eq!(lines("2B"), ["own text"]);
}

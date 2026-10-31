//! Group 298 of an MLEADERSTYLE (a flag AutoCAD 2013 and later write, undocumented) is kept as the file gave it, in a DXF and in a DWG.

use cadcraft_io::{SaveOptions, Version};

fn style_298(text: &str, name: &str) -> Option<String> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap();
    let style =
        cadcraft_dxf::records(&objects.tags).into_iter().find(|(k, t)| k == "MLEADERSTYLE" && t.iter().any(|g| g.code == 3 && g.str() == name))?.1;
    style.iter().find(|g| g.code == 298).map(|g| g.str())
}

#[test]
fn group_298_of_an_mleaderstyle_is_kept_in_a_dxf_and_a_dwg() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let mut style = d.mleader_styles.first().cloned().unwrap_or_default();
    style.name = "Flagged".into();
    style.description = "Flagged".into();
    style.unknown_298 = true;
    d.mleader_styles.push(style);
    let dxf = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let text = String::from_utf8(dxf).unwrap();
    assert_eq!(style_298(&text, "Flagged").as_deref(), Some("1"), "the DXF");
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert!(back.mleader_styles.iter().any(|s| s.name == "Flagged" && s.unknown_298), "read back");
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let again = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
    let again = String::from_utf8(again).unwrap();
    assert_eq!(style_298(&again, "Flagged").as_deref(), Some("1"), "the DWG");
}

/// AutoCAD writes group 298 as the last group of an MLEADERSTYLE (after the attachment directions 271, 272, 273), in a DXF and in a DXF
/// made from a DWG.
#[test]
fn group_298_is_the_last_group_of_an_mleaderstyle() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let mut style = d.mleader_styles.first().cloned().unwrap_or_default();
    style.name = "Flagged".into();
    style.description = "Flagged".into();
    style.unknown_298 = true;
    d.mleader_styles.push(style);
    let dxf = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let text = String::from_utf8(dxf).unwrap();
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap();
    let style = cadcraft_dxf::records(&objects.tags)
        .into_iter()
        .find(|(k, t)| k == "MLEADERSTYLE" && t.iter().any(|g| g.code == 3 && g.str() == "Flagged"))
        .unwrap()
        .1;
    let codes: Vec<i32> = style.iter().map(|g| g.code).collect();
    assert_eq!(&codes[codes.len() - 4..], [271, 272, 273, 298], "{codes:?}");
    assert_eq!(codes.iter().filter(|c| **c == 298).count(), 1);
}

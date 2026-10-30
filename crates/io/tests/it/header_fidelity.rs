//! Read header variables keep their DXF group codes without duplicating generated values.

use cadcraft_doc::{Common, EntityKind, HVal, Line, Space};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn source() -> &'static [u8] {
    b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$USERI1\n70\n1000\n9\n$USERR5\n40\n2.5\n9\n$ISOLINES\n70\n8\n9\n$LATITUDE\n40\n0\n9\n$PLIMMIN\n10\n-5\n20\n-10\n9\n$PEXTMAX\n10\n10\n20\n20\n30\n30\n9\n$DIMUPT\n70\n1\n9\n$PROJECTNAME\n1\nSynthetic\n9\n$CAMERADISPLAY\n290\n1\n9\n$CMATERIAL\n347\n123\n9\n$EXTMIN\n10\n-99\n20\n-99\n30\n0\n9\n$HANDSEED\n5\n18\n0\nENDSEC\n0\nEOF\n"
}

#[test]
fn header_fidelity_read_variables_keep_codes_and_modified_values() {
    let mut d = cadcraft_io::read_dxf(source()).unwrap();
    d.header.set_i64("USERI1", 2000);
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })).unwrap();
    let opts = SaveOptions { version: Some(Version::R2018), binary: false };
    let bytes = cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    for name in ["USERI1", "USERR5", "LATITUDE", "PLIMMIN", "PEXTMAX", "DIMUPT", "PROJECTNAME", "CAMERADISPLAY", "EXTMIN", "HANDSEED", "ACADVER"] {
        assert_eq!(tags.iter().filter(|t| t.code == 9 && t.str() == format!("${name}")).count(), 1, "{name} once");
    }
    let group_after = |name: &str| tags.windows(2).find(|p| p[0].code == 9 && p[0].str() == format!("${name}")).unwrap()[1].clone();
    assert_eq!(group_after("USERI1"), cadcraft_dxf::Tag::i(70, 2000));
    assert_eq!((group_after("CAMERADISPLAY").code, group_after("CAMERADISPLAY").i64()), (290, 1));
    assert!(!tags.iter().any(|t| t.code == 9 && t.str() == "$ISOLINES"), "private bridge header");
    let back = cadcraft_io::read_dxf(&bytes).unwrap();
    for name in ["USERI1", "USERR5", "LATITUDE", "PLIMMIN", "PEXTMAX", "DIMUPT", "PROJECTNAME", "CAMERADISPLAY"] {
        assert_eq!(back.header.get(name), d.header.get(name), "{name}");
    }
    // The writer keeps the saved extents as read: it runs no geometry query.
    assert_eq!(back.header.get("EXTMIN"), Some(&HVal::Point(Vec3::new(-99.0, -99.0, 0.0))), "the source EXTMIN is kept");
    // Its handle 123 names no object here: the pointer is never replayed, only remapped.
    let cmaterial = tags.iter().position(|t| t.code == 9 && t.str() == "$CMATERIAL").and_then(|i| tags.get(i + 1)).map(|t| t.str());
    assert!(cmaterial.as_deref().is_none_or(|h| h == "0"), "stale source handles must not be replayed: {cmaterial:?}");
}

#[test]
fn header_fidelity_downgrade_omits_variables_newer_than_the_target() {
    let d = cadcraft_io::read_dxf(source()).unwrap();
    let bytes = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2000), binary: false }).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let has = |name: &str| tags.iter().any(|t| t.code == 9 && t.str() == format!("${name}"));
    assert!(has("USERI1"), "old variables survive a downgrade");
    assert!(has("PLIMMIN"));
    assert!(!has("CAMERADISPLAY"));
    assert!(!has("LATITUDE"));
    assert!(!has("DIMJOGANG"));
    assert!(!has("DIMMZF"));
}

#[test]
fn header_fidelity_read_variables_survive_dwg() {
    let d = cadcraft_io::read_dxf(source()).unwrap();
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    for name in ["USERI1", "USERR5", "ISOLINES", "LATITUDE", "PLIMMIN", "PEXTMAX", "DIMUPT", "PROJECTNAME", "CAMERADISPLAY"] {
        assert_eq!(back.header.get(name), d.header.get(name), "{name}");
    }
}

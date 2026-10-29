//! The plot style of a POLYLINE record (a heavy 2D polyline, a 3D polyline, a polygon mesh) is read
//! from the record itself, not only from its VERTEX and SEQEND children: a named style (390), ByBlock
//! and ByDictionaryDefault (380) survive a save as user DXF and as DWG, and a second save.

use cadcraft_doc::{Drawing, EntityKind};

/// The plot style groups of the AcDbEntity subclass and the plot style they give, for each mode.
const MODES: [(&str, &str); 3] = [("380\n3\n390\nF1\n", "Thick"), ("380\n1\n", "ByBlock"), ("380\n2\n", "ByDictionaryDefault")];

fn vertex(sub: &str, flags: i32, p: (i32, i32)) -> String {
    format!("0\nVERTEX\n8\n0\n100\nAcDbVertex\n100\n{sub}\n10\n{}\n20\n{}\n30\n0\n70\n{flags}\n", p.0, p.1)
}

/// Nine POLYLINE records, three kinds in each of the three modes, with a named plot style `Thick`.
fn fixture() -> String {
    let mut text = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let mut handle = 0x100;
    for (mode, _) in MODES {
        let polyline = |handle: i32, sub: &str, flags: i32, extra: &str| {
            format!("0\nPOLYLINE\n5\n{handle:X}\n100\nAcDbEntity\n8\n0\n{mode}100\n{sub}\n66\n1\n10\n0\n20\n0\n30\n0\n70\n{flags}\n{extra}")
        };
        let points = [(0, 0), (4, 0), (4, 3), (0, 3)];
        // A 2D polyline, a 3D polyline and a polygon mesh of 2 x 2 vertices.
        text += &polyline(handle, "AcDb2dPolyline", 0, "");
        points.iter().take(3).for_each(|p| text += &vertex("AcDb2dVertex", 0, *p));
        text += "0\nSEQEND\n8\n0\n";
        text += &polyline(handle + 1, "AcDb3dPolyline", 8, "");
        points.iter().take(3).for_each(|p| text += &vertex("AcDb3dPolylineVertex", 32, *p));
        text += "0\nSEQEND\n8\n0\n";
        text += &polyline(handle + 2, "AcDbPolygonMesh", 16, "71\n2\n72\n2\n");
        points.iter().for_each(|p| text += &vertex("AcDbPolygonMeshVertex", 64, *p));
        text += "0\nSEQEND\n8\n0\n";
        handle += 0x10;
    }
    text += "0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nACAD_PLOTSTYLENAME\n350\nF0\n";
    text += "0\nACDBDICTIONARYWDFLT\n5\nF0\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nNormal\n350\nF2\n3\nThick\n350\nF1\n100\nAcDbDictionaryWithDefault\n340\nF2\n";
    text + "0\nACDBPLACEHOLDER\n5\nF1\n330\nF0\n0\nACDBPLACEHOLDER\n5\nF2\n330\nF0\n0\nENDSEC\n0\nEOF\n"
}

fn assert_styles(d: &Drawing, what: &str) {
    let seen: Vec<(&str, String)> =
        d.model.iter().filter(|e| !matches!(e.kind, EntityKind::Unknown(_))).map(|e| (e.kind.type_name(), e.common.plot_style.clone())).collect();
    assert_eq!(seen.len(), 9, "{what}: {seen:?}");
    for (i, (kind, style)) in seen.iter().enumerate() {
        assert_eq!(style, MODES[i / 3].1, "{what}: the parent polyline {i} ({kind})");
    }
}

#[test]
fn the_parent_polyline_keeps_its_plot_style_through_every_save() {
    let d = cadcraft_io::read_dxf(fixture().as_bytes()).unwrap();
    assert_styles(&d, "read");
    let text = cadcraft_io::write_dxf(&d);
    let again = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_styles(&again, "DXF");
    assert_styles(&cadcraft_io::read_dxf(cadcraft_io::write_dxf(&again).as_bytes()).unwrap(), "DXF, second save");
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    assert_styles(&back, "DWG");
    assert_styles(&cadcraft_io::read(&cadcraft_io::write(&back, "x.dwg").unwrap(), "x.dwg").unwrap(), "DWG, second save");
}

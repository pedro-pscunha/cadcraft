//! A named shape STYLE keeps its shape flag and its independent last-used height.
use cadcraft_dxf::Tag;

const SOURCE: &str = include_str!("../data/named_shape_style.dxf");

fn style(bytes: &[u8]) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let styles: Vec<_> = cadcraft_dxf::sections(&tags)
        .into_iter()
        .filter(|s| s.name == "TABLES")
        .flat_map(|s| cadcraft_dxf::records(&s.tags))
        .filter(|(k, _)| k == "STYLE")
        .collect();
    styles
        .iter()
        .find(|(_, t)| t.iter().any(|t| t.code == 3 && t.str().eq_ignore_ascii_case("source_shapes.shx")))
        .unwrap_or_else(|| {
            panic!("missing named shape STYLE: {:?}", styles.iter().map(|(_, t)| t.iter().find(|t| t.code == 2).map(Tag::str)).collect::<Vec<_>>())
        })
        .1
        .clone()
}

fn assert_style(bytes: &[u8], vertical: bool) {
    let tags = style(bytes);
    let flags = tags.iter().find(|t| t.code == 70).unwrap().i64();
    assert_eq!(flags & 5, 1 | if vertical { 4 } else { 0 });
    assert_eq!(tags.iter().find(|t| t.code == 42).unwrap().f64(), 12.75);
}

#[test]
fn named_shape_and_last_height_survive_two_saves() {
    for format in ["saved.dxf", "saved.dwg"] {
        let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
        for _ in 0..2 {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            assert_style(&dxf, true);
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

#[test]
fn shape_name_survives_user_dxf() {
    let drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let saved = cadcraft_io::write(&drawing, "saved.dxf").unwrap();
    assert_eq!(style(&saved).iter().find(|t| t.code == 2).unwrap().str(), "SourceShapes");
}

#[test]
fn shape_name_survives_dwg_dxf_emission() {
    let drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let saved = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
    let dxf = cadcraft_dwg::dwg_to_dxf(&saved).unwrap();
    assert_eq!(style(&dxf).iter().find(|t| t.code == 2).unwrap().str(), "SourceShapes");
}

#[test]
fn source_shape_slot_is_reused_instead_of_growing_each_save() {
    let source=SOURCE.replace("2\nSourceShapes\n", "2\n\n").replace("0\nENDTAB\n0\nENDSEC\n", "0\nENDTAB\n0\nTABLE\n2\nLTYPE\n70\n1\n0\nLTYPE\n5\n65\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nShapeDash\n70\n0\n3\nsource\n72\n65\n73\n1\n40\n1\n49\n1\n74\n4\n75\n1\n340\n60\n46\n1\n50\n0\n44\n0\n45\n0\n0\nENDTAB\n0\nENDSEC\n");
    for format in ["saved.dxf", "saved.dwg"] {
        let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        for _ in 0..3 {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            assert_style(&dxf, true);
            let tags = cadcraft_dxf::parse(&dxf).unwrap();
            let records: Vec<_> =
                cadcraft_dxf::sections(&tags).into_iter().filter(|s| s.name == "TABLES").flat_map(|s| cadcraft_dxf::records(&s.tags)).collect();
            let shapes: Vec<_> = records.iter().filter(|(k, t)| k == "STYLE" && t.iter().any(|t| t.code == 70 && t.i64() & 1 != 0)).collect();
            assert_eq!(shapes.len(), 1, "{format}: one source shape slot");
            let handle = shapes[0].1.iter().find(|t| t.code == 5).unwrap().str();
            let ltype = &records.iter().find(|(k, t)| k == "LTYPE" && t.iter().any(|t| t.code == 2 && t.str() == "ShapeDash")).unwrap().1;
            assert_eq!(ltype.iter().find(|t| t.code == 340).unwrap().str(), handle);
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

#[test]
fn a_vertical_edit_changes_only_the_owned_style_flag() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    drawing.text_styles.iter_mut().find(|s| s.name == "SourceShapes").unwrap().vertical = false;
    assert_style(&cadcraft_io::write(&drawing, "edited.dxf").unwrap(), false);
}

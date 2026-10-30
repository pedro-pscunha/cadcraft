//! Source ACI fallbacks are independent of their RGB colors.
use cadcraft_doc::{
    EntityKind,
    color::{Color, Rgb},
};
use cadcraft_dxf::Tag;

const SOURCE: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n1\n0\nLAYER\n5\n50\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\nPaint\n70\n0\n62\n-6\n420\n1193046\n6\nContinuous\n0\nENDTAB\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n100\n100\nAcDbEntity\n8\nPaint\n62\n7\n420\n1122867\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n10\n21\n10\n31\n0\n\
0\nHATCH\n5\n101\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n4\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n10\n0\n20\n10\n97\n0\n75\n0\n76\n1\n98\n0\n450\n1\n451\n0\n460\n0\n461\n0\n452\n0\n462\n0\n453\n2\n463\n0\n63\n3\n421\n16711680\n463\n1\n63\n2\n421\n255\n470\nLINEAR\n0\nENDSEC\n0\nEOF\n";

fn records(bytes: &[u8]) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    cadcraft_dxf::sections(&tags).into_iter().flat_map(|s| cadcraft_dxf::records(&s.tags)).collect()
}
fn value(tags: &[Tag], code: i32) -> i64 {
    tags.iter().find(|t| t.code == code).unwrap().i64()
}
fn assert_fallbacks(bytes: &[u8], layer_aci: i64) {
    let records = records(bytes);
    let line = &records.iter().find(|(k, _)| k == "LINE").unwrap().1;
    let layer = &records.iter().find(|(k, t)| k == "LAYER" && t.iter().any(|t| t.code == 2 && t.str() == "Paint")).unwrap().1;
    let hatch = &records.iter().find(|(k, _)| k == "HATCH").unwrap().1;
    assert_eq!((value(line, 62), value(line, 420)), (7, 1122867));
    assert_eq!((value(layer, 62), value(layer, 420)), (layer_aci, 1193046));
    assert_eq!(hatch.iter().filter(|t| t.code == 63).map(Tag::i64).collect::<Vec<_>>(), [3, 2]);
    assert_eq!(hatch.iter().filter(|t| t.code == 421).map(Tag::i64).collect::<Vec<_>>(), [16711680, 255]);
}
#[test]
fn rgb_and_aci_pairs_survive_two_dxf_saves() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dxf").unwrap();
        assert_fallbacks(&saved, -6);
        drawing = cadcraft_io::read(&saved, "saved.dxf").unwrap();
    }
}

#[test]
fn dwg_cmc_fallbacks_follow_the_native_nearest_color_rule() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&saved).unwrap();
        let records = records(&dxf);
        let layer = &records.iter().find(|(k, t)| k == "LAYER" && t.iter().any(|t| t.code == 2 && t.str() == "Paint")).unwrap().1;
        let hatch = &records.iter().find(|(k, _)| k == "HATCH").unwrap().1;
        // ODA 2.11 CMC stores index zero; exported indexed fallbacks derive from RGB.
        assert_eq!((value(layer, 62), value(layer, 420)), (-i64::from(cadcraft_doc::color::nearest_aci(Rgb(0x12, 0x34, 0x56))), 1193046));
        assert_eq!(hatch.iter().filter(|t| t.code == 63).map(Tag::i64).collect::<Vec<_>>(), [1, 5]);
        assert_eq!(hatch.iter().filter(|t| t.code == 421).map(Tag::i64).collect::<Vec<_>>(), [16711680, 255]);
        drawing = cadcraft_io::read(&saved, "saved.dwg").unwrap();
    }
}

#[test]
#[ignore = "pending acadrust: ACI fallback with true colour in DWG"]
fn dwg_entity_enc_keeps_the_source_index_next_to_rgb() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    for _ in 0..2 {
        let saved = cadcraft_io::write(&drawing, "saved.dwg").unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&saved).unwrap();
        let records = records(&dxf);
        let line = &records.iter().find(|(k, _)| k == "LINE").unwrap().1;
        assert_eq!((value(line, 62), value(line, 420)), (7, 1122867));
        drawing = cadcraft_io::read(&saved, "saved.dwg").unwrap();
    }
}
#[test]
fn layer_visibility_edits_keep_the_source_index_magnitude() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    drawing.layers.iter_mut().find(|l| l.name == "Paint").unwrap().on = true;
    assert_fallbacks(&cadcraft_io::write(&drawing, "edited.dxf").unwrap(), 6);
}
#[test]
fn rgb_edits_replace_only_the_matching_aci_fallback() {
    let mut drawing = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let rgb = Rgb(80, 90, 100);
    let line = drawing.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
    drawing.modify_entity(line, |e| e.common.color = Color::True(rgb)).unwrap();
    let hatch = drawing.model.iter().find(|e| matches!(e.kind, EntityKind::Hatch(_))).unwrap().handle;
    drawing
        .modify_entity(hatch, |e| {
            let EntityKind::Hatch(h) = &mut e.kind else { panic!("hatch") };
            h.gradient.as_mut().unwrap().color1 = Color::True(rgb);
        })
        .unwrap();
    let saved = cadcraft_io::write(&drawing, "edited.dxf").unwrap();
    let records = records(&saved);
    let line = &records.iter().find(|(k, _)| k == "LINE").unwrap().1;
    let hatch = &records.iter().find(|(k, _)| k == "HATCH").unwrap().1;
    assert_eq!((value(line, 62), value(line, 420)), (i64::from(cadcraft_doc::color::nearest_aci(rgb)), i64::from(rgb.to_u32())));
    assert_eq!(hatch.iter().filter(|t| t.code == 63).map(Tag::i64).collect::<Vec<_>>(), [i64::from(cadcraft_doc::color::nearest_aci(rgb)), 2]);
}

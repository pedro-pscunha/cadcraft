//! A TEXT read by the legacy reader keeps its generation flags (group 71: 2 backward, 4 upside down) and is drawn mirrored
//! like AutoCAD draws it: the entity's flags join the text style's, so the display equals that of the same text with the
//! flags in its style.

use cadcraft_doc::{EntityKind, Space};
use cadcraft_render::{Options, build};

fn text_dxf(flags: Option<i64>) -> String {
    let flag = flags.map_or(String::new(), |f| format!("71\n{f}\n"));
    format!(
        "0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n5\n20\n3\n30\n0\n40\n2\n1\nHELLO\n50\n20\n{flag}7\nStandard\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn a_legacy_text_keeps_its_generation_flags_and_is_drawn_mirrored() {
    let plain = cadcraft_io::read_dxf(text_dxf(None).as_bytes()).unwrap();
    let shown = |d: &cadcraft_doc::Drawing| format!("{:?}", build(d, &Space::Model, &Options::default()));
    assert!(shown(&plain).len() > 200, "the text is drawn");
    for flags in [2, 4, 6] {
        let flagged = cadcraft_io::read_dxf(text_dxf(Some(flags)).as_bytes()).unwrap();
        let text =
            |d: &cadcraft_doc::Drawing| d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
        let (a, b) = (text(&plain), text(&flagged));
        assert_eq!((b.backward, b.upside_down), (flags & 2 != 0, flags & 4 != 0), "flags {flags}: group 71 is read");
        assert_eq!(cadcraft_doc::Text { backward: false, upside_down: false, ..b }, a, "flags {flags}: nothing else differs");
        // The same text with the flags in its style (and none on the entity) is the mirrored display.
        let mut styled = cadcraft_io::read_dxf(text_dxf(None).as_bytes()).unwrap();
        styled.text_styles.iter_mut().for_each(|s| (s.backwards, s.upside_down) = (flags & 2 != 0, flags & 4 != 0));
        assert_ne!(shown(&plain), shown(&styled), "flags {flags}: the mirror changes the drawing");
        assert_eq!(shown(&flagged), shown(&styled), "flags {flags}: the entity flags mirror like the style flags");
    }
}

fn attrib_dxf(flags: i64) -> String {
    format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nB\n70\n0\n10\n0\n20\n0\n30\n0\n3\nB\n1\n\n\
         0\nATTDEF\n5\n20\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n0\n20\n0\n30\n0\n40\n1\n1\nDEF\n71\n{flags}\n7\nStandard\n100\nAcDbAttributeDefinition\n3\nPrompt\n2\nTAG\n70\n0\n\
         0\nENDBLK\n8\n0\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n\
         0\nINSERT\n5\n50\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockReference\n66\n1\n2\nB\n10\n10\n20\n0\n30\n0\n\
         0\nATTRIB\n5\n60\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n10\n20\n0\n30\n0\n40\n1\n1\nVAL\n71\n{flags}\n7\nStandard\n100\nAcDbAttribute\n2\nTAG\n70\n0\n\
         0\nSEQEND\n5\n70\n100\nAcDbEntity\n8\n0\n0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn a_legacy_attribute_is_drawn_mirrored_too() {
    let shown = |d: &cadcraft_doc::Drawing| format!("{:?}", build(d, &Space::Model, &Options::default()));
    let plain = cadcraft_io::read_dxf(attrib_dxf(0).as_bytes()).unwrap();
    for flags in [2, 4, 6] {
        let flagged = cadcraft_io::read_dxf(attrib_dxf(flags).as_bytes()).unwrap();
        let mut styled = cadcraft_io::read_dxf(attrib_dxf(0).as_bytes()).unwrap();
        styled.text_styles.iter_mut().for_each(|s| (s.backwards, s.upside_down) = (flags & 2 != 0, flags & 4 != 0));
        assert_ne!(shown(&plain), shown(&styled), "flags {flags}: the mirror changes the drawing");
        assert_eq!(shown(&flagged), shown(&styled), "flags {flags}: the attribute is mirrored like its style would");
    }
}

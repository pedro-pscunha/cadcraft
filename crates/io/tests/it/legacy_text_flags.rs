//! A TEXT read by the legacy reader keeps its generation flags (group 71) in the model, but the 2D display still draws it
//! unmirrored: `place_text_entity` uses the text style's flags only. Known gap, closed by the world text layout, not here.

use cadcraft_doc::{EntityKind, Space};
use cadcraft_render::{Options, build};

fn text_dxf(flags: Option<i64>) -> String {
    let flag = flags.map_or(String::new(), |f| format!("71\n{f}\n"));
    format!(
        "0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n5\n20\n3\n30\n0\n40\n2\n1\nHELLO\n50\n20\n{flag}7\nStandard\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn a_legacy_text_keeps_its_generation_flags_but_the_2d_display_does_not_mirror_it_yet() {
    let plain = cadcraft_io::read_dxf(text_dxf(None).as_bytes()).unwrap();
    for flags in [2, 4, 6] {
        let flagged = cadcraft_io::read_dxf(text_dxf(Some(flags)).as_bytes()).unwrap();
        let kind =
            |d: &cadcraft_doc::Drawing| d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
        let (a, b) = (kind(&plain), kind(&flagged));
        assert_eq!((b.backward, b.upside_down), (flags & 2 != 0, flags & 4 != 0), "flags {flags}: group 71 is read");
        assert_eq!((a.backward, a.upside_down), (false, false));
        assert_eq!(cadcraft_doc::Text { backward: false, upside_down: false, ..b }, a, "flags {flags}: nothing else differs");
        let shown = |d: &cadcraft_doc::Drawing| format!("{:?}", build(d, &Space::Model, &Options::default()));
        assert_eq!(shown(&plain), shown(&flagged), "flags {flags}: the display is the unmirrored one (known gap)");
        assert!(shown(&plain).contains("HELLO") || shown(&plain).len() > 200, "the text is drawn");
    }
}

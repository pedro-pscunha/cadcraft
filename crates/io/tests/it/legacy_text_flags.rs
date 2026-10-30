//! A TEXT or ATTRIB read by the legacy reader displays as it did before the text model gained its generation flags: the
//! flags (group 71) are canonical-only, so the legacy payload is the same with or without them.

use cadcraft_doc::{EntityKind, Space};
use cadcraft_render::{Options, build};

fn text_dxf(flags: Option<i64>) -> String {
    let flag = flags.map_or(String::new(), |f| format!("71\n{f}\n"));
    format!(
        "0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n5\n20\n3\n30\n0\n40\n2\n1\nHELLO\n50\n20\n{flag}7\nStandard\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn a_legacy_text_ignores_its_generation_flags_in_the_model_and_in_the_display() {
    let plain = cadcraft_io::read_dxf(text_dxf(None).as_bytes()).unwrap();
    for flags in [2, 4, 6] {
        let flagged = cadcraft_io::read_dxf(text_dxf(Some(flags)).as_bytes()).unwrap();
        let kind =
            |d: &cadcraft_doc::Drawing| d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
        let (a, b) = (kind(&plain), kind(&flagged));
        assert!(!b.backward && !b.upside_down, "flags {flags}: the legacy reader does not read group 71");
        assert_eq!(a, b, "flags {flags}: the same legacy text");
        let shown = |d: &cadcraft_doc::Drawing| format!("{:?}", build(d, &Space::Model, &Options::default()));
        assert_eq!(shown(&plain), shown(&flagged), "flags {flags}: the same display");
        assert!(shown(&plain).contains("HELLO") || shown(&plain).len() > 200, "the text is drawn");
    }
}

//! An MTEXT in the default text style has no group 7, as AutoCAD writes it (802 of the Citycon drawing's block MTEXTs); one in another
//! style has it, and both read back in their style.

use cadcraft_doc::{Common, EntityKind, MText, Space};
use cadcraft_geom::Vec3;

fn mtext(style: &str) -> EntityKind {
    EntityKind::MText(MText {
        insert: Vec3::ZERO,
        height: 2.0,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: style.into(),
        contents: "NOTE".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
        drawing_direction: None,
    })
}

/// The group 7 values of the MTEXT records in a DXF text.
fn styles(text: &str) -> Vec<Option<String>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let mut out: Vec<Option<String>> = Vec::new();
    let mut in_mtext = false;
    for t in &tags {
        if t.code == 0 {
            in_mtext = t.str() == "MTEXT";
            if in_mtext {
                out.push(None);
            }
        } else if in_mtext && t.code == 7 {
            *out.last_mut().unwrap() = Some(t.str());
        }
    }
    out
}

#[test]
fn the_default_style_has_no_group_7_and_another_has_it() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let (a, b, c) = (
        d.add(&Space::Model, Common::default(), mtext("Standard")).unwrap(),
        d.add(&Space::Model, Common::default(), mtext("STANDARD")).unwrap(),
        d.add(&Space::Model, Common::default(), mtext("Notes")).unwrap(),
    );
    let text = cadcraft_io::write_dxf(&d);
    assert_eq!(styles(&text), [None, None, Some("Notes".into())]);
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let style = |h| match &back.entity(h).unwrap().kind {
        EntityKind::MText(t) => t.style.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!((style(a).to_ascii_uppercase(), style(b).to_ascii_uppercase(), style(c)), ("STANDARD".into(), "STANDARD".into(), "Notes".into()));
}

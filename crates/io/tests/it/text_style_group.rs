//! A TEXT in the default text style has no group 7, as AutoCAD writes it; one in another style has it,
//! and both read back in their style.

use cadcraft_doc::{Common, EntityKind, HAlign, Space, Text, VAlign};
use cadcraft_geom::Vec3;

fn text(style: &str) -> EntityKind {
    EntityKind::Text(Text {
        insert: Vec3::ZERO,
        align_pt: None,
        height: 2.0,
        value: "NOTE".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: style.into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        backward: false,
        upside_down: false,
    })
}

/// The group 7 values of the TEXT records in a DXF text.
fn styles(text: &str) -> Vec<Option<String>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let mut out: Vec<Option<String>> = Vec::new();
    let mut in_text = false;
    for t in &tags {
        if t.code == 0 {
            in_text = t.str() == "TEXT";
            if in_text {
                out.push(None);
            }
        } else if in_text && t.code == 7 {
            *out.last_mut().unwrap() = Some(t.str());
        }
    }
    out
}

#[test]
fn the_default_style_has_no_group_7_and_another_has_it() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let handles: Vec<_> = ["Standard", "STANDARD", "Notes"].into_iter().map(|s| d.add(&Space::Model, Common::default(), text(s)).unwrap()).collect();
    let written = cadcraft_io::write_dxf(&d);
    assert_eq!(styles(&written), [None, None, Some("Notes".into())]);
    let back = cadcraft_io::read_dxf(written.as_bytes()).unwrap();
    let style = |h| match &back.entity(h).unwrap().kind {
        EntityKind::Text(t) => t.style.to_ascii_uppercase(),
        other => panic!("{other:?}"),
    };
    assert_eq!(handles.iter().map(|h| style(*h)).collect::<Vec<_>>(), ["STANDARD", "STANDARD", "NOTES"]);
}

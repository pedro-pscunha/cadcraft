//! Dictionary keys, not a possibly shortened MLINESTYLE group 2, identify styles.

use cadcraft_doc::{Common, Drawing, EntityKind, MLine, MLineJust, MLineStyle, Space};
use cadcraft_geom::Vec2;

#[test]
fn distinct_dictionary_entries_with_shortened_style_names_are_not_merged() {
    let mut d = Drawing::new_metric();
    for name in ["SpareA", "SpareB"] {
        d.mline_styles.push(MLineStyle { name: name.into(), description: name.into(), ..MLineStyle::default() });
    }
    for name in ["SpareA", "SpareB"] {
        let ml = MLine::new(d.mline_style(name).unwrap(), &[Vec2::ZERO, Vec2::new(3.0, 0.0)], false, MLineJust::Zero, 1.0);
        d.add(&Space::Model, Common::default(), EntityKind::MLine(ml)).unwrap();
    }
    let tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let mut kind = String::new();
    let source: String = tags
        .iter()
        .map(|t| {
            if t.code == 0 {
                kind = t.str();
            }
            let v = t.str();
            let v = if matches!(kind.as_str(), "MLINESTYLE" | "MLINE") && t.code == 2 && v.starts_with("Spare") { "Short" } else { &v };
            format!("{}\n{v}\n", t.code)
        })
        .collect();
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for _ in 0..2 {
        for name in ["SpareA", "SpareB"] {
            let st = d.mline_styles.iter().find(|s| s.name == name).expect("each dictionary key keeps its own style");
            assert_eq!(st.description, name);
        }
        assert_eq!(d.mline_styles.len(), 3);
        let mlines: Vec<_> = d
            .model
            .iter()
            .filter_map(|e| if let EntityKind::MLine(m) = &e.kind { Some((m.style.as_str(), m.def.description.as_str())) } else { None })
            .collect();
        assert_eq!(mlines, [("SpareA", "SpareA"), ("SpareB", "SpareB")]);
        let text = cadcraft_io::write_dxf(&d);
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.mline_styles, d.mline_styles);
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        d = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    }
}

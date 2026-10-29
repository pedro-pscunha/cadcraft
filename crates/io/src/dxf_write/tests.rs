use super::*;
use std::cell::Cell;
std::thread_local! { pub(super) static SCANNED: Cell<usize> = const { Cell::new(0) }; }

#[test]
fn dimstyle_fidelity_bridge_private_variables_are_plain_groups_only_for_supported_dwg_targets() {
    let mut d = Drawing::new_imperial();
    let fields = serde_json::json!({"DIMMZF": 12.5, "DIMALTMZF": 25.0, "DIMMZS": "m", "DIMALTMZS": "cm"});
    assert!(d.dim_styles[0].apply_fields(fields.as_object().unwrap()).is_empty());
    for target in [Version::R2000, Version::R2007, Version::R2010, Version::R2018] {
        let tags = tags(&d, target);
        // The public tag writer is not the bridge, even when its version is 2018.
        let bytes = dwg_bridge(&d, target);
        let bridge = cadcraft_dxf::parse(&bytes).unwrap();
        let has_private = |tags: &[Tag]| tags.iter().any(|t| t.code == 51 && t.f64() == 12.5);
        assert!(!has_private(&tags), "public DXF {target:?}");
        assert_eq!(has_private(&bridge), target >= Version::R2010, "DWG {target:?}");
        if target >= Version::R2010 {
            assert!(bridge.iter().any(|t| t.code == 52 && t.f64() == 25.0));
            assert!(bridge.iter().any(|t| t.code == 300 && t.str() == "m"));
            assert!(bridge.iter().any(|t| t.code == 301 && t.str() == "cm"));
        }
    }
}

#[test]
fn dimstyle_fidelity_bridge_true_colors_keep_all_24_bits() {
    let mut d = Drawing::new_imperial();
    let rgb = Color::True(cadcraft_color::Rgb(187, 130, 157));
    let st = &mut d.dim_styles[0];
    st.dim_line_color = rgb;
    st.ext_line_color = rgb;
    st.text_color = rgb;
    st.text_fill = 2;
    st.text_fill_color = rgb;
    for version in [Version::R2000, Version::R2004, Version::R2007, Version::R2018] {
        let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, version)).unwrap();
        let public = tags(&d, version);
        for code in 421..=424 {
            assert!(!public.iter().any(|t| t.code == code && t.i64() == 0xbb829d), "public DXF {version:?}/{code}");
            let supported = version >= if code == 424 { Version::R2007 } else { Version::R2004 };
            assert_eq!(bridge.iter().any(|t| t.code == code && t.i64() == 0xbb829d), supported, "{version:?}/{code}");
        }
    }
}

#[test]
fn private_header_variables_stay_in_the_dwg_bridge() {
    let mut d = Drawing::new_imperial();
    for (name, code, value) in [
        ("ISOLINES", 70, HVal::Int(4)),
        ("FACETRES", 40, HVal::Real(0.5)),
        ("TEXTQLTY", 70, HVal::Int(50)),
        ("PELLIPSE", 70, HVal::Int(0)),
        ("DIMMZF", 40, HVal::Real(1.0)),
        ("DIMMZS", 1, HVal::Str("m".into())),
        ("DIMALTMZF", 40, HVal::Real(1.0)),
        ("DIMALTMZS", 1, HVal::Str("cm".into())),
    ] {
        d.header.set(name, value);
        d.header.dxf_codes.insert(name.into(), vec![code]);
    }
    let public = tags(&d, Version::R2018);
    let bridge = cadcraft_dxf::parse(&dwg_bridge(&d, Version::R2018)).unwrap();
    for name in ["ISOLINES", "FACETRES", "TEXTQLTY", "PELLIPSE", "DIMMZF", "DIMMZS", "DIMALTMZF", "DIMALTMZS"] {
        let variable = format!("${name}");
        assert!(!public.iter().any(|t| t.code == 9 && t.str() == variable), "public {variable}");
        assert!(bridge.iter().any(|t| t.code == 9 && t.str() == variable), "bridge {variable}");
    }
}

#[test]
fn entity_groups_does_not_scan_the_document_namespace() {
    let mut d = Drawing::new_metric();
    let h = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    for _ in 0..1000 {
        d.add(&Space::Model, Common::default(), EntityKind::Point(Point { p: Vec3::ZERO, angle: 0.0 })).unwrap();
    }
    SCANNED.with(|count| count.set(0));
    let (kind, tags) = entity_groups(&d, d.entity(h).unwrap());
    assert_eq!(kind, "LINE");
    assert!(tags.iter().any(|t| t.code == 5 && t.str() == h.hex()));
    SCANNED.with(|count| assert_eq!(count.get(), 0, "entget must not reserve every drawing handle"));
}

//! DIMSTYLE groups and DSTYLE overrides survive the R2000 DXF used by the DWG bridge.

use cadcraft_doc::{Common, DimKind, DimStyle, Dimension, Drawing, EntityKind, Linetype, Space};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn drawing() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.linetypes.push(Linetype::simple("HIDDEN", "Hidden", &[0.25, -0.125]));
    d.dim_styles.push(DimStyle {
        name: "Detail".into(),
        fixed_ext_length: 0.98,
        jog_angle: 90.0,
        arc_symbol: 2,
        fixed_ext: true,
        dim_linetype: "ByLayer".into(),
        ext_linetype1: "HIDDEN".into(),
        ext_linetype2: "HIDDEN".into(),
        ..DimStyle::default()
    });
    d
}

#[test]
fn dimstyle_fidelity_newer_groups_survive_the_dwg_bridge() {
    let d = drawing();
    for version in [Version::R2007, Version::R2018] {
        let opts = SaveOptions { version: Some(version), binary: false };
        let dxf = cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap();
        let direct = cadcraft_io::read_dxf(&dxf).unwrap();
        assert_eq!(direct.dim_style("Detail"), d.dim_style("Detail"), "DXF {version:?}");
        let converted = cadcraft_dwg::dxf_to_dwg(&dxf).unwrap();
        let back = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&converted).unwrap()).unwrap();
        assert_eq!(back.dim_style("Detail"), d.dim_style("Detail"), "DXF to DWG {version:?}");
        let dwg = cadcraft_io::write_with(&d, "x.dwg", &opts).unwrap();
        let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        assert_eq!(back.dim_style("Detail"), d.dim_style("Detail"), "R2000 bridge {version:?}");
    }
}

#[test]
fn dimstyle_fidelity_editing_variables_survive_dxf_and_dwg() {
    let mut d = drawing();
    let values = serde_json::json!({"DIMUPT": 1, "DIMALTTD": 3, "DIMALTTZ": 7, "DIMTXTDIRECTION": 1, "DIMSAH": 1});
    let st = d.dim_styles.iter_mut().find(|s| s.name == "Detail").unwrap();
    assert!(st.apply_fields(values.as_object().unwrap()).is_empty(), "all variables are modelled");
    let want = st.clone();
    let opts = SaveOptions { version: Some(Version::R2018), binary: false };
    let dxf = cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap();
    assert_eq!(cadcraft_io::read_dxf(&dxf).unwrap().dim_style("Detail"), Some(&want));
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &opts).unwrap();
    assert_eq!(cadcraft_io::read(&dwg, "x.dwg").unwrap().dim_style("Detail"), Some(&want));
}

#[test]
fn dimstyle_fidelity_unused_separate_arrow_names_do_not_enable_dimsah() {
    let mut d = drawing();
    let st = d.dim_styles.iter_mut().find(|s| s.name == "Detail").unwrap();
    st.arrow_block1 = "_Dot".into();
    st.separate_arrows = false;
    let want = st.clone();
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    assert_eq!(back.dim_style("Detail"), Some(&want));
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(back.dim_style("Detail"), Some(&want));
}

#[test]
fn dimstyle_fidelity_dimsah_controls_screen_and_saved_arrows() {
    let source = b"0\nSECTION\n2\nENTITIES\n0\nDIMENSION\n5\n100\n8\n0\n70\n0\n10\n0\n20\n5\n13\n0\n23\n0\n14\n20\n24\n0\n0\nENDSEC\n0\nEOF\n";
    let mut d = cadcraft_io::read_dxf(source).unwrap();
    let st = d.dim_styles.iter_mut().find(|s| s.name == "Standard").unwrap();
    st.arrow_block = "_ClosedFilled".into();
    st.arrow_block1 = "_None".into();
    st.arrow_block2 = "_Open".into();
    st.separate_arrows = false;
    let EntityKind::Dimension(dm) = &d.model.iter().next().unwrap().kind else { panic!("dimension") };
    assert_eq!(cadcraft_render::dimension_in(&d, dm).fills.len(), 2, "screen arrows");
    let opts = SaveOptions { version: Some(Version::R2018), binary: false };
    let dxf = cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap();
    let converted = cadcraft_dwg::dxf_to_dwg(&dxf).unwrap();
    let bridged = cadcraft_dwg::dwg_to_dxf(&converted).unwrap();
    let dwg = cadcraft_io::write_with(&d, "x.dwg", &opts).unwrap();
    for back in [cadcraft_io::read(&dxf, "x.dxf").unwrap(), cadcraft_io::read_dxf(&bridged).unwrap(), cadcraft_io::read(&dwg, "x.dwg").unwrap()] {
        let EntityKind::Dimension(dm) = &back.model.iter().next().unwrap().kind else { panic!("dimension") };
        let block = back.block(dm.block.as_deref().unwrap()).unwrap();
        assert_eq!(block.entities.iter().filter(|e| matches!(e.kind, EntityKind::Solid(_))).count(), 2, "saved arrows");
        assert_eq!(cadcraft_render::dimension_in(&back, dm).fills.len(), 2, "reopened screen arrows");
    }
}

#[test]
fn dimstyle_fidelity_measurement_factors_stay_private_in_user_dxf() {
    let mut d = drawing();
    let st = d.dim_styles.iter_mut().find(|s| s.name == "Detail").unwrap();
    let fields = serde_json::json!({"DIMMZF": 12.5, "DIMALTMZF": 25.0, "DIMMZS": "m", "DIMALTMZS": "cm"});
    assert!(st.apply_fields(fields.as_object().unwrap()).is_empty());
    let want = st.clone();
    for version in [Version::R2000, Version::R2010, Version::R2018] {
        let dxf = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let text = String::from_utf8(dxf.clone()).unwrap();
        let lines: Vec<_> = text.lines().map(str::trim).collect();
        let start = lines.windows(2).position(|p| p == ["2", "Detail"]).unwrap();
        let style: Vec<_> = lines[start..].chunks(2).take_while(|p| p[0] != "0").collect();
        assert!(!style.iter().any(|p| matches!(p[0], "51" | "52" | "300" | "301")), "no acadrust-private table groups in {version:?}");
        assert!(style.iter().any(|p| p[1] == "DIMVARS"), "private xdata in {version:?}");
        assert_eq!(cadcraft_io::read_dxf(&dxf).unwrap().dim_style("Detail"), Some(&want));
    }
}

#[test]
fn dimstyle_fidelity_true_colors_stay_private_in_user_dxf() {
    let mut d = drawing();
    let st = d.dim_styles.iter_mut().find(|s| s.name == "Detail").unwrap();
    let rgb = cadcraft_color::Color::True(cadcraft_color::Rgb(187, 130, 157));
    st.dim_line_color = rgb;
    st.ext_line_color = rgb;
    st.text_color = rgb;
    st.text_fill = 2;
    st.text_fill_color = rgb;
    let want = st.clone();
    for version in [Version::R2000, Version::R2007, Version::R2018] {
        let dxf = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let tags = cadcraft_dxf::parse(&dxf).unwrap();
        assert!(!tags.iter().any(|t| (421..=424).contains(&t.code)), "private RGB groups are not user DXF {version:?}");
        assert_eq!(cadcraft_io::read_dxf(&dxf).unwrap().dim_style("Detail"), Some(&want));
    }
}

fn dimension() -> Dimension {
    Dimension {
        kind: DimKind::Aligned,
        defpt: Vec3::new(0.0, 3.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Detail".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn overrides(d: &Drawing) -> &serde_json::Map<String, serde_json::Value> {
    d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(&dm.overrides) } else { None }).unwrap()
}

#[test]
fn dimstyle_fidelity_null_linetype_overrides_survive_dxf_and_dwg() {
    let mut d = drawing();
    let mut dm = dimension();
    dm.overrides = serde_json::json!({
        "dimLinetype": "", "extLinetype1": "", "extLinetype2": "",
        "leaderArrowBlock": "", "textStyle": "Standard", "arrowSize": 2.5,
    })
    .as_object()
    .unwrap()
    .clone();
    let want = dm.overrides.clone();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dm)).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let direct = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_eq!(overrides(&direct), &want, "DXF null handles");
    let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
    let back = cadcraft_io::read_dxf(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    assert_eq!(overrides(&back), &want, "DWG null handles");
}

#[test]
fn dimstyle_fidelity_missing_handle_values_do_not_consume_the_next_variable() {
    let mut d = drawing();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension())).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let at = text.find("AcDbAlignedDimension\r\n").unwrap() + "AcDbAlignedDimension\r\n".len();
    let missing = "1001\r\nACAD\r\n1000\r\nDSTYLE\r\n1002\r\n{\r\n1070\r\n347\r\n1070\r\n346\r\n1070\r\n345\r\n1070\r\n340\r\n1070\r\n341\r\n1070\r\n41\r\n1040\r\n2.5\r\n1002\r\n}\r\n";
    let text = format!("{}{missing}{}", &text[..at], &text[at..]);
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let want = serde_json::json!({
        "extLinetype2": "", "extLinetype1": "", "dimLinetype": "",
        "textStyle": "", "leaderArrowBlock": "", "arrowSize": 2.5,
    });
    assert_eq!(overrides(&back), want.as_object().unwrap());
    let dwg = cadcraft_io::write(&back, "x.dwg").unwrap();
    let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    assert_eq!(overrides(&back), want.as_object().unwrap(), "nulls are written explicitly");
}

#[test]
fn dimstyle_fidelity_measurement_factors_and_suffixes_survive_dwg() {
    let mut d = Drawing::new_imperial();
    let mut st = DimStyle { name: "Measurement".into(), ..DimStyle::default() };
    let fields = serde_json::json!({
        "DIMMZF": 12.5, "DIMALTMZF": 25.0,
        "DIMMZS": "m", "DIMALTMZS": "cm",
    });
    assert!(st.apply_fields(fields.as_object().unwrap()).is_empty());
    d.dim_styles.push(st.clone());
    for version in [Version::R2010, Version::R2013, Version::R2018] {
        let dwg = cadcraft_io::write_with(&d, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        assert_eq!(back.dim_style("Measurement"), Some(&st), "{version:?}");
    }
}

#[test]
fn dimstyle_fidelity_true_colors_survive_dwg() {
    let mut d = Drawing::new_imperial();
    let rgb = cadcraft_color::Color::True(cadcraft_color::Rgb(187, 130, 157));
    let st = DimStyle {
        name: "RGB".into(),
        dim_line_color: rgb,
        ext_line_color: rgb,
        text_color: rgb,
        text_fill: 2,
        text_fill_color: rgb,
        ..DimStyle::default()
    };
    d.dim_styles.push(st.clone());
    for version in [Version::R2007, Version::R2010, Version::R2018] {
        let dwg = cadcraft_io::write_with(&d, "x.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let back = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        assert_eq!(back.dim_style("RGB"), Some(&st), "{version:?}");
    }
}

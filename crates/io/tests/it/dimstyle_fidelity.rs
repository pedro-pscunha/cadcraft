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

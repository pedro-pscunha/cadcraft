//! A dimension's DSTYLE override pairs are written in ascending group code, as AutoCAD writes them (not in the alphabetical order of the
//! override names), and read back as the same overrides.

use cadcraft_doc::{Common, DimKind, Dimension, Drawing, EntityKind, Space};
use cadcraft_geom::Vec3;

fn dimension(overrides: serde_json::Value) -> Drawing {
    let mut d = Drawing::new_metric();
    let dm = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(15.0, 5.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(30.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: overrides.as_object().cloned().unwrap_or_default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dm)).unwrap();
    d
}

/// The keys (the 1070 group that names the variable) of the DSTYLE pairs of the first DIMENSION in the file.
fn dstyle_keys(dxf: &str) -> Vec<i64> {
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 1000 && t.str() == "DSTYLE").expect("a DSTYLE");
    let end = tags[start..].iter().position(|t| t.code == 1002 && t.str() == "}").map(|p| start + p).unwrap();
    // Between "{" and "}" the groups pair up: the variable code, then its value.
    tags[start + 2..end].chunks(2).map(|pair| pair[0].i64()).collect()
}

#[test]
fn the_override_pairs_are_in_ascending_code_whatever_the_names_sort_to() {
    // DIMTXT (140), DIMSCALE (40), DIMEXO (42) and DIMGAP (147): the names sort as DIMEXO, DIMGAP, DIMSCALE, DIMTXT.
    let d = dimension(serde_json::json!({"DIMTXT": 0.5, "DIMSCALE": 2.0, "DIMEXO": 0.3, "DIMGAP": 0.2}));
    let keys = dstyle_keys(&cadcraft_io::write_dxf(&d));
    assert_eq!(keys, vec![40, 42, 140, 147]);
}

#[test]
fn the_overrides_survive_a_save_in_the_new_order() {
    let d = dimension(serde_json::json!({"textHeight": 0.5, "scale": 2.0}));
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let overrides = |d: &Drawing| match &d.model.iter().next().unwrap().kind {
        EntityKind::Dimension(dm) => dm.overrides.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(overrides(&back), overrides(&d));
}

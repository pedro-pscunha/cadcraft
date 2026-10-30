//! Group 52 (oblique angle) of a linear or aligned dimension is written only for an oblique that
//! differs from the perpendicular, as AutoCAD writes it: AutoCAD LT 2025 refuses a DXF whose
//! DIMENSION has `52 / 90.0` ("unexpected DXF group code: 52"). The model keeps its oblique angle.

use cadcraft_doc::{Common, DimKind, DimMarks, Dimension, Drawing, EntityKind, Space};
use cadcraft_geom::Vec3;

fn dimension(kind: DimKind, oblique: f64) -> EntityKind {
    EntityKind::Dimension(Dimension {
        kind,
        defpt: Vec3::new(0.0, 3.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: DimMarks { oblique: Some(oblique.to_radians()), ..Default::default() },
        leader_length: 0.0,
    })
}

/// The 52 groups of the DIMENSION records of a saved text.
fn groups_52(d: &Drawing, binary_free: bool) -> Vec<Vec<f64>> {
    let text = if binary_free {
        cadcraft_io::write_dxf(d)
    } else {
        String::from_utf8(cadcraft_dwg::dwg_to_dxf(&cadcraft_io::write(d, "x.dwg").unwrap()).unwrap()).unwrap()
    };
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects =
        cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").map(|s| cadcraft_dxf::records(&s.tags)).unwrap_or_default();
    objects.iter().filter(|(k, _)| k == "DIMENSION").map(|(_, t)| t.iter().filter(|t| t.code == 52).map(|t| t.f64()).collect()).collect()
}

#[test]
fn a_perpendicular_oblique_writes_no_group_52() {
    let mut d = Drawing::new_imperial();
    // Linear dimension lines at 0 and 30 degrees; extension lines at 90 (and -270, 270) less the
    // dimension line's angle, and at 120 degrees (30 + 90) are all the perpendicular. An aligned one
    // along +X likewise. The last two are 60 degrees to the dimension line.
    for (kind, oblique) in [
        (DimKind::Linear { rotation: 0.0 }, 90.0),
        (DimKind::Linear { rotation: 0.0 }, -270.0),
        (DimKind::Linear { rotation: 30f64.to_radians() }, 120.0),
        (DimKind::Aligned, 90.0),
        (DimKind::Linear { rotation: 0.0 }, 60.0),
        (DimKind::Aligned, 60.0),
    ] {
        d.add(&Space::Model, Common::default(), dimension(kind, oblique)).unwrap();
    }
    let kept: Vec<Option<f64>> =
        d.model.iter().filter_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm.marks.oblique) } else { None }).collect();
    let text = groups_52(&d, true);
    assert_eq!(text.len(), 6);
    for (i, g) in text.iter().enumerate().take(4) {
        assert!(g.is_empty(), "dimension {i} has a perpendicular oblique: {g:?}");
    }
    for (i, g) in text.iter().enumerate().skip(4) {
        assert!(g.len() == 1 && (g[0] - 60.0).abs() < 1e-9, "dimension {i} has an oblique: {g:?}");
    }
    // The model keeps what it had.
    assert!(kept.iter().all(Option::is_some));
    // A DWG has the same.
    let dwg = groups_52(&d, false);
    assert!(dwg.iter().take(4).all(|g| g.is_empty()), "{dwg:?}");
}

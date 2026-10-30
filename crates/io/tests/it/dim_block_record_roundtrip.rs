//! A dimension that still uses the graphics block it was read with writes that block as read: its base point (BLOCK 10),
//! description (BLOCK 4) and BLOCK_RECORD insertion units (70), explodability (280) and scalability (281).

use std::sync::Arc;

use cadcraft_doc::{Block, Common, DimKind, Dimension, Drawing, EntityKind, Space};
use cadcraft_geom::Vec3;

fn linear() -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(5.0, 3.0, 0.0),
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
        marks: Default::default(),
        leader_length: 0.0,
    }
}

/// The drawing after a save and reopen, with its dimension block set to non-default record settings.
fn drawing_with_edited_dim_block() -> (Drawing, String) {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(linear())).unwrap();
    let mut d = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    let name = d.blocks.keys().find(|n| n.starts_with("*D")).cloned().expect("a dimension block");
    let mut b: Block = d.blocks[&name].as_ref().clone();
    b.base = Vec3::new(2.0, -3.0, 0.5);
    b.description = "graphics of a dimension".into();
    b.units = 4;
    b.explodable = false;
    b.scale_uniformly = true;
    d.blocks.insert(name.clone(), Arc::new(b));
    (d, name)
}

fn settings(d: &Drawing, name: &str) -> (Vec3, String, u8, bool, bool) {
    let b = d.blocks.get(name).unwrap_or_else(|| panic!("no block {name}"));
    (b.base, b.description.clone(), b.units, b.explodable, b.scale_uniformly)
}

#[test]
fn a_reused_dimension_block_keeps_its_base_point_and_record_settings() {
    let (d, name) = drawing_with_edited_dim_block();
    let want = settings(&d, &name);
    let dxf = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    assert_eq!(settings(&dxf, &name), want, "DXF round trip");
    let dwg = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(settings(&dwg, &name), want, "DWG round trip");
}

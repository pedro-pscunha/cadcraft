//! The entity-level groups that end an AutoCAD MULTILEADER, measured in AutoCAD LT 2025 files. A text multileader carries the
//! block groups (93, 10, 43, 176) with their defaults, and an AC1032 entity ends with 295 (an AC1024 one does not). The entity's
//! own block rotation (43) stays as it was when the multileader is rotated: the rotation lives in the context's 46.

use cadcraft_doc::{Block, Circle, Common, Drawing, Entity, EntityKind, Handle, Insert, MLeader, Space};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version};

/// The groups after `AcDbMLeader` of the first MULTILEADER of an AutoCAD LT 2025 DXFOUT (tilted UCS, text "Tilted"), without the
/// cached graphics (160, 310).
const AUTOCAD_TEXT_BODY: &str = "270\n2\n300\nCONTEXT_DATA{\n40\n1.0\n10\n10.60660171779821\n20\n6.0\n30\n10.60660171779821\n41\n2.5\n140\n2.5\n145\n0.625\n174\n1\n175\n1\n176\n0\n177\n0\n290\n1\n304\nTilted\n11\n-0.7071067811865476\n21\n0.0\n31\n0.7071067811865476\n340\n112E\n12\n11.0485434560398\n22\n7.25\n32\n11.0485434560398\n13\n0.7071067811865476\n23\n0.0\n33\n0.7071067811865476\n42\n1.570796326794896\n43\n0.0\n44\n0.0\n45\n1.0\n170\n1\n90\n-1073741824\n171\n1\n172\n5\n91\n-1073741824\n141\n0.0\n92\n0\n291\n0\n292\n0\n173\n0\n293\n0\n142\n0.0\n143\n0.0\n294\n0\n295\n0\n296\n0\n110\n1.414213562373095\n120\n2.0\n130\n1.414213562373095\n111\n0.7071067811865476\n121\n0.0\n131\n0.7071067811865476\n112\n0.0\n122\n1.0\n132\n0.0\n297\n0\n302\nLEADER{\n290\n1\n291\n1\n10\n7.071067811865475\n20\n6.0\n30\n7.071067811865475\n11\n0.7071067811865476\n21\n0.0\n31\n0.7071067811865476\n90\n0\n40\n5.0\n304\nLEADER_LINE{\n10\n1.414213562373095\n20\n2.0\n30\n1.414213562373095\n91\n0\n305\n}\n271\n0\n303\n}\n272\n4\n273\n0\n301\n}\n340\n1106\n90\n279552\n170\n1\n91\n-1056964608\n341\n1127\n171\n-2\n290\n1\n291\n1\n41\n5.0\n42\n2.5\n172\n2\n343\n112E\n173\n1\n95\n1\n174\n1\n175\n0\n92\n-1056964608\n292\n0\n93\n-1056964608\n10\n1.0\n20\n1.0\n30\n1.0\n43\n0.0\n176\n0\n293\n0\n294\n0\n178\n0\n179\n1\n45\n1.0\n271\n0\n272\n4\n273\n0\n295\n0\n";

fn dxf_with(body: &str) -> String {
    format!("0\nSECTION\n2\nENTITIES\n0\nMULTILEADER\n5\nA0\n100\nAcDbEntity\n8\n0\n100\nAcDbMLeader\n{body}0\nENDSEC\n0\nEOF\n")
}

fn write_at(d: &Drawing, version: Version) -> Vec<(i32, String)> {
    let opts = SaveOptions { version: Some(version), binary: false };
    let text = String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let groups: Vec<(i32, String)> =
        lines.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].trim().parse().unwrap_or(-1), c[1].trim().to_string())).collect();
    let start = groups.iter().position(|g| g.0 == 0 && g.1 == "MULTILEADER").expect("a MULTILEADER record");
    let end = groups[start + 1..].iter().position(|g| g.0 == 0).map_or(groups.len(), |n| start + 1 + n);
    groups[start + 1..end].to_vec()
}

/// The groups after the context's closing `301 }`.
fn entity_part(record: &[(i32, String)]) -> &[(i32, String)] {
    let at = record.iter().rposition(|g| g.0 == 301).expect("the context end");
    &record[at + 1..]
}

fn codes(part: &[(i32, String)], from: i32) -> Vec<i32> {
    let start = part.iter().position(|g| g.0 == from).expect("the group");
    part[start..].iter().map(|g| g.0).collect()
}

fn value(part: &[(i32, String)], code: i32) -> f64 {
    part.iter().find(|g| g.0 == code).map(|g| g.1.parse::<f64>().unwrap()).expect("the group")
}

/// The AutoCAD multileader moved by one unit, so that no source replay applies and the entity is written from the model.
fn moved_autocad_text() -> Drawing {
    let source = cadcraft_io::read_dxf(dxf_with(AUTOCAD_TEXT_BODY).as_bytes()).unwrap();
    let e = source.model.iter().next().expect("the multileader").as_ref().clone();
    let moved = e.transformed(&Mat4::from_mat3(&Mat3::translate(Vec2::new(1.0, 0.0)))).expect("the edit");
    let mut d = Drawing::new_metric();
    d.model.push(moved);
    d
}

#[test]
fn a_text_multileader_ends_like_the_autocad_entity() {
    let record = write_at(&moved_autocad_text(), Version::R2018);
    let part = entity_part(&record);
    assert_eq!(codes(part, 292), vec![292, 93, 10, 20, 30, 43, 176, 293, 294, 178, 179, 45, 271, 272, 273, 295]);
    assert_eq!(value(part, 93), -1_056_964_608.0);
    assert_eq!((value(part, 10), value(part, 20), value(part, 30)), (1.0, 1.0, 1.0));
    assert_eq!((value(part, 43), value(part, 176), value(part, 295)), (0.0, 0.0, 0.0));
}

#[test]
fn old_targets_omit_newer_attachment_fields_even_after_source_replay() {
    let source = cadcraft_io::read_dxf(dxf_with(AUTOCAD_TEXT_BODY).as_bytes()).unwrap();
    for version in [Version::R2000, Version::R2004, Version::R2007] {
        let record = write_at(&source, version);
        assert!(record.iter().all(|group| !matches!(group.0, 271..=273)), "{version:?}");
        assert!(entity_part(&record).iter().all(|group| group.0 != 295), "{version:?}");
    }
}

#[test]
fn group_295_ends_an_ac1032_entity_only() {
    let record = write_at(&moved_autocad_text(), Version::R2010);
    let part = entity_part(&record);
    assert_eq!(codes(part, 292), vec![292, 93, 10, 20, 30, 43, 176, 293, 294, 178, 179, 45, 271, 272, 273]);
}

fn block_multileader(entity_rotation: Option<f64>) -> Drawing {
    let mut d = Drawing::new_metric();
    let mut blk = Block::new("BUBBLE");
    blk.entities.push(Entity::new(Handle(0x200), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 })));
    d.blocks.insert(blk.name.clone(), std::sync::Arc::new(blk));
    let insert = Insert {
        block: "BUBBLE".into(),
        insert: Vec3::new(12.0, 3.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.5,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let mut m = MLeader::single(Vec3::new(10.0, 3.0, 0.0), 1.0, false, vec![vec![Vec3::ZERO, Vec3::new(4.0, 5.0, 0.0)]], None);
    m.block = Some(insert);
    m.context.codes.groups.entity.block_rotation = entity_rotation;
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(m)).unwrap();
    d
}

#[test]
fn a_rotated_block_multileader_keeps_the_entity_rotation_apart_from_the_context() {
    // AutoCAD after ROTATE: 46 of the context 1.5708 -> 2.0944, 43 of the entity stays 0.
    let record = write_at(&block_multileader(Some(0.0)), Version::R2018);
    let part = entity_part(&record);
    assert_eq!(value(part, 43), 0.0);
    let context: Vec<_> = record.iter().take_while(|g| g.0 != 301).cloned().collect();
    assert!((value(&context, 46) - 0.5).abs() < 1e-12);
}

#[test]
fn a_new_block_multileader_writes_the_rotation_in_both_places() {
    let record = write_at(&block_multileader(None), Version::R2018);
    assert!((value(entity_part(&record), 43) - 0.5).abs() < 1e-12);
}

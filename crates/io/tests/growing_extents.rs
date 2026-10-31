use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line, geometry_key, legacy_entity_bounds};
use cadcraft_geom::Vec3;

fn source() -> Drawing {
    cadcraft_io::read(b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$EXTMIN\n10\n-20\n20\n-30\n30\n-40\n9\n$EXTMAX\n10\n190.5\n20\n12\n30\n50\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n200\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n10\n21\n0\n31\n0\n0\nTEXT\n5\n201\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n189.5\n20\n0\n30\n0\n40\n10\n1\nXXXXXXXXXX\n100\nAcDbText\n73\n0\n0\nENDSEC\n0\nEOF\n", "source.dxf").unwrap()
}

fn extent_bits(drawing: &Drawing) -> [u64; 6] {
    let min = drawing.header.point("EXTMIN").unwrap();
    let max = drawing.header.point("EXTMAX").unwrap();
    [min.x.to_bits(), min.y.to_bits(), min.z.to_bits(), max.x.to_bits(), max.y.to_bits(), max.z.to_bits()]
}

fn move_line(drawing: &mut Drawing, delta: Vec3) {
    drawing
        .modify_entity(Handle(0x200), |entity| {
            let EntityKind::Line(line) = &mut entity.kind else { panic!("line") };
            line.a = line.a + delta;
            line.b = line.b + delta;
        })
        .unwrap();
}

#[test]
fn a_no_edit_save_keeps_extent_bits_despite_different_legacy_text_bounds() {
    let drawing = source();
    let text = drawing.entity(Handle(0x201)).unwrap();
    assert!(legacy_entity_bounds(&drawing, text, 0).unwrap().max.x > 190.5);
    for entity in drawing.model.iter() {
        assert_eq!(geometry_key(&entity.kind, &entity.common).as_deref(), Some(entity.common.geometry.as_ref().unwrap().model.as_str()));
    }
    for name in ["noedit.dxf", "noedit.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, name, &cadcraft_io::SaveOptions::default()).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(extent_bits(&reopened), extent_bits(&drawing), "{name}");
        assert!(notes.is_empty(), "{notes:?}");
    }
}

#[test]
fn a_far_move_grows_only_the_saved_maximum() {
    let mut drawing = source();
    move_line(&mut drawing, Vec3::new(1000.0, 1000.0, 0.0));
    for name in ["far.dxf", "far.dwg"] {
        let bytes = cadcraft_io::write(&drawing, name).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(reopened.header.point("EXTMIN"), Some(Vec3::new(-20.0, -30.0, -40.0)), "{name}");
        assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(1010.0, 1000.0, 50.0)), "{name}");
    }
}

#[test]
fn an_inward_move_does_not_shrink_or_recompute_saved_extents() {
    let mut drawing = source();
    let before = extent_bits(&drawing);
    move_line(&mut drawing, Vec3::new(5.0, 5.0, 0.0));
    for name in ["inward.dxf", "inward.dwg"] {
        let bytes = cadcraft_io::write(&drawing, name).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(extent_bits(&reopened), before, "{name}");
    }
}

#[test]
fn an_added_entity_without_a_snapshot_grows_saved_extents() {
    let mut drawing = source();
    drawing.model.push(Entity::new(Handle(0x300), EntityKind::Line(Line { a: Vec3::new(1200.0, 1300.0, 0.0), b: Vec3::new(1210.0, 1310.0, 0.0) })));
    let bytes = cadcraft_io::write(&drawing, "added.dxf").unwrap();
    let reopened = cadcraft_io::read(&bytes, "added.dxf").unwrap();
    assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(1210.0, 1310.0, 50.0)));
}

#[test]
fn canonical_world_bounds_grow_the_saved_pair_alongside_legacy_edits() {
    let mut drawing = source();
    move_line(&mut drawing, Vec3::new(1000.0, 1000.0, 0.0));
    let mut canonical = Entity::new(Handle(0x2A0), EntityKind::Circle(Circle { center: Vec3::new(1e6, 1e6, 0.0), radius: 4.0 }));
    canonical.geometry_revision = GeometryRevision::CanonicalV2;
    canonical.common.extrusion = Vec3::new(0.0, 0.0, -1.0);
    drawing.model.push(canonical);
    for name in ["canonical-growth.dxf", "canonical-growth.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, name, &cadcraft_io::SaveOptions::default()).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        // Negative OCS X places the circle at WCS (-1e6,1e6), not (+1e6,1e6).
        assert_eq!(reopened.header.point("EXTMIN"), Some(Vec3::new(-1_000_004.0, -30.0, -40.0)), "{name}");
        assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(1010.0, 1_000_004.0, 50.0)), "{name}");
        assert!(notes.is_empty(), "{name}: {notes:?}");
    }
}

#[test]
fn a_headerless_canonical_drawing_gets_world_extents() {
    let mut drawing = Drawing::new_metric();
    assert!(drawing.header.point("EXTMIN").is_none() && drawing.header.point("EXTMAX").is_none());
    let mut canonical = Entity::new(Handle(0x2A0), EntityKind::Circle(Circle { center: Vec3::new(100.0, 20.0, 3.0), radius: 2.0 }));
    canonical.geometry_revision = GeometryRevision::CanonicalV2;
    canonical.common.extrusion = Vec3::new(0.0, 0.0, -1.0);
    drawing.model.push(canonical);
    for name in ["canonical-fresh.dxf", "canonical-fresh.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, name, &cadcraft_io::SaveOptions::default()).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(reopened.header.point("EXTMIN"), Some(Vec3::new(-102.0, 18.0, 0.0)), "{name}");
        assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(-98.0, 22.0, 0.0)), "{name}");
        assert!(notes.is_empty(), "{name}: {notes:?}");
    }
}

#[test]
fn unsupported_canonical_bounds_keep_available_growth_and_name_every_refusal() {
    let mut drawing = source();
    move_line(&mut drawing, Vec3::new(1000.0, 1000.0, 0.0));
    for handle in [0x2A0, 0x2A1] {
        let mut canonical = Entity::new(Handle(handle), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 4.0 }));
        canonical.geometry_revision = GeometryRevision::CanonicalV2;
        canonical.common.extrusion = Vec3::ZERO;
        drawing.model.push(canonical);
    }
    let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, "refused.dxf", &cadcraft_io::SaveOptions::default()).unwrap();
    let reopened = cadcraft_io::read(&bytes, "refused.dxf").unwrap();
    assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(1010.0, 1000.0, 50.0)));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("2A0") && notes[0].contains("2A1"), "{notes:?}");
}

#[test]
fn legacy_save_bytes_match_the_frozen_writer() {
    // Full ASCII DXF fingerprints captured at e0f63fd2 before the extent dispatch changes.
    for (case, expected, length) in [
        ("unchanged", 0x08ff_ffaf_b945_8399_u64, 18051),
        ("inward", 0x24dd_ebb2_0b3a_666d, 18051),
        ("far", 0x20b3_61b2_c71c_6426, 18065),
        ("added", 0xd30d_dd4c_e88f_03c7, 18198),
    ] {
        let mut drawing = source();
        match case {
            "inward" => move_line(&mut drawing, Vec3::new(5.0, 5.0, 0.0)),
            "far" => move_line(&mut drawing, Vec3::new(1000.0, 1000.0, 0.0)),
            "added" => drawing
                .model
                .push(Entity::new(Handle(0x300), EntityKind::Line(Line { a: Vec3::new(1200.0, 1300.0, 0.0), b: Vec3::new(1210.0, 1310.0, 0.0) }))),
            _ => {}
        }
        let bytes = cadcraft_io::write(&drawing, "legacy.dxf").unwrap();
        let fingerprint = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3));
        assert_eq!((fingerprint, bytes.len()), (expected, length), "{case}");
    }
}

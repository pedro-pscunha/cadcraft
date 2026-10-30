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
fn a_refused_added_canonical_entity_keeps_legacy_growth_and_adds_one_note() {
    let mut drawing = source();
    move_line(&mut drawing, Vec3::new(1000.0, 1000.0, 0.0));
    let mut canonical = Entity::new(Handle(0x2A0), EntityKind::Circle(Circle { center: Vec3::new(1e6, 1e6, 0.0), radius: 4.0 }));
    canonical.geometry_revision = GeometryRevision::CanonicalV2;
    drawing.model.push(canonical);
    let (bytes, notes) = cadcraft_io::write_with_noted(&drawing, "refused.dxf", &cadcraft_io::SaveOptions::default()).unwrap();
    let reopened = cadcraft_io::read(&bytes, "refused.dxf").unwrap();
    assert_eq!(reopened.header.point("EXTMAX"), Some(Vec3::new(1010.0, 1000.0, 50.0)));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("2A0"), "{notes:?}");
}

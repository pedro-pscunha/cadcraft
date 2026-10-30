//! Attribute identities share the document allocator, including repeated save/read/edit cycles.

use std::collections::HashSet;

use cadcraft_doc::{Common, Drawing, EntityKind, Handle, Line, Space};
use cadcraft_dxf::{records, sections};
use cadcraft_geom::Vec3;

fn drawing() -> Drawing {
    cadcraft_io::read_dxf(include_bytes!("../data/attrib_fields.dxf")).unwrap()
}

fn assert_unique_file(text: &str, stage: &str) {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let seed = tags.windows(2).find(|p| p[0].code == 9 && p[0].str() == "$HANDSEED").map(|p| Handle::parse_hex(&p[1].str()).unwrap()).unwrap();
    let mut used = HashSet::new();
    for section in sections(&tags).into_iter().filter(|s| s.name != "HEADER") {
        for (kind, body) in records(&section.tags) {
            // Later group 5 values in SORTENTSTABLE are sort keys, not object identities.
            if let Some(tag) = body.iter().find(|t| matches!(t.code, 5 | 105)) {
                let h = Handle::parse_hex(&tag.str()).unwrap();
                assert!(used.insert(h), "{stage}: duplicate {h:?} on {kind}");
                assert!(h.0 < seed.0, "{stage}: {kind} handle {h:?} is not below HANDSEED {seed:?}");
            }
        }
    }
}

#[test]
fn edit_insert_copy_and_reopen_cycles_keep_every_record_handle_unique() {
    for dwg in [false, true] {
        let mut d = drawing();
        for cycle in 0..12 {
            d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
            let mut insert = d
                .model
                .iter()
                .find_map(|e| match &e.kind {
                    EntityKind::Insert(i) => Some(i.clone()),
                    _ => None,
                })
                .unwrap();
            for a in &mut insert.attribs {
                a.handle = None;
            }
            let h = d.add(&Space::Model, Common::default(), EntityKind::Insert(insert)).unwrap();
            let original = (**d.entity(h).unwrap()).clone();
            for copy in d.duplicate_entities(vec![vec![original]], cadcraft_doc::DuplicationSource::SameDrawing).unwrap() {
                d.model.push(copy);
            }
            let text = if dwg {
                let bytes = cadcraft_io::write(&d, "x.dwg").unwrap();
                String::from_utf8(cadcraft_dwg::dwg_to_dxf(&bytes).unwrap()).unwrap()
            } else {
                cadcraft_io::write_dxf(&d)
            };
            assert_unique_file(&text, &format!("DWG={dwg}, cycle={cycle}"));
            d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        }
    }
}

#[test]
fn read_rejects_attribute_handles_already_used_by_any_record() {
    for duplicate in ["30", "32"] {
        let source = include_str!("../data/attrib_fields.dxf");
        assert_eq!(source.matches("0\nATTRIB\n5\n33\n").count(), 1);
        let source = source.replace("0\nATTRIB\n5\n33\n", &format!("0\nATTRIB\n5\n{duplicate}\n"));
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        let mut used: HashSet<_> = d.model.iter().map(|e| e.handle).collect();
        for e in d.model.iter() {
            if let EntityKind::Insert(i) = &e.kind {
                for a in &i.attribs {
                    let h = a.handle.expect("reader allocates missing or rejected identities");
                    assert!(used.insert(h), "reader accepted duplicate attribute handle {h:?}");
                    assert!(h.0 < d.handseed, "attribute identity is outside the allocator");
                }
            }
        }
        assert_unique_file(&cadcraft_io::write_dxf(&d), "hostile duplicate input");
    }
}

#[test]
fn writer_backstop_rejects_duplicate_and_writer_range_attribute_handles() {
    let mut d = drawing();
    let range = d.handseed.saturating_add(0x1000);
    d.modify_entity(Handle(0x31), |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.attribs[0].handle = Some(Handle(0x30));
            i.attribs[1].handle = Some(Handle(range));
        }
    })
    .unwrap();
    assert_unique_file(&cadcraft_io::write_dxf(&d), "writer backstop");
}

fn private_seed_source() -> String {
    let source = include_str!("../data/attrib_fields.dxf");
    assert_eq!(source.matches("100\nAcDbDictionary\n0\nDICTIONARY\n5\n40\n").count(), 1);
    source
        .replace("100\nAcDbDictionary\n0\nDICTIONARY\n5\n40\n", "100\nAcDbDictionary\n3\nCADCRAFT_HANDSEED\n350\nFE\n0\nDICTIONARY\n5\n40\n")
        .replace("0\nENDSEC\n0\nEOF\n", "0\nXRECORD\n5\nFE\n330\nC\n100\nAcDbXrecord\n280\n1\n1\n100\n0\nENDSEC\n0\nEOF\n")
}

#[test]
fn repaired_attribute_identity_does_not_consume_an_unread_foreign_object_handle() {
    let source = private_seed_source()
        .replace("0\nATTRIB\n5\n32\n", "0\nATTRIB\n5\n30\n")
        .replace("5\n43\n", "5\n100\n")
        .replace("330\n43\n", "330\n100\n")
        .replace("360\n43\n", "360\n100\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert_eq!(
        d.foreign.objects.iter().filter(|o| o.kind == "FIELD").count(),
        2,
        "repair allocated an attribute over the valid child FIELD's identity"
    );
    let i = d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i) } else { None }).unwrap();
    assert!(i.attribs.iter().all(|a| a.handle != Some(Handle(0x100))), "retained input object identity was reused");
}

#[test]
fn referenced_symbol_roles_are_reserved_and_emitted_before_remapping() {
    let source = private_seed_source().replace("0\nATTRIB\n5\n32\n","0\nATTRIB\n5\n30\n")
        .replace("0\nSECTION\n2\nBLOCKS\n","0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n5\nFF\n70\n1\n0\nAPPID\n5\n100\n2\nAPPDOM\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n")
        .replace("3\nACAD_FIELD\n360\n41\n0\nDICTIONARY\n","3\nACAD_FIELD\n360\n41\n1001\nAPPDOM\n1005\n100\n0\nDICTIONARY\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(d.foreign.targets.contains_key(&Handle(0x100)), "fixture must reference the APPID's input identity");
    let i = d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i) } else { None }).unwrap();
    assert!(i.attribs.iter().all(|a| a.handle != Some(Handle(0x100))), "repair reused a retained role identity");
    let text = cadcraft_io::write_dxf(&d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let records: Vec<_> = sections(&tags).into_iter().flat_map(|s| records(&s.tags)).collect();
    let app = &records.iter().find(|(k, t)| k == "APPID" && t.iter().any(|t| t.code == 2 && t.str() == "APPDOM")).unwrap().1;
    let handle = app.iter().find(|t| t.code == 5).unwrap().str();
    let dict = &records.iter().find(|(k, t)| k == "DICTIONARY" && t.iter().any(|t| t.code == 1001 && t.str() == "APPDOM")).unwrap().1;
    assert_eq!(dict.iter().find(|t| t.code == 1005).unwrap().str(), handle, "late APPID planning turned a live symbol reference into null");
}

#[test]
fn adding_an_insert_allocates_every_attribute_before_writing() {
    let mut d = drawing();
    let mut i = d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i.clone()) } else { None }).unwrap();
    for a in &mut i.attribs {
        a.handle = None;
    }
    let h = d.add(&Space::Model, Common::default(), EntityKind::Insert(i)).unwrap();
    let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { panic!("insert") };
    assert!(i.attribs.iter().all(|a| a.handle.is_some_and(|h| h.0 < d.handseed)), "new attributes must belong to the document allocator");
}

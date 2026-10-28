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
            for tag in body.iter().filter(|t| matches!(t.code, 5 | 105)) {
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
            let mut copy = (**d.entity(h).unwrap()).clone();
            copy.handle = d.new_handle();
            d.copy_foreign_data(h, &mut copy);
            d.model.push(copy);
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

//! Old hosts need a source proxy cache or an explicit save note, never a stale cache.
use cadcraft_doc::{Common, Drawing, EntityKind, MLeader, MText, Space};
use cadcraft_dxf::{Tag, Value, records, sections};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version};

fn options() -> SaveOptions {
    SaveOptions { version: Some(Version::R2004), binary: false }
}
fn record(bytes: &[u8]) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| records(&s.tags))
        .find(|(kind, _)| kind == "MULTILEADER")
        .expect("native MULTILEADER")
        .1
}
fn cache(body: &[Tag]) -> Vec<(i32, String)> {
    let start = body.iter().position(|tag| tag.code == 100 && tag.str() == "AcDbEntity").unwrap() + 1;
    body.iter()
        .skip(start)
        .take_while(|tag| tag.code != 100)
        .filter(|tag| matches!(tag.code, 92 | 160 | 310))
        .map(|tag| (tag.code, tag.str()))
        .collect()
}
fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let text = MText {
        insert: Vec3::new(3.0, 2.0, 0.0),
        height: 1.0,
        width: 5.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "source".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
        drawing_direction: None,
    };
    let leader = MLeader::single(Vec3::new(2.0, 2.0, 0.0), 0.5, false, vec![vec![Vec3::ZERO]], Some(text));
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(leader)).unwrap();
    d
}
fn source() -> Drawing {
    with_proxy(&drawing(), 160)
}
fn with_proxy(d: &Drawing, code: i32) -> Drawing {
    with_proxy_data(d, code, 4, "01020304")
}
fn with_proxy_data(d: &Drawing, code: i32, count: i64, hex: &str) -> Drawing {
    let bytes = cadcraft_io::write_with(d, "source.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let mut tags = cadcraft_dxf::parse(&bytes).unwrap();
    let start = tags.iter().position(|tag| tag.code == 0 && tag.str() == "MULTILEADER").unwrap();
    let anchor = if code == 92 { 8 } else { 270 };
    let at = start + tags.iter().skip(start).position(|tag| tag.code == anchor).unwrap() + 1;
    // Opaque synthetic bytes: the test checks lossless storage, not a graphics decoder.
    tags.splice(at..at, [Tag::i(code, count), Tag::new(310, Value::Hex(hex.into()))]);
    cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap()
}
#[test]
fn unchanged_proxy_bytes_survive_two_old_target_saves() {
    let mut d = source();
    let snapshot = d.model.iter().next().unwrap().common.geometry.as_ref().unwrap();
    assert!(snapshot.records.iter().flat_map(|record| &record.groups).all(|tag| tag.code != 310));
    for _ in 0..2 {
        let (bytes, notes) = cadcraft_io::write_with_noted(&d, "old.dxf", &options()).unwrap();
        let body = record(&bytes);
        assert_eq!(cache(&body), vec![(92, "4".into()), (310, "01020304".into())]);
        let layer = body.iter().position(|tag| tag.code == 8).unwrap();
        assert_eq!(body[layer + 1].code, 92);
        assert!(notes.iter().all(|note| !note.contains("proxy graphics")), "{notes:?}");
        d = cadcraft_io::read_dxf(&bytes).unwrap();
    }
}
#[test]
fn an_old_common_proxy_is_kept_and_a_modern_proxy_stays_in_its_own_subclass() {
    let old = with_proxy(&drawing(), 92);
    let bytes = cadcraft_io::write_with(&old, "old.dxf", &options()).unwrap();
    assert_eq!(cache(&record(&bytes)), vec![(92, "4".into()), (310, "01020304".into())]);
    let bytes = cadcraft_io::write_with(&source(), "new.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let body = record(&bytes);
    assert!(cache(&body).is_empty());
    let version = body.iter().position(|tag| tag.code == 270).unwrap();
    assert_eq!(body[version + 1].code, 160);
    assert_eq!(body[version + 2].str(), "01020304");
}
#[test]
fn a_block_definition_edit_invalidates_its_owners_proxy_cache() {
    use cadcraft_doc::{Block, Circle, Entity, Handle, Insert};
    let mut d = drawing();
    let mut block = Block::new("B");
    block.entities.push(Entity::new(Handle(0x500), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 })));
    d.blocks.insert("B".into(), std::sync::Arc::new(block));
    d.bump_handseed(Handle(0x501));
    let h = d.model.iter().next().unwrap().handle;
    d.model
        .modify(h, |entity| {
            if let EntityKind::MLeader(leader) = &mut entity.kind {
                leader.text = None;
                leader.block = Some(Insert {
                    block: "B".into(),
                    insert: Vec3::new(3.0, 2.0, 0.0),
                    scale: Vec3::new(1.0, 1.0, 1.0),
                    rotation: 0.0,
                    attribs: Vec::new(),
                    cols: 1,
                    rows: 1,
                    col_spacing: 0.0,
                    row_spacing: 0.0,
                });
            }
        })
        .unwrap();
    let mut d = with_proxy(&d, 160);
    let owner = d.entity(h).unwrap();
    let before = cadcraft_doc::geometry_key(&owner.kind, &owner.common);
    std::sync::Arc::make_mut(d.blocks.get_mut("B").unwrap())
        .entities
        .modify(Handle(0x500), |entity| {
            if let EntityKind::Circle(circle) = &mut entity.kind {
                circle.radius = 2.0;
            }
        })
        .unwrap();
    let owner = d.entity(h).unwrap();
    assert_eq!(cadcraft_doc::geometry_key(&owner.kind, &owner.common), before);
    let (bytes, notes) = cadcraft_io::write_with_noted(&d, "old.dxf", &options()).unwrap();
    assert!(cache(&record(&bytes)).is_empty());
    assert!(notes.iter().any(|note| note.contains(&h.hex()) && note.contains("proxy graphics")), "{notes:?}");
}
#[test]
fn binary_dxf_keeps_typed_proxy_chunks() {
    let bytes = cadcraft_io::write_with(&source(), "old.dxf", &SaveOptions { binary: true, ..options() }).unwrap();
    assert!(cadcraft_dxf::is_binary(&bytes));
    assert_eq!(cache(&record(&bytes)), vec![(92, "4".into()), (310, "01020304".into())]);
}

#[test]
fn edits_never_replay_stale_proxy_graphics() {
    for edit in 0..3 {
        let mut d = source();
        let h = d.model.iter().next().unwrap().handle;
        match edit {
            0 => {
                let moved = d.entity(h).unwrap().transformed(&Mat4::from_mat3(&Mat3::translate(Vec2::new(1.0, 0.0)))).unwrap();
                assert!(d.model.replace(moved));
            }
            1 => {
                d.model
                    .modify(h, |entity| {
                        if let EntityKind::MLeader(leader) = &mut entity.kind
                            && let Some(text) = &mut leader.text
                        {
                            text.contents = "changed".into();
                        }
                    })
                    .unwrap();
            }
            _ => {
                d.text_styles[0].font = "changed.shx".into();
            }
        }
        let (bytes, notes) = cadcraft_io::write_with_noted(&d, "old.dxf", &options()).unwrap();
        assert!(cache(&record(&bytes)).is_empty());
        assert!(notes.iter().any(|note| note.contains(&h.hex()) && note.contains("proxy graphics")), "{notes:?}");
        assert!(notes.iter().all(|note| !note.contains("block attribute(s) left out")), "{notes:?}");
    }
}
#[test]
fn an_unchecked_dependency_graph_retains_raw_cache_data_but_reports_no_replay() {
    use cadcraft_doc::{Block, Entity, Handle, Insert, MAX_BLOCK_DEPTH};
    let mut d = drawing();
    let h = d.model.iter().next().unwrap().handle;
    let reference = Insert {
        block: "B0".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    for depth in 0..=MAX_BLOCK_DEPTH + 1 {
        let name = format!("B{depth}");
        let mut block = Block::new(&name);
        if depth <= MAX_BLOCK_DEPTH {
            let mut next = reference.clone();
            next.block = format!("B{}", depth + 1);
            block.entities.push(Entity::new(Handle(0x600 + depth as u64), EntityKind::Insert(next)));
        }
        d.blocks.insert(name, std::sync::Arc::new(block));
    }
    d.bump_handseed(Handle(0x700));
    d.model
        .modify(h, |entity| {
            if let EntityKind::MLeader(leader) = &mut entity.kind {
                leader.text = None;
                leader.block = Some(reference);
            }
        })
        .unwrap();
    let d = with_proxy(&d, 160);
    let proxy = d.foreign.entities.get(&h).unwrap().proxy_graphics.as_ref().unwrap();
    assert!(proxy.display.is_none());
    assert!(proxy.tags.iter().any(|tag| tag.code == 310 && tag.value == "01020304"));
    let (bytes, notes) = cadcraft_io::write_with_noted(&d, "old.dxf", &options()).unwrap();
    assert!(cache(&record(&bytes)).is_empty());
    assert!(notes.iter().any(|note| note.contains(&h.hex()) && note.contains("proxy graphics")), "{notes:?}");
}

#[test]
fn an_untrusted_proxy_byte_count_is_not_emitted_to_the_dwg_decoder() {
    let d = with_proxy_data(&drawing(), 160, 1_000_000_000_000, "01020304");
    let h = d.model.iter().next().unwrap().handle;
    let proxy = d.foreign.entities.get(&h).unwrap().proxy_graphics.as_ref().unwrap();
    assert!(proxy.display.is_none());
    assert!(proxy.tags.iter().any(|tag| tag.code == 310));
    for name in ["old.dxf", "old.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&d, name, &options()).unwrap();
        let back = cadcraft_io::read(&bytes, name).unwrap();
        assert!(back.model.iter().any(|entity| matches!(entity.kind, EntityKind::MLeader(_))));
        assert!(back.foreign.entities.values().all(|extra| extra.proxy_graphics.is_none()));
        assert!(notes.iter().any(|note| note.contains(&h.hex()) && note.contains("proxy graphics")), "{notes:?}");
    }
}

#[test]
fn new_native_old_target_entities_report_the_absent_proxy_cache() {
    let d = source();
    let mut clean = Drawing::new_metric();
    let source = d.model.iter().next().unwrap();
    clean.add(&Space::Model, source.common.clone(), source.kind.clone()).unwrap();
    let h = clean.model.iter().next().unwrap().handle;
    for name in ["old.dxf", "old.dwg"] {
        let (bytes, notes) = cadcraft_io::write_with_noted(&clean, name, &options()).unwrap();
        let back = cadcraft_io::read(&bytes, name).unwrap();
        assert!(back.model.iter().any(|e| matches!(e.kind, EntityKind::MLeader(_))));
        assert!(notes.iter().any(|note| note.contains(&h.hex()) && note.contains("proxy graphics")), "{notes:?}");
    }
}

/// A source that has the graphics in its `AcDbEntity` subclass (AutoCAD 2010 files do, with group 160) gets them back there, not
/// inside `AcDbMLeader`.
#[test]
fn a_proxy_in_the_entity_subclass_stays_there_in_a_new_target() {
    let d = drawing();
    let bytes = cadcraft_io::write_with(&d, "source.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let mut tags = cadcraft_dxf::parse(&bytes).unwrap();
    let start = tags.iter().position(|tag| tag.code == 0 && tag.str() == "MULTILEADER").unwrap();
    let at = start + tags.iter().skip(start).position(|tag| tag.code == 8).unwrap() + 1;
    tags.splice(at..at, [Tag::i(160, 4), Tag::new(310, Value::Hex("01020304".into()))]);
    let source = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    let saved = cadcraft_io::write_with(&source, "new.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let body = record(&saved);
    assert_eq!(cache(&body), vec![(160, "4".into()), (310, "01020304".into())], "in AcDbEntity");
    let marker = body.iter().position(|tag| tag.code == 100 && tag.str() == "AcDbMLeader").unwrap();
    assert!(body[marker..].iter().all(|tag| !matches!(tag.code, 160 | 310)), "none inside AcDbMLeader");
}

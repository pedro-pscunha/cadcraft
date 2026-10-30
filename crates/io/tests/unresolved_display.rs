//! Visual exports must refuse every unresolved raw record in the selected display scope.
use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Layout, Line, RawTag, Space, Unknown, Viewport};
use cadcraft_geom::{Vec2, Vec3};
use std::sync::Arc;

fn raw(h: u64, point: bool) -> Entity {
    let tags = if point { vec![RawTag { code: 10, value: "2".into() }, RawTag { code: 20, value: "3".into() }] } else { Vec::new() };
    {
        use cadcraft_doc::{RawCompound, RawRecord};
        let mut tags = tags;
        tags.extend([RawTag { code: 5, value: format!("{h:X}") }, RawTag { code: 330, value: "1F".into() }]);
        Entity::from_raw(
            RawCompound::new(RawRecord::new("WIDGETX".into(), tags).unwrap(), Vec::new(), Common::default(), "No exact WIDGETX adapter.".into())
                .unwrap(),
        )
    }
}
fn drawing(canonical: bool) -> Drawing {
    let mut d = Drawing::new_metric();
    d.model.push(raw(0xA0, true));
    let mut block = Block::new("B");
    block.base = Vec3::new(1.0, 1.0, 0.0);
    block.entities.push(raw(0xA1, true));
    d.blocks.insert("B".into(), Arc::new(block));
    let h = d.new_handle();
    let mut insert = Entity::new(
        h,
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::new(10.0, 20.0, 0.0),
            scale: Vec3::new(2.0, 3.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    );
    if canonical {
        insert.geometry_revision = GeometryRevision::CanonicalV2;
    }
    d.model.push(insert);
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })).unwrap();
    d
}
fn compound(family: &str) -> Entity {
    use cadcraft_doc::{RawCompound, RawRecord};
    let tag = |code, value: &str| RawTag { code, value: value.into() };
    let mut parent = vec![tag(5, "B0"), tag(330, "1F"), tag(10, "0"), tag(20, "0"), tag(30, "5")];
    let follower = if family == "INSERT" {
        parent.extend([tag(2, "UnresolvedPart"), tag(66, "1")]);
        "ATTRIB"
    } else {
        parent.push(tag(70, "4"));
        "VERTEX"
    };
    let child = RawRecord::new(follower.into(), vec![tag(5, "B1"), tag(330, "B0"), tag(10, "2"), tag(20, "3"), tag(30, "5")]).unwrap();
    let end = RawRecord::new("SEQEND".into(), vec![tag(5, "B2"), tag(330, "B0")]).unwrap();
    Entity::from_raw(
        RawCompound::new(
            RawRecord::new(family.into(), parent).unwrap(),
            vec![child, end],
            Common::default(),
            format!("No exact {family} display adapter."),
        )
        .unwrap(),
    )
}
#[test]
fn a_mixed_raw_and_unknown_drawing_refuses_only_the_raw_handle() {
    let mut d = Drawing::new_metric();
    d.model.push(compound("INSERT"));
    d.model.push(Entity::new(Handle(0xA0), EntityKind::Unknown(Unknown { dxf_type: "REGION".into(), tags: Vec::new() })));
    for name in ["mixed.png", "mixed.pdf", "mixed.svg"] {
        let message = cadcraft_io::write_framed(&d, name, None).unwrap_err().to_string();
        assert!(message.contains("B0"), "{name}: {message}");
        assert!(!message.contains("A0"), "{name}: Unknown must retain warning-only behavior: {message}");
    }
}

#[test]
fn complete_raw_compounds_are_unresolved_in_model_blocks_and_paper_viewports() {
    for family in ["INSERT", "POLYLINE"] {
        for scope in 0..5 {
            let mut d = drawing(matches!(scope, 2 | 4));
            d.model.remove(Handle(0xA0));
            let block = Arc::make_mut(d.blocks.get_mut("B").unwrap());
            block.entities.remove(Handle(0xA1));
            let unit = compound(family);
            assert_eq!(unit.geometry_revision, GeometryRevision::RawSource);
            let reason = unit.unresolved_display().unwrap().reason.to_owned();
            if matches!(scope, 1 | 2 | 4) {
                Arc::make_mut(d.blocks.get_mut("B").unwrap()).entities.push(unit);
            } else {
                d.model.push(unit);
            }
            let space = if scope >= 3 {
                d.layouts.push(Layout::new("Sheet", 1));
                d.add(
                    &Space::Paper("Sheet".into()),
                    Common::default(),
                    EntityKind::Viewport(Viewport {
                        center: Vec3::new(100.0, 100.0, 0.0),
                        width: 100.0,
                        height: 100.0,
                        view_center: Vec2::ZERO,
                        view_height: 50.0,
                        id: 2,
                        ..Viewport::default()
                    }),
                )
                .unwrap();
                Space::Paper("Sheet".into())
            } else {
                Space::Model
            };
            let list = cadcraft_render::build(&d, &space, &Default::default());
            let diagnostic = list.unresolved.iter().find(|record| record.handle == Handle(0xB0)).expect("raw unit diagnostic");
            assert_eq!(diagnostic.reason, reason, "{family} scope {scope}");
            let results = [
                cadcraft_io::png(&d, &space, 64, 64).map(|_| ()),
                cadcraft_io::svg::export(&d, &space).map(|_| ()),
                cadcraft_io::pdf::pdf(&d, &space, &cadcraft_io::pdf::PdfOptions::default()).map(|_| ()),
            ];
            for result in results {
                assert!(result.is_err(), "{family} scope {scope}");
                let message = result.unwrap_err().to_string();
                assert!(message.contains("B0") && message.contains("unresolved"), "{family} scope {scope}: {message}");
            }
        }
    }
}

#[test]
fn placeholders_keep_position_hints_but_never_extend_geometric_bounds() {
    for canonical in [false, true] {
        let d = drawing(canonical);
        let list = cadcraft_render::build(&d, &Space::Model, &Default::default());
        assert_eq!(list.unresolved.len(), 2);
        let top = list.unresolved.iter().find(|record| record.handle == Handle(0xA0)).unwrap();
        assert_eq!(top.location, Vec2::new(2.0, 3.0));
        let child = list.unresolved.iter().find(|record| record.handle == Handle(0xA1)).unwrap();
        assert_eq!(child.location, Vec2::new(12.0, 26.0));
        assert_eq!(list.bounds.min, Vec2::ZERO);
        assert_eq!(list.bounds.max, Vec2::new(1.0, 1.0));
        assert!(list.unresolved.iter().all(|record| !record.reason.is_empty()));
    }
}
#[test]
fn a_raw_record_without_a_point_uses_its_owners_origin() {
    let mut d = drawing(false);
    assert!(d.model.replace(raw(0xA0, false)));
    assert!(Arc::make_mut(d.blocks.get_mut("B").unwrap()).entities.replace(raw(0xA1, false)));
    let list = cadcraft_render::build(&d, &Space::Model, &Default::default());
    assert_eq!(list.unresolved.iter().find(|record| record.handle == Handle(0xA0)).unwrap().location, Vec2::ZERO);
    assert_eq!(list.unresolved.iter().find(|record| record.handle == Handle(0xA1)).unwrap().location, Vec2::new(8.0, 17.0));
}
#[test]
fn a_raw_child_inside_a_tilted_canonical_insert_uses_the_spatial_parent() {
    let mut d = drawing(true);
    let h = d.model.iter().find(|entity| matches!(entity.kind, EntityKind::Insert(_))).unwrap().handle;
    let mut entity = (**d.entity(h).unwrap()).clone();
    entity.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
    if let EntityKind::Insert(insert) = &mut entity.kind {
        insert.insert.z = 30.0;
    }
    assert!(d.model.replace(entity));
    let list = cadcraft_render::build(&d, &Space::Model, &Default::default());
    let child = list.unresolved.iter().find(|record| record.handle == Handle(0xA1)).unwrap();
    assert!((child.location - Vec2::new(-12.0, -2.8)).len() < 1e-9, "{:?}", child.location);
}
#[test]
fn raw_only_drawings_and_explicit_windows_cannot_bypass_export_refusal() {
    let mut d = Drawing::new_metric();
    d.model.push(raw(0xA2, true));
    for name in ["raw.png", "raw.pdf", "raw.svg"] {
        for window in [None, Some(cadcraft_geom::Bounds2::new(Vec2::ZERO, Vec2::new(1.0, 1.0)))] {
            let result = cadcraft_io::write_framed(&d, name, window);
            assert!(result.is_err(), "{name}: unresolved-only output was allowed");
            let message = result.unwrap_err().to_string();
            assert!(message.contains("A2"), "{message}");
        }
    }
}
#[test]
fn legacy_unknown_records_keep_warning_only_behavior_without_a_second_marker() {
    let raw: Unknown = serde_json::from_value(serde_json::json!({"dxf_type":"WIDGETX","tags":[]})).unwrap();
    let saved = serde_json::to_value(&raw).unwrap();
    assert!(saved.get("display").is_none(), "Unknown must not own a second display marker");
    let entity = Entity::new(Handle(0xA0), EntityKind::Unknown(raw));
    assert!(entity.unresolved_display().is_none());
}
#[test]
fn model_and_block_raw_records_refuse_png_pdf_and_svg_with_all_handles() {
    for canonical in [false, true] {
        let d = drawing(canonical);
        for name in ["raw.png", "raw.pdf", "raw.svg"] {
            let result = cadcraft_io::write_framed(&d, name, None);
            assert!(result.is_err(), "{name}: raw records were silently omitted");
            let message = result.unwrap_err().to_string();
            for h in [Handle(0xA0), Handle(0xA1)] {
                assert!(message.contains(&h.hex()), "{name}: {message}");
            }
            assert!(message.contains("unresolved"), "{message}");
        }
    }
}
#[test]
fn a_paper_viewport_refuses_each_export_when_its_model_has_raw_records() {
    let mut d = drawing(false);
    d.layouts.push(Layout::new("Sheet", 1));
    d.add(
        &Space::Paper("Sheet".into()),
        Common::default(),
        EntityKind::Viewport(Viewport {
            center: Vec3::new(100.0, 100.0, 0.0),
            width: 100.0,
            height: 100.0,
            view_center: Vec2::ZERO,
            view_height: 50.0,
            id: 2,
            ..Viewport::default()
        }),
    )
    .unwrap();
    let space = Space::Paper("Sheet".into());
    let results = [
        cadcraft_io::png(&d, &space, 64, 64).map(|_| ()),
        cadcraft_io::svg::export(&d, &space).map(|_| ()),
        cadcraft_io::pdf::pdf(&d, &space, &cadcraft_io::pdf::PdfOptions::default()).map(|_| ()),
    ];
    for result in results {
        assert!(result.is_err(), "paper export omitted unresolved records");
        let message = result.unwrap_err().to_string();
        for h in [Handle(0xA0), Handle(0xA1)] {
            assert!(message.contains(&h.hex()), "{message}");
        }
    }
}

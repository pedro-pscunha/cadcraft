//! Draw-order keys refer to actual top-level records, including lowered and split entities.

use std::collections::{HashMap, HashSet};

use cadcraft_doc::{Common, Drawing, EntityKind, Handle, HatchLoop, Line, MLeader, MText, Region, Space};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};

fn write(drawing: &Drawing) -> Vec<(String, Vec<Tag>)> {
    let options = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2000), binary: false };
    let bytes = cadcraft_io::write_with(drawing, "lowered.dxf", &options).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    sections(&tags).into_iter().filter(|s| matches!(s.name.as_str(), "ENTITIES" | "OBJECTS")).flat_map(|s| records(&s.tags)).collect()
}

fn handle(body: &[Tag]) -> Handle {
    Handle::parse_hex(&body.iter().find(|t| t.code == 5).unwrap().str()).unwrap()
}

fn assert_flattened_order(records: &[(String, Vec<Tag>)], cover: Handle, expected_kinds: &[&str]) {
    let entities: Vec<_> = records.iter().filter(|(k, _)| expected_kinds.contains(&k.as_str()) || k == "LINE").collect();
    let expected: Vec<_> = entities.iter().map(|(_, t)| handle(t)).collect();
    assert_eq!(expected.last(), Some(&cover));
    assert_eq!(entities.iter().take(expected_kinds.len()).map(|(k, _)| k.as_str()).collect::<Vec<_>>(), expected_kinds);
    let live: HashSet<_> = expected.iter().copied().collect();
    let table = &records.iter().find(|(k, _)| k == "SORTENTSTABLE").expect("expanded record order needs a table").1;
    let mut keys = HashMap::new();
    for pair in table.windows(2).filter(|p| p[0].code == 331 && p[1].code == 5) {
        let entity = Handle::parse_hex(&pair[0].str()).unwrap();
        assert!(live.contains(&entity), "dangling 331 entity handle {entity:?}");
        keys.insert(entity, Handle::parse_hex(&pair[1].str()).unwrap());
    }
    assert_eq!(keys.len(), expected.len(), "every emitted part needs its own key");
    let mut actual = expected.clone();
    actual.sort_by_key(|h| keys.get(h).copied().unwrap_or(*h));
    assert_eq!(actual, expected, "all parts must stay beneath the covering entity");
    let mut sort_keys: Vec<_> = keys.values().copied().collect();
    sort_keys.sort_unstable();
    let mut entity_handles = expected;
    entity_handles.sort_unstable();
    assert_eq!(sort_keys, entity_handles, "sort keys are a permutation of emitted handles");
}

#[test]
fn legacy_multileader_parts_sent_to_back_all_sort_below_the_cover() {
    let mut drawing = Drawing::new_metric();
    let cover = drawing.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    let leader = drawing
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::MLeader(MLeader {
                roots: vec![cadcraft_doc::MLeaderRoot::new(Vec3::new(2.0, 1.0, 0.0), 0.5, false, vec![vec![Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0)]])],
                arrow_size: 0.2,
                spline: false,
                block: None,
                style: "Standard".into(),
                text: Some(MText {
                    insert: Vec3::new(2.5, 1.0, 0.0),
                    height: 0.3,
                    width: 3.0,
                    attach: 1,
                    rotation: 0.0,
                    style: "Standard".into(),
                    contents: "below".into(),
                    line_spacing: 1.0,
                    line_spacing_exact: false,
                    columns: None,
                    direction: None,
                    drawing_direction: None,
                }),
                arrow_block: String::new(),
                context: cadcraft_doc::MLeaderContext::default(),
            }),
        )
        .unwrap();
    drawing.model.send_to_back(leader);
    assert_flattened_order(&write(&drawing), cover, &["LEADER", "MTEXT"]);
}

#[test]
fn every_region_loop_sorts_below_the_cover_even_if_source_handles_are_ascending() {
    let mut drawing = Drawing::new_metric();
    let square = |x: f64| HatchLoop {
        vertices: vec![
            PolyVertex::new(Vec2::new(x, 0.0)),
            PolyVertex::new(Vec2::new(x + 1.0, 0.0)),
            PolyVertex::new(Vec2::new(x + 1.0, 1.0)),
            PolyVertex::new(Vec2::new(x, 1.0)),
        ],
        outer: true,
        sources: vec![],
        ..Default::default()
    };
    drawing.add(&Space::Model, Common::default(), EntityKind::Region(Region { loops: vec![square(0.0), square(2.0)] })).unwrap();
    let cover = drawing.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    assert_flattened_order(&write(&drawing), cover, &["LWPOLYLINE", "LWPOLYLINE"]);
}

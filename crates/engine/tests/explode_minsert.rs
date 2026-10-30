//! EXPLODE refuses a MINSERT (a block reference with rows or columns), as AutoCAD does ("the object is minserted"), before it writes
//! anything: a plain reference still explodes.

use std::sync::Arc;

use cadcraft_engine::Session;
use cadcraft_engine::doc::geom::Vec3;
use cadcraft_engine::doc::{Block, Circle, Common, Entity, EntityKind, Handle, Insert, Space};
use serde_json::json;

fn session_with_bolt() -> Session {
    let mut s = Session::new();
    let d = s.doc_mut().unwrap();
    let mut block = Block::new("BOLT");
    let h = d.new_handle();
    block.entities.push(Entity::new(h, EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 })));
    d.blocks.insert("BOLT".into(), Arc::new(block));
    s
}

fn put_insert(s: &mut Session, x: f64, cols: u32, rows: u32) -> Handle {
    let kind = EntityKind::Insert(Insert {
        block: "BOLT".into(),
        insert: Vec3::new(x, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols,
        rows,
        col_spacing: 5.0,
        row_spacing: 5.0,
    });
    s.doc_mut().unwrap().add(&Space::Model, Common::default(), kind).unwrap()
}

#[test]
fn a_plain_reference_explodes() {
    let mut s = session_with_bolt();
    let h = put_insert(&mut s, 0.0, 1, 1);
    s.execute("explode", &json!({"handles": [h.hex()]})).unwrap();
    assert!(s.doc().unwrap().entity(h).is_none());
    assert!(s.doc().unwrap().model.iter().all(|e| matches!(e.kind, EntityKind::Circle(_))));
}

#[test]
fn a_minsert_is_refused_whatever_its_shape_and_nothing_changes() {
    for (cols, rows) in [(3, 2), (2, 1), (1, 4)] {
        let mut s = session_with_bolt();
        let h = put_insert(&mut s, 0.0, cols, rows);
        let before = s.doc().unwrap().model.clone();
        let seed = s.doc().unwrap().handseed;
        let error = s.execute("explode", &json!({"handles": [h.hex()]})).expect_err("a MINSERT was exploded");
        assert!(error.to_string().contains("MINSERT"), "{error}");
        assert_eq!(s.doc().unwrap().model, before, "{cols} x {rows}");
        assert_eq!(s.doc().unwrap().handseed, seed);
    }
}

#[test]
fn a_selection_with_a_minsert_is_refused_whole_in_either_order() {
    for minsert_first in [true, false] {
        let mut s = session_with_bolt();
        let (plain, grid) = (put_insert(&mut s, 0.0, 1, 1), put_insert(&mut s, 20.0, 3, 2));
        let handles = if minsert_first { [grid, plain] } else { [plain, grid] };
        let before = s.doc().unwrap().model.clone();
        let params = json!({"handles": [handles[0].hex(), handles[1].hex()]});
        assert!(s.execute("explode", &params).is_err());
        assert_eq!(s.doc().unwrap().model, before, "minsert first: {minsert_first}");
    }
}

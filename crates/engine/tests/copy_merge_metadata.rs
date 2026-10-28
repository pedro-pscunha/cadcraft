//! Copies preserve occurrence-local references; joins remap every consumed identity.

use std::sync::Arc;

use cadcraft_engine::doc::{Block, Drawing, Entity, EntityKind, Handle, RawTag};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::{Value, json};

fn handle(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

fn probe(s: &mut Session, h: Handle, role: &str, target: Handle) {
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            e.common.xdata = vec![
                RawTag { code: 1001, value: "COPY_PROBE".into() },
                RawTag { code: 1000, value: role.into() },
                RawTag { code: 1005, value: target.hex() },
            ];
        })
        .unwrap();
}

fn line(s: &mut Session, points: [[f64; 2]; 2]) -> Handle {
    handle(&s.execute("line", &json!({"points":points})).unwrap()["handles"][0])
}

/// Relocate, rather than duplicate, model objects into a definition so setup cannot fix the
/// references that NCOPY itself must remap. All original handles and owned metadata stay intact.
fn enclose(s: &mut Session, name: &str) -> Handle {
    let d = s.doc_mut().unwrap();
    let mut block = Block::new(name);
    block.entities = std::mem::take(&mut d.model);
    d.blocks.insert(name.into(), Arc::new(block));
    handle(&s.execute("insert", &json!({"name":name, "at":[0,0]})).unwrap()["handle"])
}

fn repeated_occurrences() -> (Session, Handle) {
    let mut s = Session::new();
    let a = line(&mut s, [[0.0, 0.0], [1.0, 0.0]]);
    let b = line(&mut s, [[1.0, 0.0], [2.0, 0.0]]);
    probe(&mut s, a, "A", b);
    probe(&mut s, b, "B", b);
    enclose(&mut s, "SOURCE");
    // The SOURCE INSERT itself has the same handle in both WRAPPER occurrences: grouping by
    // only its definition or immediate parent handle is not enough; the entire path matters.
    enclose(&mut s, "WRAPPER");
    s.execute("insert", &json!({"name":"WRAPPER", "at":[20,0]})).unwrap();
    let root = enclose(&mut s, "ROOT");
    (s, root)
}

fn role(e: &Entity, name: &str) -> bool {
    e.common.xdata.iter().any(|t| t.code == 1000 && t.value == name)
}

fn assert_occurrence_references(d: &Drawing) {
    let copies: Vec<_> = d.model.iter().filter(|e| role(e, "A") || role(e, "B")).collect();
    assert_eq!(copies.len(), 4, "both leaves of both occurrences must be copied");
    for a in copies.iter().filter(|e| role(e, "A")) {
        let EntityKind::Line(a_line) = &a.kind else { panic!("line A") };
        let b = copies.iter().find(|e| role(e, "B") && matches!(&e.kind, EntityKind::Line(b_line) if b_line.a == a_line.b)).unwrap();
        assert!(
            a.common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(b.handle)),
            "NCOPY sibling reference must follow its own occurrence's B copy"
        );
        assert!(
            b.common.xdata.iter().any(|t| t.code == 1005 && Handle::parse_hex(&t.value) == Some(b.handle)),
            "NCOPY B self-reference must follow its own copy"
        );
    }
}

#[test]
fn ncopy_remaps_sibling_references_separately_for_repeated_occurrences() {
    let (mut s, root) = repeated_occurrences();
    let r = s.execute("ncopy", &json!({"handle":root.hex(), "delta":[0,10]})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 4);
    assert_occurrence_references(s.doc().unwrap());
    for extension in ["dxf", "dwg"] {
        let name = format!("ncopy.{extension}");
        let bytes = cadcraft_io::write(s.doc().unwrap(), &name).unwrap();
        assert_occurrence_references(&cadcraft_io::read(&bytes, &name).unwrap());
    }
}

#[test]
fn interactive_ncopy_groups_picked_siblings_by_occurrence() {
    let (mut s, _) = repeated_occurrences();
    s.start("ncopy").unwrap();
    for x in [0.5, 1.5, 20.5, 21.5] {
        s.input(Input::Point(Vec2::new(x, 0.0))).unwrap();
    }
    s.input(Input::Enter).unwrap();
    s.input(Input::Point(Vec2::ZERO)).unwrap();
    s.input(Input::Point(Vec2::new(0.0, 10.0))).unwrap();
    assert!(s.running.is_none());
    assert_occurrence_references(s.doc().unwrap());
}

#[test]
fn ncopy_preserves_raw_association_when_geometry_is_another_leaf() {
    let mut s = Session::new();
    let geometry = line(&mut s, [[0.0, 0.0], [10.0, 0.0]]);
    let dimension = handle(&s.execute("dimlinear", &json!({"object":geometry.hex(), "at":[5,5]})).unwrap()["handle"]);
    s.execute("dimreassociate", &json!({"handles":[dimension.hex()]})).unwrap();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    let mut own = false;
    let mut association = false;
    tags.retain_mut(|t| {
        if t.code == 0 {
            own = false;
            association = t.str() == "DIMASSOC";
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        if association && t.code == 72 {
            *t = cadcraft_dxf::Tag::i(72, 8);
        }
        !own
    });
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(dimension), "fixture must have an opaque raw association");
    s.open_drawing(d, "raw.dxf", None);
    let root = enclose(&mut s, "ASSOCIATED");
    let r = s.execute("ncopy", &json!({"handle":root.hex()})).unwrap();
    let d = s.doc().unwrap();
    let copies: Vec<_> = r["handles"].as_array().unwrap().iter().map(handle).collect();
    let copied_dimension = copies.iter().copied().find(|h| matches!(d.entity(*h).unwrap().kind, EntityKind::Dimension(_))).unwrap();
    let copied_geometry = copies.iter().copied().find(|h| matches!(d.entity(*h).unwrap().kind, EntityKind::Line(_))).unwrap();
    assert!(d.has_raw_dimassoc(copied_dimension), "NCOPY must retain raw association when all of its geometry is copied");
    let raw = d
        .foreign
        .objects
        .iter()
        .find(|o| {
            o.kind == "DIMASSOC"
                && cadcraft_engine::doc::foreign::dimassoc_reference(o.tags.iter().map(|t| (t.code, t.value.as_str())))
                    .is_some_and(|(_, h)| h == copied_dimension)
        })
        .unwrap();
    assert!(
        raw.tags.iter().filter(|t| t.code == 331).all(|t| Handle::parse_hex(&t.value) == Some(copied_geometry)),
        "raw references must name the copied geometry"
    );
    for extension in ["dxf", "dwg"] {
        let name = format!("ncopy-raw.{extension}");
        let bytes = cadcraft_io::write(d, &name).unwrap();
        let saved = cadcraft_io::read(&bytes, &name).unwrap();
        assert!(saved.has_raw_dimassoc(copied_dimension), "{extension}: copied raw association must survive a save");
        assert!(
            saved
                .foreign
                .objects
                .iter()
                .filter(|o| o.kind == "DIMASSOC")
                .flat_map(|o| &o.tags)
                .any(|t| t.code == 331 && Handle::parse_hex(&t.value) == Some(copied_geometry)),
            "{extension}: copied raw geometry reference was lost"
        );
    }
}

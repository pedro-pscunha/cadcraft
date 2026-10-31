//! Angular and arc length dimensions whose text fits between the extension lines, as AutoCAD LT 2025 makes them (ISO-25, DIMASSOC 0 pieces,
//! `/tmp/cad/elev-oracle15`): the entity counts of the oracle and the positions that do not depend on the width of the text.
use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_engine::Session;
use serde_json::json;
use std::collections::BTreeMap;

fn counts(s: &Session, before: usize) -> BTreeMap<&'static str, usize> {
    let mut seen = BTreeMap::new();
    for e in s.doc().unwrap().model.iter().skip(before) {
        let name = match &e.kind {
            EntityKind::Line(_) => "LINE",
            EntityKind::Arc(_) => "ARC",
            EntityKind::Solid(_) => "SOLID",
            EntityKind::MText(_) => "MTEXT",
            other => panic!("made {other:?}"),
        };
        *seen.entry(name).or_default() += 1;
    }
    seen
}

fn metric() -> Session {
    let mut s = Session::new();
    s.open_drawing(Drawing::new_metric(), "metric", None);
    s.execute("setvar", &json!({"name": "DIMASSOC", "value": 0})).unwrap();
    s
}

fn lines_at(s: &mut Session, radius: f64) -> usize {
    let a = 60f64.to_radians();
    s.execute("line", &json!({"points": [[0, 0], [60, 0]]})).unwrap();
    let l1 = s.doc().unwrap().model.iter().last().unwrap().handle;
    s.execute("line", &json!({"points": [[0, 0], [60.0 * a.cos(), 60.0 * a.sin()]]})).unwrap();
    let l2 = s.doc().unwrap().model.iter().last().unwrap().handle;
    let before = s.doc().unwrap().model.len();
    let m = 30f64.to_radians();
    s.execute("dimangular", &json!({"lines": [l1.hex(), l2.hex()], "at": [radius * m.cos(), radius * m.sin()]})).unwrap();
    before
}

#[test]
fn two_lines_with_the_arrowheads_outside_have_the_whole_arc_and_two_stubs() {
    // Chord 10 holds the text "60" and a gap on each side but not two arrowheads as well: arrowheads outside, the arc whole and a stub of an
    // arrowhead past each (the oracle: arcs 331.277..345.638, 74.362..88.723 and 0..60 at radius 10).
    let mut s = metric();
    let before = lines_at(&mut s, 10.0);
    assert_eq!(counts(&s, before), BTreeMap::from([("ARC", 3), ("SOLID", 2), ("MTEXT", 1)]));
    let arcs: Vec<(f64, f64)> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .skip(before)
        .filter_map(|e| {
            if let EntityKind::Arc(a) = &e.kind { Some((a.start.to_degrees().rem_euclid(360.0), a.end.to_degrees().rem_euclid(360.0))) } else { None }
        })
        .collect();
    let has = |lo: f64, hi: f64| arcs.iter().any(|(a, b)| (a - lo).abs() < 1e-3 && (b - hi).abs() < 1e-3);
    assert!(has(0.0, 60.0) && has(331.277, 345.638) && has(74.362, 88.723), "{arcs:?}");
}

#[test]
fn two_lines_with_everything_between_the_extension_lines_have_the_arc_between_the_arrowheads() {
    for radius in [20.0, 40.0] {
        let mut s = metric();
        let before = lines_at(&mut s, radius);
        assert_eq!(counts(&s, before), BTreeMap::from([("ARC", 1), ("SOLID", 2), ("MTEXT", 1)]), "radius {radius}");
    }
}

#[test]
fn a_vertex_and_two_points_add_the_extension_lines_to_the_inside_layout() {
    let mut s = metric();
    let before = s.doc().unwrap().model.len();
    let (m, p) = (35f64.to_radians(), 70f64.to_radians());
    s.execute(
        "dimangular",
        &json!({"vertex": [0, 0], "p1": [60, 0], "p2": [60.0 * p.cos(), 60.0 * p.sin()], "at": [20.0 * m.cos(), 20.0 * m.sin()]}),
    )
    .unwrap();
    assert_eq!(counts(&s, before), BTreeMap::from([("LINE", 2), ("ARC", 1), ("SOLID", 2), ("MTEXT", 1)]));
}

#[test]
fn an_arc_length_with_its_text_inside_has_the_arc_symbol_as_a_semicircle() {
    for (r, outer) in [(10.0, 12.0), (10.0, 20.0), (30.0, 32.0)] {
        let mut s = metric();
        s.execute("arc", &json!({"p1": [r, 0], "p2": [0, r], "p3": [-r, 0]})).unwrap();
        let arc = s.doc().unwrap().model.iter().last().unwrap().handle;
        let before = s.doc().unwrap().model.len();
        s.execute("dimarc", &json!({"handle": arc.hex(), "at": [0, outer]})).unwrap();
        // The extension lines, the arc between the arrowheads and the symbol, the arrowheads and the text without the symbol character.
        assert_eq!(counts(&s, before), BTreeMap::from([("LINE", 2), ("ARC", 2), ("SOLID", 2), ("MTEXT", 1)]), "radius {r} at {outer}");
        let text =
            s.doc().unwrap().model.iter().skip(before).find_map(|e| if let EntityKind::MText(t) = &e.kind { Some(t.contents.clone()) } else { None });
        assert!(!text.unwrap().contains('⌒'));
    }
}

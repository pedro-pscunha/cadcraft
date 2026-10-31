//! DIMBASELINE and DIMCONTINUE at DIMASSOC 0 add the pieces of each dimension, not a DIMENSION, and the chain keeps its base: the next piece
//! group starts from the dimension made with DIMASSOC 2, not from the one before (AutoCAD LT 2025, accoreconsole,
//! `/tmp/cad/elev-oracle17`: one base DIMENSION from (0,10) to (40,10) with the line at y 20, then two points 70 and 100).

use cadcraft_doc::{Drawing, EntityKind, Handle};
use cadcraft_engine::Session;
use serde_json::json;

fn chain(command: &str) -> Session {
    let mut s = Session::new();
    s.open_drawing(Drawing::new_metric(), "metric", None);
    s.execute("dimlinear", &json!({"p1": [0, 10], "p2": [40, 10], "at": [20, 20]})).unwrap();
    s.execute("setvar", &json!({"name": "DIMASSOC", "value": 0})).unwrap();
    s.execute(command, &json!({"points": [[70, 10], [100, 10]]})).unwrap();
    s
}

fn count(s: &Session, name: &str) -> usize {
    s.doc().unwrap().model.iter().filter(|e| e.kind.dxf_name() == name).count()
}

/// The x of the ends of the extension lines (vertical LINEs) and the y of the dimension line (the horizontal LINEs), in drawing order.
fn lines(s: &Session) -> (Vec<(f64, f64)>, Vec<f64>) {
    let (mut verticals, mut horizontals) = (Vec::new(), Vec::new());
    for e in s.doc().unwrap().model.iter() {
        if let EntityKind::Line(l) = &e.kind {
            if (l.a.x - l.b.x).abs() < 1e-6 {
                verticals.push((l.a.x, l.b.x));
            } else {
                horizontals.push(l.a.y);
            }
        }
    }
    (verticals, horizontals)
}

#[test]
fn dimcontinue_without_association_adds_pieces_and_keeps_the_first_base() {
    let s = chain("dimcontinue");
    assert_eq!((count(&s, "DIMENSION"), count(&s, "LINE"), count(&s, "SOLID"), count(&s, "MTEXT")), (1, 6, 4, 2));
    let (verticals, _) = lines(&s);
    let xs: Vec<f64> = verticals.iter().map(|v| v.0).collect();
    assert_eq!(xs, [40.0, 70.0, 40.0, 100.0], "both pieces start at the base dimension's second point");
    assert_eq!(
        s.doc().unwrap().entity(Handle(s.last_dim.unwrap().0)).map(|e| e.kind.dxf_name()),
        Some("DIMENSION"),
        "the base stays the dimension to continue"
    );
}

#[test]
fn dimbaseline_without_association_adds_pieces_and_does_not_stack() {
    let s = chain("dimbaseline");
    assert_eq!((count(&s, "DIMENSION"), count(&s, "LINE"), count(&s, "SOLID"), count(&s, "MTEXT")), (1, 6, 4, 2));
    let (verticals, horizontals) = lines(&s);
    let xs: Vec<f64> = verticals.iter().map(|v| v.0).collect();
    assert_eq!(xs, [0.0, 70.0, 0.0, 100.0]);
    assert!((horizontals[0] - horizontals[1]).abs() < 1e-9, "the second dimension line is at the same offset: {horizontals:?}");
}

#[test]
fn qdim_without_association_still_adds_dimensions() {
    // AutoCAD LT 2025 (`/tmp/cad/elev-oracle17`, DIMASSOC 0 and 2 give the same): three lines, QDIM Continuous, two DIMENSION entities and no pieces.
    let mut s = Session::new();
    s.open_drawing(Drawing::new_metric(), "metric", None);
    let mut handles = Vec::new();
    for x in [0, 30, 70] {
        s.execute("line", &json!({"points": [[x, 0], [x, 10]]})).unwrap();
        handles.push(s.doc().unwrap().model.iter().last().unwrap().handle.hex());
    }
    s.execute("setvar", &json!({"name": "DIMASSOC", "value": 0})).unwrap();
    s.execute("qdim", &json!({"handles": handles, "at": [35, 25], "mode": "continuous"})).unwrap();
    assert_eq!((count(&s, "DIMENSION"), count(&s, "SOLID"), count(&s, "MTEXT")), (2, 0, 0));
}

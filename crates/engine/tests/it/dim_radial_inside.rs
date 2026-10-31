//! A radius or diameter made with its location inside the circle, as AutoCAD LT 2025 makes it (ISO-25, `/tmp/cad/elev-oracle11`, circles of
//! radius 4 to 15): at DIMASSOC 0 one LINE (the leader, from the base of the arrowhead inward), one SOLID inside the circle and one MTEXT.
use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_engine::Session;
use serde_json::json;

#[test]
fn radius_and_diameter_with_the_location_inside_the_circle_make_one_line_one_arrowhead_and_the_text() {
    for command in ["dimradius", "dimdiameter"] {
        let mut s = Session::new();
        s.open_drawing(Drawing::new_metric(), "metric", None);
        s.execute("circle", &json!({"center": [0, 0], "radius": 8})).unwrap();
        let circle = s.doc().unwrap().model.iter().last().unwrap().handle;
        s.execute("setvar", &json!({"name": "DIMASSOC", "value": 0})).unwrap();
        let before = s.doc().unwrap().model.len();
        s.execute(command, &json!({"handle": circle.hex(), "at": [2.8, 2.8]})).unwrap();
        let made: Vec<_> = s.doc().unwrap().model.iter().skip(before).collect();
        let lines = made.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count();
        let solids: Vec<_> = made.iter().filter_map(|e| if let EntityKind::Solid(c) = &e.kind { Some(c) } else { None }).collect();
        let texts = made.iter().filter(|e| matches!(e.kind, EntityKind::MText(_))).count();
        assert_eq!((lines, solids.len(), texts), (1, 1, 1), "{command}: {made:?}");
        // The base of the arrowhead (the middle of its two other corners) is inside the circle.
        let c = solids[0].corners;
        let base = ((c[1].x + c[2].x) / 2.0, (c[1].y + c[2].y) / 2.0);
        assert!(base.0.hypot(base.1) < 8.0, "{command}: the arrowhead points out of the circle: {c:?}");
    }
}

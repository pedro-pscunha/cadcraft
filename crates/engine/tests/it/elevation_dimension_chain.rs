//! A dimension that continues or sets a baseline from the last linear dimension is at the elevation of the dimension and of the picks.
use cadcraft_doc::{DimKind, EntityKind};
use cadcraft_engine::Session;
use serde_json::json;

fn chained(command: &str) -> Vec<f64> {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 3})).unwrap();
    s.execute("dimlinear", &json!({"p1": [0, 40], "p2": [10, 40], "at": [5, 43]})).unwrap();
    let before = s.doc().unwrap().model.len();
    s.execute(command, &json!({"points": [[15, 40]]})).unwrap();
    let made: Vec<_> = s.doc().unwrap().model.iter().skip(before).collect();
    assert_eq!(made.len(), 1, "{command} made {} objects", made.len());
    let EntityKind::Dimension(dm) = &made[0].kind else { panic!("{command} made {:?}", made[0].kind) };
    assert!(matches!(dm.kind, DimKind::Linear { .. }));
    vec![dm.defpt.z, dm.text_mid.z, dm.p13.z, dm.p14.z]
}

#[test]
fn dimcontinue_and_dimbaseline_draw_at_the_elevation() {
    for command in ["dimcontinue", "dimbaseline"] {
        assert_eq!(chained(command), vec![3.0; 4], "{command}");
    }
}

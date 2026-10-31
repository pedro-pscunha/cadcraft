//! The interactive object-made dimensions take the object's Z, whatever DIMASSOC is: the object at Z 7, ELEVATION 3.
use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use serde_json::json;

/// (command line steps, the Z of the definition point that comes from the object, the Z of the text)
const DIMENSIONS: &[(&str, &[&str])] = &[
    ("DIMRADIUS", &["DIMRADIUS", "32,0", "33,3"]),
    ("DIMDIAMETER", &["DIMDIAMETER", "32,0", "27,-3"]),
    ("DIMJOGGED", &["DIMJOGGED", "32,0", "40,5", "34,4", "37,5"]),
    ("DIMARC", &["DIMARC", "72,2", "72,4"]),
    ("DIMANGULAR", &["DIMANGULAR", "55,0", "52.5,4", "53,4"]),
    ("DIMANGULAR circle", &["DIMANGULAR", "32,0", "30,2", "34,4"]),
    ("DIMLINEAR", &["DIMLINEAR", "", "5,10", "5,13"]),
];

fn drawn_at(assoc: i64, steps: &[&str]) -> (f64, f64) {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "DIMASSOC", "value": assoc})).unwrap();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 7})).unwrap();
    s.execute("circle", &json!({"center": [30, 0], "radius": 2})).unwrap();
    s.execute("arc", &json!({"p1": [70, 0], "p2": [72, 2], "p3": [74, 0]})).unwrap();
    s.execute("line", &json!({"points": [[50, 0], [60, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[50, 0], [55, 8]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 10], [10, 10]]})).unwrap();
    s.execute("setvar", &json!({"name": "ELEVATION", "value": 3})).unwrap();
    let before = s.doc().unwrap().model.len();
    for step in steps {
        s.cmdline(step).unwrap_or_else(|e| panic!("{steps:?} at {step:?}: {e}"));
    }
    let made: Vec<_> = s.doc().unwrap().model.iter().skip(before).collect();
    assert_eq!(made.len(), 1, "{steps:?} made {} objects", made.len());
    match &made[0].kind {
        EntityKind::Dimension(d) => (d.p13.z.max(d.p15.z).max(d.defpt.z), d.text_mid.z),
        other => panic!("{steps:?} made {other:?}"),
    }
}

#[test]
fn an_interactive_dimension_made_from_an_object_takes_the_objects_z_at_every_dimassoc() {
    for assoc in [1, 2] {
        for (name, steps) in DIMENSIONS {
            let (from_object, text) = drawn_at(assoc, steps);
            assert_eq!(from_object, 7.0, "{name} at DIMASSOC {assoc}: the point from the object");
            assert_eq!(text, 3.0, "{name} at DIMASSOC {assoc}: the text");
        }
    }
}

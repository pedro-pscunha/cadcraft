//! SHAPES and LOOPS skip a canonical shape or LINE by name instead of measuring or chaining its OCS fields as plan-view values.

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

#[test]
fn shapes_names_a_canonical_circle_and_polyline_as_skipped() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    let polyline = create(
        &mut session,
        "pline",
        json!({"vertices": [{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [4.0, 0.0], "bulge": 0.0}, {"p": [4.0, 3.0], "bulge": 0.0}], "closed": true}),
    );
    let legacy = session.execute("shapes", &json!({})).unwrap();
    assert_eq!(legacy["count"], 2, "legacy control: {legacy}");
    for handle in [circle, polyline] {
        make_canonical(&mut session, handle);
    }
    let report = session.execute("shapes", &json!({})).unwrap();
    assert_eq!(report["count"], 0, "{report}");
    assert!(report["skipped"].to_string().contains("canonical"), "{report}");
}

#[test]
fn loops_does_not_chain_a_canonical_line_into_a_triangle() {
    let mut session = Session::new();
    create(&mut session, "line", json!({"points": [[0, 0], [4, 0]]}));
    let tilted = create(&mut session, "line", json!({"points": [[4, 0], [4, 3]]}));
    create(&mut session, "line", json!({"points": [[4, 3], [0, 0]]}));
    assert_eq!(session.execute("loops", &json!({})).unwrap()["count"], 1, "legacy control");
    make_canonical(&mut session, tilted);
    let report = session.execute("loops", &json!({})).unwrap();
    assert_eq!(report["count"], 0, "{report}");
    assert_eq!(report["skipped"]["Line"]["count"], 1, "{report}");
}

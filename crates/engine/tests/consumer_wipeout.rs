//! WIPEOUT from a polyline handle refuses a canonical polyline, in a tilted plane, before it makes the wipeout or erases the source.

mod consumer_support;

use consumer_support::*;
use serde_json::json;

fn drawing() -> (cadcraft_doc::Drawing, cadcraft_doc::Handle) {
    let mut session = cadcraft_engine::Session::new();
    let polyline = create(
        &mut session,
        "pline",
        json!({"vertices": [{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [10.0, 0.0], "bulge": 0.0}, {"p": [10.0, 5.0], "bulge": 0.0}, {"p": [0.0, 5.0], "bulge": 0.0}], "closed": true}),
    );
    (session.doc().unwrap().clone(), polyline)
}

#[test]
fn a_canonical_polyline_is_refused_before_the_wipeout_is_made_or_the_source_erased() {
    let (d, polyline) = drawing();
    for erase in [false, true] {
        check_refusals(&d, "wipeout", json!({"handle": polyline.hex(), "erase": erase}), &[polyline]);
    }
}

#[test]
fn a_legacy_polyline_still_becomes_a_wipeout_and_is_erased() {
    let (d, polyline) = drawing();
    let mut session = session_of(&d);
    session.execute("wipeout", &json!({"handle": polyline.hex(), "erase": true})).unwrap();
    let model = &session.doc().unwrap().model;
    assert_eq!(model.len(), 1);
    assert!(matches!(model.iter().next().unwrap().kind, cadcraft_doc::EntityKind::Wipeout(_)));
    assert!(session.doc().unwrap().entity(polyline).is_none());
}

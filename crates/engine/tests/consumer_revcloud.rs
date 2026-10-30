//! REVCLOUD from an object refuses a canonical circle or polyline in a tilted plane before it writes the cloud.

mod consumer_support;

use consumer_support::*;
use serde_json::json;

#[test]
fn a_canonical_object_is_refused_before_the_cloud_is_made() {
    let mut session = cadcraft_engine::Session::new();
    let circle = create(&mut session, "circle", json!({"center": [0, 0], "radius": 3}));
    let polyline = create(
        &mut session,
        "pline",
        json!({"vertices": [{"p": [10.0, 0.0], "bulge": 0.0}, {"p": [14.0, 0.0], "bulge": 0.0}, {"p": [14.0, 4.0], "bulge": 0.0}], "closed": true}),
    );
    let d = session.doc().unwrap().clone();
    for object in [circle, polyline] {
        check_refusals(&d, "revcloud", json!({"handle": object.hex(), "arcLength": 0.5}), &[object]);
    }
}

//! TEXTS filters and reports a canonical TEXT by its world insertion point (the tilted plane of the normal (0, 0.6, 0.8): OCS x axis
//! (-1, 0, 0), y axis (0, -0.8, 0.6)), and names a canonical text it cannot place instead of reading its OCS point.

mod consumer_support;

use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use consumer_support::*;
use serde_json::json;

fn drawing_with_text() -> (Session, cadcraft_doc::Handle) {
    let mut session = Session::new();
    let text = create(&mut session, "text", json!({"at": [2, 1], "height": 1, "text": "A"}));
    (session, text)
}

#[test]
fn a_canonical_text_is_found_in_a_world_window_and_reported_at_its_world_point() {
    let (mut session, text) = drawing_with_text();
    let world_window = json!({"window": [[-3, -1.5], [-1, 0]]});
    assert_eq!(session.execute("texts", &world_window).unwrap()["count"], 0, "legacy control: the OCS point is outside the world window");
    let legacy_window = session.execute("texts", &json!({"window": [[1, 0], [3, 2]]})).unwrap();
    assert_eq!(legacy_window["texts"][0]["at"], json!([2.0, 1.0]));
    make_canonical(&mut session, text);
    let found = session.execute("texts", &world_window).unwrap();
    assert_eq!(found["count"], 1, "{found}");
    let at = found["texts"][0]["at"].as_array().unwrap();
    assert!((at[0].as_f64().unwrap() + 2.0).abs() < 1e-9 && (at[1].as_f64().unwrap() + 0.8).abs() < 1e-9, "{at:?}");
    assert_eq!(session.execute("texts", &json!({"window": [[1, 0], [3, 2]]})).unwrap()["count"], 0);
}

#[test]
fn a_canonical_text_without_a_plane_is_reported_as_refused() {
    let (mut session, text) = drawing_with_text();
    make_canonical_in(&mut session, text, Vec3::ZERO);
    let report = session.execute("texts", &json!({})).unwrap();
    assert_eq!(report["count"], 0, "{report}");
    assert_eq!(report["refused"][0]["handle"], text.hex(), "{report}");
}

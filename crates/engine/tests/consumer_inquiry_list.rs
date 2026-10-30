//! LIST places the points of a canonical circle, arc, polyline and text in the world by their plane. The tilted plane has the normal
//! (0, 0.6, 0.8): its OCS x axis is (-1, 0, 0) and its y axis is (0, -0.8, 0.6).

mod consumer_support;

use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

fn listing(session: &mut Session, handle: cadcraft_doc::Handle) -> String {
    session.execute("list", &json!({"handles": [handle.hex()]})).unwrap()["message"].as_str().unwrap().to_string()
}

#[test]
fn a_canonical_circle_is_listed_at_its_world_center() {
    let mut session = Session::new();
    let circle = create(&mut session, "circle", json!({"center": [10, 5], "radius": 2}));
    assert!(listing(&mut session, circle).contains("X=10.0000  Y=5.0000  Z=0.0000"), "legacy control");
    make_canonical(&mut session, circle);
    let text = listing(&mut session, circle);
    assert!(text.contains("X=-10.0000  Y=-4.0000  Z=3.0000"), "{text}");
}

#[test]
fn a_canonical_arc_is_listed_at_its_world_center() {
    let mut session = Session::new();
    let arc = create(&mut session, "arc", json!({"center": [10, 5], "radius": 2, "start": 0, "end": 90}));
    make_canonical(&mut session, arc);
    let text = listing(&mut session, arc);
    assert!(text.contains("X=-10.0000  Y=-4.0000  Z=3.0000"), "{text}");
}

#[test]
fn a_canonical_polyline_lists_its_vertices_in_the_world() {
    let mut session = Session::new();
    let polyline = create(
        &mut session,
        "pline",
        json!({"vertices": [{"p": [0.0, 0.0], "bulge": 0.0}, {"p": [4.0, 0.0], "bulge": 0.0}, {"p": [4.0, 3.0], "bulge": 0.0}], "closed": true}),
    );
    assert!(listing(&mut session, polyline).contains("at point  X=4.0000  Y=3.0000  Z=0.0000"), "legacy control");
    make_canonical(&mut session, polyline);
    let text = listing(&mut session, polyline);
    // (4, 3) in the plane is 4 * (-1, 0, 0) + 3 * (0, -0.8, 0.6).
    assert!(text.contains("at point  X=-4.0000  Y=-2.4000  Z=1.8000"), "{text}");
}

#[test]
fn a_canonical_text_is_listed_at_its_world_start_point() {
    let mut session = Session::new();
    let text = create(&mut session, "text", json!({"at": [2, 1], "height": 1, "text": "A"}));
    make_canonical(&mut session, text);
    let listed = listing(&mut session, text);
    assert!(listed.contains("X=-2.0000  Y=-0.8000  Z=0.6000"), "{listed}");
}

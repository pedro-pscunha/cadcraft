use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

#[test]
fn repeated_selection_handles_do_not_duplicate_explode_or_repeat_move() {
    let mut s = Session::new();
    let h = s.execute("rectang", &json!({"p1":[0,0],"p2":[10,10]})).unwrap()["handle"].as_str().unwrap().to_string();
    let result = s.execute("explode", &json!({"handles":[h,h]})).unwrap();
    let pieces = result["handles"].as_array().unwrap();
    assert_eq!(pieces.len(), 4, "selection is a set, not repeated work: {result}");
    let first = pieces[0].as_str().unwrap();
    let h = Handle::parse_hex(first).unwrap();
    let EntityKind::Line(before) = s.doc().unwrap().entity(h).unwrap().kind.clone() else { panic!("line") };
    s.execute("move", &json!({"handles":[first,h.0],"delta":[2,0]})).unwrap();
    let EntityKind::Line(after) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("line") };
    assert_eq!(after.a.x, before.a.x + 2.0, "hex/numeric aliases must be selected once");
}

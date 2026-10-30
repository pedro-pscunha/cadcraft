//! MLEADERCOLLECT checks every multileader before it writes: a canonical one among legacy ones refuses the whole selection, in either order.

mod consumer_support;

use cadcraft_doc::Handle;
use cadcraft_engine::Session;
use consumer_support::*;
use serde_json::json;

fn drawing_with_two_block_leaders() -> (cadcraft_doc::Drawing, Handle, Handle) {
    let mut session = Session::new();
    session.execute("line", &json!({"points": [[0, 0], [2, 2]]})).unwrap();
    let line = session.doc().unwrap().model.iter().next().unwrap().handle;
    session.execute("block", &json!({"name": "B", "base": [0, 0], "handles": [line.hex()], "keep": "delete"})).unwrap();
    let a = create(&mut session, "mleader", json!({"points": [[10, 0], [10, 5]], "block": "B"}));
    let b = create(&mut session, "mleader", json!({"points": [[20, 0], [20, 5]], "block": "B"}));
    (session.doc().unwrap().clone(), a, b)
}

#[test]
fn a_canonical_leader_refuses_the_whole_collection_in_both_orders() {
    let (d, a, b) = drawing_with_two_block_leaders();
    for handles in [[a, b], [b, a]] {
        let params = json!({"handles": [handles[0].hex(), handles[1].hex()], "at": [0, 20], "mode": "horizontal", "spacing": 1});
        check_refusals(&d, "mleadercollect", params, &[a, b]);
    }
}

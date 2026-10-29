use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

#[test]
fn move_and_reverse_keep_the_heavy_polyline_representation() {
    let source = b"0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n70\n0\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n10\n20\n0\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
    let drawing = cadcraft_io::read_dxf(source).unwrap();
    let mut session = Session::new();
    session.open_drawing(drawing, "heavy.dxf", None);
    session.execute("move", &json!({"handles": ["100"], "from": [0, 0], "to": [3, 4]})).unwrap();
    session.execute("reverse", &json!({"handles": ["100"]})).unwrap();
    let entity = session.doc().unwrap().entity(Handle(0x100)).unwrap();
    let EntityKind::LwPolyline(polyline) = &entity.kind else { panic!("polyline") };
    assert!(polyline.heavy);
    assert_eq!(polyline.vertices[0].p, cadcraft_engine::geom::Vec2::new(13.0, 4.0));
    let saved = cadcraft_io::write_dxf(session.doc().unwrap());
    assert!(saved.lines().any(|line| line.trim() == "POLYLINE"));
    assert!(!saved.lines().any(|line| line.trim() == "LWPOLYLINE"));
}

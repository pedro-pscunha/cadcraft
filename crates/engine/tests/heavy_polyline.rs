use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

#[test]
fn geometry_edits_decurve_a_fitted_polyline_without_restoring_its_old_control_points() {
    let source = b"0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n70\n4\n75\n6\n0\nVERTEX\n10\n0\n20\n0\n70\n16\n0\nVERTEX\n10\n10\n20\n0\n70\n16\n0\nVERTEX\n10\n0\n20\n0\n70\n8\n0\nVERTEX\n10\n5\n20\n3\n70\n8\n0\nVERTEX\n10\n10\n20\n0\n70\n8\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
    for (command, parameters) in [
        ("move", json!({"handles": ["100"], "from": [0, 0], "to": [3, 4]})),
        ("reverse", json!({"handles": ["100"]})),
        ("pedit", json!({"handles": ["100"], "option": "width", "width": 2.5})),
    ] {
        let mut session = Session::new();
        session.open_drawing(cadcraft_io::read_dxf(source).unwrap(), "fitted.dxf", None);
        session.execute(command, &parameters).unwrap();
        for format in ["edited.dxf", "edited.dwg"] {
            let bytes = cadcraft_io::write(session.doc().unwrap(), format).unwrap();
            let drawing = cadcraft_io::read(&bytes, format).unwrap();
            let EntityKind::LwPolyline(polyline) = &drawing.entity(Handle(0x100)).unwrap().kind else { panic!("polyline") };
            assert!(polyline.heavy);
            assert!(polyline.fit.is_none(), "{command}: {format}");
            assert_eq!(polyline.vertices.len(), 3, "{command}: {format}");
        }
    }
}

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
    session.execute("pedit", &json!({"handles": ["100"], "option": "width", "width": 2.5})).unwrap();
    for format in ["saved.dxf", "saved.dwg"] {
        let bytes = cadcraft_io::write(session.doc().unwrap(), format).unwrap();
        let drawing = cadcraft_io::read(&bytes, format).unwrap();
        let EntityKind::LwPolyline(polyline) = &drawing.entity(Handle(0x100)).unwrap().kind else { panic!("polyline") };
        assert!(polyline.heavy);
        assert!(polyline.vertices.iter().all(|v| v.start_width == 2.5 && v.end_width == 2.5), "{format}: {polyline:?}");
    }
}

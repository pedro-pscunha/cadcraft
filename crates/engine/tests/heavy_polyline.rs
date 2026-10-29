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
fn polyline_extrusion_and_ocs_survive_unchanged_and_width_edits() {
    for (normal, groups) in [
        (cadcraft_engine::geom::Vec3::new(0.0, 1.0, 0.0), "210\n0\n220\n1\n230\n0\n"),
        (cadcraft_engine::geom::Vec3::new(0.0, 0.0, -1.0), "210\n0\n220\n0\n230\n-1\n"),
    ] {
        for record in [
            "POLYLINE\n5\n100\n8\n0\n10\n0\n20\n0\n30\n4\n70\n0\n{normal}0\nVERTEX\n10\n2\n20\n3\n0\nVERTEX\n10\n7\n20\n8\n0\nSEQEND\n",
            "LWPOLYLINE\n5\n100\n8\n0\n90\n2\n70\n0\n38\n4\n{normal}10\n2\n20\n3\n10\n7\n20\n8\n",
        ] {
            let source = format!("0\nSECTION\n2\nENTITIES\n0\n{}0\nENDSEC\n0\nEOF\n", record.replace("{normal}", groups));
            for edit in [false, true] {
                let mut session = Session::new();
                session.open_drawing(cadcraft_io::read_dxf(source.as_bytes()).unwrap(), "plane.dxf", None);
                if edit {
                    session.execute("pedit", &json!({"handles": ["100"], "option": "width", "width": 2.5})).unwrap();
                }
                let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
                let dxf = cadcraft_io::write_with(session.doc().unwrap(), "plane.dxf", &opts).unwrap();
                let converted = cadcraft_dwg::dxf_to_dwg(&dxf).unwrap();
                let bridge = cadcraft_dwg::dwg_to_dxf(&converted).unwrap();
                let dwg = cadcraft_io::write_with(session.doc().unwrap(), "plane.dwg", &opts).unwrap();
                for back in [
                    cadcraft_io::read(&dxf, "plane.dxf").unwrap(),
                    cadcraft_io::read_dxf(&bridge).unwrap(),
                    cadcraft_io::read(&dwg, "plane.dwg").unwrap(),
                ] {
                    let entity = back.entity(Handle(0x100)).unwrap();
                    assert_eq!(entity.common.extrusion, normal, "edit={edit}, {record}");
                    let EntityKind::LwPolyline(polyline) = &entity.kind else { panic!("polyline") };
                    assert_eq!(polyline.elevation, 4.0);
                    assert_eq!(
                        polyline.vertices.iter().map(|v| v.p).collect::<Vec<_>>(),
                        vec![cadcraft_engine::geom::Vec2::new(2.0, 3.0), cadcraft_engine::geom::Vec2::new(7.0, 8.0)]
                    );
                }
            }
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

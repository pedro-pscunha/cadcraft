use cadcraft_engine::Session;
use cadcraft_engine::geom::{Mat3, Vec2};
use serde_json::json;

fn source(polyface: bool) -> Vec<u8> {
    let flags = if polyface { 64 } else { 49 };
    let class = if polyface { "AcDbPolyFaceMesh" } else { "AcDbPolygonMesh" };
    let vertex_class = if polyface { "AcDbPolyFaceMeshVertex" } else { "AcDbPolygonMeshVertex" };
    let mut text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n0\nAPPID\n2\nVENDOR\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n100\nAcDbEntity\n8\n0\n100\n{class}\n66\n1\n70\n{flags}\n71\n{}\n72\n2\n73\n0\n74\n0\n75\n0\n",
        if polyface { 4 } else { 2 }
    );
    for (i, (x, y, z)) in [(0, 0, 1), (10, 0, 2), (10, 10, 3), (0, 10, 4)].into_iter().enumerate() {
        let data = if i == 0 { "1001\nVENDOR\n1000\nmesh-vertex\n" } else { "" };
        text += &format!(
            "0\nVERTEX\n5\n{:X}\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbVertex\n100\n{vertex_class}\n10\n{x}\n20\n{y}\n30\n{z}\n70\n{}\n{data}",
            0x101 + i,
            if polyface { 192 } else { 64 }
        );
    }
    if polyface {
        for (i, indices) in [[1, 2, -3, 0], [1, 3, 4, 0]].into_iter().enumerate() {
            text += &format!("0\nVERTEX\n5\n{:X}\n330\n100\n100\nAcDbEntity\n8\n0\n100\nAcDbFaceRecord\n10\n0\n20\n0\n30\n0\n70\n128\n", 0x105 + i);
            for (code, value) in (71..=74).zip(indices) {
                text += &format!("{code}\n{value}\n");
            }
        }
    }
    text += "0\nSEQEND\n5\n190\n330\n100\n100\nAcDbEntity\n8\n0\n1001\nVENDOR\n1000\nmesh-end\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n3\nReferences\n350\n200\n0\nXRECORD\n5\n200\n330\nC\n100\nAcDbXrecord\n280\n1\n340\n101\n0\nENDSEC\n0\nEOF\n";
    text.into_bytes()
}

fn check_geometry(bytes: &[u8], polyface: bool, matrix: Mat3) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|section| cadcraft_dxf::records(&section.tags)).collect();
    let parents: Vec<_> = records.iter().filter(|(kind, _)| kind == "POLYLINE").collect();
    assert_eq!(parents.len(), 1);
    assert!(!records.iter().any(|(kind, _)| kind == "3DFACE"));
    let parent = &parents[0].1;
    let integer = |code| parent.iter().find(|t| t.code == code).map_or(0, |t| t.i64());
    assert_eq!(integer(70), if polyface { 64 } else { 49 });
    assert_eq!((integer(71), integer(72), integer(73), integer(74), integer(75)), (if polyface { 4 } else { 2 }, 2, 0, 0, 0));
    let vertices: Vec<_> = records.iter().filter(|(kind, _)| kind == "VERTEX").collect();
    assert_eq!(vertices.len(), if polyface { 6 } else { 4 });
    for ((_, vertex), (x, y, z)) in vertices.iter().take(4).zip([(0.0, 0.0, 1.0), (10.0, 0.0, 2.0), (10.0, 10.0, 3.0), (0.0, 10.0, 4.0)]) {
        let coordinate = |code| vertex.iter().find(|t| t.code == code).map_or(0.0, |t| t.f64());
        let expected = matrix.apply(Vec2::new(x, y));
        assert!((coordinate(10) - expected.x).abs() < 1e-9);
        assert!((coordinate(20) - expected.y).abs() < 1e-9);
        assert!((coordinate(30) - z * matrix.scale_factor()).abs() < 1e-9);
    }
    if polyface {
        for ((_, face), expected) in vertices.iter().skip(4).zip([[1, 2, -3, 0], [1, 3, 4, 0]]) {
            let indices = [71, 72, 73, 74].map(|code| face.iter().find(|t| t.code == code).map_or(0, |t| t.i64()));
            assert_eq!(indices, expected);
        }
    }
}

fn check_identity(bytes: &[u8]) {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let records: Vec<_> = cadcraft_dxf::sections(&tags).into_iter().flat_map(|section| cadcraft_dxf::records(&section.tags)).collect();
    for (handle, marker) in [("101", "mesh-vertex"), ("190", "mesh-end")] {
        let (_, record) = records.iter().find(|(_, tags)| tags.iter().any(|t| t.code == 5 && t.str() == handle)).unwrap();
        assert!(record.iter().any(|t| t.code == 1000 && t.str() == marker));
    }
    assert!(records.iter().any(|(kind, tags)| kind == "XRECORD" && tags.iter().any(|t| t.code == 340 && t.str() == "101")));
}

#[test]
fn meshes_keep_one_entity_and_face_display_through_saves_and_affine_edits() {
    for polyface in [false, true] {
        for (command, params, matrix) in [
            ("", json!({}), Mat3::IDENTITY),
            ("move", json!({"handles": ["100"], "delta": [3, 4]}), Mat3::translate(Vec2::new(3.0, 4.0))),
            ("rotate", json!({"handles": ["100"], "base": [0, 0], "angle": 90}), Mat3::rotate_about(Vec2::ZERO, std::f64::consts::FRAC_PI_2)),
            ("scale", json!({"handles": ["100"], "base": [0, 0], "factor": 2}), Mat3::scale_about(Vec2::ZERO, 2.0)),
            ("mirror", json!({"handles": ["100"], "p1": [0, 0], "p2": [1, 0], "erase": true}), Mat3::mirror(Vec2::ZERO, Vec2::new(1.0, 0.0))),
        ] {
            let mut session = Session::new();
            session.open_drawing(cadcraft_io::read_dxf(&source(polyface)).unwrap(), "mesh.dxf", None);
            assert_eq!(session.doc().unwrap().entity_count(), 1, "mesh is not a collection of 3DFACEs");
            let entity = session.doc().unwrap().model.iter().next().unwrap();
            assert_eq!(entity.kind.dxf_name(), "POLYLINE");
            assert_eq!(entity.kind.prims().len(), if polyface { 8 } else { 16 });
            let display = cadcraft_render::build(session.doc().unwrap(), &cadcraft_engine::doc::Space::Model, &cadcraft_render::Options::default());
            assert!(!display.prims.is_empty());
            assert!(display.prims.iter().all(|p| p.handle == cadcraft_engine::doc::Handle(0x100)));
            let selected = session.execute("select", &json!({"at": [5, 0], "aperture": 0.1})).unwrap();
            assert_eq!(selected["handles"], json!(["100"]));
            assert!((session.execute("id", &json!({"at": [5, 0]})).unwrap()["z"].as_f64().unwrap() - 1.5).abs() < 1e-9);
            if !command.is_empty() {
                session.execute(command, &params).unwrap();
            }
            for format in ["x.dxf", "x.dwg"] {
                let mut drawing = session.doc().unwrap().clone();
                for _ in 0..2 {
                    let bytes = cadcraft_io::write_with(
                        &drawing,
                        format,
                        &cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false },
                    )
                    .unwrap();
                    let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes.clone() };
                    check_geometry(&dxf, polyface, matrix);
                    if format.ends_with("dxf") {
                        check_identity(&dxf);
                    }
                    drawing = cadcraft_io::read(&bytes, format).unwrap();
                    assert_eq!(drawing.entity_count(), 1);
                }
            }
        }
    }
}

#[test]
#[ignore = "needs acadrust arterra/polyline-children (VERTEX/SEQEND identity)"]
fn mesh_children_keep_identity_and_xdata_through_dwg() {
    for polyface in [false, true] {
        let drawing = cadcraft_io::read_dxf(&source(polyface)).unwrap();
        let dwg = cadcraft_io::write(&drawing, "mesh.dwg").unwrap();
        check_identity(&cadcraft_dwg::dwg_to_dxf(&dwg).unwrap());
    }
}

#[test]
fn smoothed_polygon_mesh_keeps_control_and_fitted_records() {
    check_smoothed("smooth.dxf");
}

#[test]
#[ignore = "needs acadrust arterra/polyline-children (mesh control-vertex flags)"]
fn smoothed_polygon_mesh_keeps_control_and_fitted_records_in_dwg() {
    check_smoothed("smooth.dwg");
}

fn check_smoothed(format: &str) {
    let mut source = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n70\n53\n71\n4\n72\n4\n73\n6\n74\n6\n75\n6\n",
    );
    for (count, flags) in [(4, 80), (6, 64)] {
        for i in 0..count {
            for j in 0..count {
                source += &format!(
                    "0\nVERTEX\n8\n0\n10\n{}\n20\n{}\n30\n0\n70\n{flags}\n",
                    f64::from(i) * 9.0 / f64::from(count - 1),
                    f64::from(j) * 9.0 / f64::from(count - 1)
                );
            }
        }
    }
    source += "0\nSEQEND\n8\n0\n0\nENDSEC\n0\nEOF\n";
    {
        let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        for _ in 0..2 {
            let entity = drawing.model.iter().next().unwrap();
            let cadcraft_engine::doc::EntityKind::PolylineMesh(mesh) = &entity.kind else { panic!("mesh") };
            assert_eq!(
                (mesh.dxf_flags(), mesh.m_count, mesh.n_count, mesh.m_density, mesh.n_density, mesh.smooth_type),
                (53, 4, 4, 6, 6, 6),
                "{format}"
            );
            assert_eq!(mesh.vertices.len(), 52, "{format}");
            assert_eq!(mesh.vertices.iter().filter(|v| v.vertex.flags == 80).count(), 16, "{format}: controls");
            assert_eq!(mesh.vertices.iter().filter(|v| v.vertex.flags == 64).count(), 36, "{format}: fitted vertices");
            assert_eq!(mesh.faces().len(), 36, "{format}: display topology");
            let bytes = cadcraft_io::write(&drawing, format).unwrap();
            drawing = cadcraft_io::read(&bytes, format).unwrap();
        }
    }
}

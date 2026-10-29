use cadcraft_engine::Session;
use cadcraft_engine::doc::{Drawing, EntityKind, Handle, LwPolyline};
use cadcraft_engine::geom::{Mat3, Vec2};
use serde_json::json;

fn source(spline: bool, negative: bool) -> Drawing {
    let (flags, vertices) = if spline {
        (
            4,
            "0\nVERTEX\n10\n0\n20\n0\n70\n16\n0\nVERTEX\n10\n10\n20\n0\n70\n16\n0\nVERTEX\n10\n0\n20\n0\n70\n8\n0\nVERTEX\n10\n5\n20\n3\n70\n8\n0\nVERTEX\n10\n10\n20\n0\n70\n8\n",
        )
    } else {
        (2, "0\nVERTEX\n10\n0\n20\n0\n70\n2\n50\n30\n42\n0.25\n0\nVERTEX\n10\n5\n20\n3\n70\n1\n42\n-0.25\n0\nVERTEX\n10\n10\n20\n0\n70\n2\n50\n330\n")
    };
    let dxf = format!(
        "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\n100\n8\n0\n70\n{flags}\n75\n{}\n40\n0.25\n41\n0.5\n210\n0\n220\n0\n230\n{}\n{vertices}0\nSEQEND\n0\nENDSEC\n0\nEOF\n",
        if spline { 6 } else { 0 },
        if negative { -1 } else { 1 }
    );
    cadcraft_io::read_dxf(dxf.as_bytes()).unwrap()
}

fn polyline(d: &Drawing) -> &LwPolyline {
    let EntityKind::LwPolyline(p) = &d.entity(Handle(0x100)).unwrap().kind else { panic!("polyline") };
    p
}

fn saved(d: &Drawing) -> Vec<Drawing> {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let dxf = cadcraft_io::write_with(d, "x.dxf", &opts).unwrap();
    let bridge = cadcraft_dwg::dwg_to_dxf(&cadcraft_dwg::dxf_to_dwg(&dxf).unwrap()).unwrap();
    let dwg = cadcraft_io::write_with(d, "x.dwg", &opts).unwrap();
    vec![cadcraft_io::read(&dxf, "x.dxf").unwrap(), cadcraft_io::read_dxf(&bridge).unwrap(), cadcraft_io::read(&dwg, "x.dwg").unwrap()]
}

#[test]
fn affine_edits_transform_controls_fit_points_and_tangents() {
    for spline in [false, true] {
        for negative in [false, true] {
            for (command, params, matrix) in [
                ("move", json!({"handles": ["100"], "delta": [3, 4]}), Mat3::translate(Vec2::new(3.0, 4.0))),
                ("grip.move", json!({"handles": ["100"], "index": 0, "to": [3, 4], "mode": "move"}), Mat3::translate(Vec2::new(3.0, 4.0))),
                ("rotate", json!({"handles": ["100"], "base": [0, 0], "angle": 90}), Mat3::rotate_about(Vec2::ZERO, std::f64::consts::FRAC_PI_2)),
                ("scale", json!({"handles": ["100"], "base": [0, 0], "factor": 2}), Mat3::scale_about(Vec2::ZERO, 2.0)),
                ("mirror", json!({"handles": ["100"], "p1": [0, 0], "p2": [1, 0], "erase": true}), Mat3::mirror(Vec2::ZERO, Vec2::new(1.0, 0.0))),
                ("stretch", json!({"window": [[-20, -20], [20, 20]], "delta": [3, 4]}), Mat3::translate(Vec2::new(3.0, 4.0))),
            ] {
                let drawing = source(spline, negative);
                let original = polyline(&drawing).saved_fit().unwrap().clone();
                let mut session = Session::new();
                session.open_drawing(drawing, "x.dxf", None);
                session.execute(command, &params).unwrap();
                let ocs_x = if negative { -1.0 } else { 1.0 };
                for back in std::iter::once(session.doc().unwrap().clone()).chain(saved(session.doc().unwrap())) {
                    let p = polyline(&back);
                    let fit = p.saved_fit().unwrap_or_else(|| panic!("{command}, spline={spline}, negative={negative}: lost fit"));
                    assert_eq!((fit.flags, fit.smooth_type, fit.extrusion), (original.flags, original.smooth_type, original.extrusion));
                    assert_eq!(fit.vertices.len(), original.vertices.len());
                    assert_eq!(p.vertices, fit.display_vertices);
                    for (vertex, old) in fit.vertices.iter().zip(&original.vertices) {
                        let expected = matrix.apply(Vec2::new(ocs_x * old.position.x, old.position.y));
                        assert!(vertex.position.xy().dist(Vec2::new(ocs_x * expected.x, expected.y)) < 1e-9, "{command}: {vertex:?}");
                        assert_eq!(vertex.flags, old.flags);
                        assert!((vertex.start_width - old.start_width * matrix.scale_factor()).abs() < 1e-9);
                        assert!((vertex.end_width - old.end_width * matrix.scale_factor()).abs() < 1e-9);
                        assert!((vertex.bulge - old.bulge * if matrix.is_mirroring() { -1.0 } else { 1.0 }).abs() < 1e-9);
                        if old.flags & 2 != 0 {
                            let expected = matrix.apply_vec(Vec2::new(ocs_x * old.tangent.cos(), old.tangent.sin())).normalized();
                            let actual = Vec2::new(ocs_x * vertex.tangent.cos(), vertex.tangent.sin());
                            assert!(actual.dist(expected) < 1e-9, "{command}: tangent");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn reverse_and_width_keep_fit_flags_and_vertex_order() {
    for spline in [false, true] {
        for (command, params) in [
            ("reverse", json!({"handles": ["100"]})),
            ("pedit", json!({"handles": ["100"], "option": "reverse"})),
            ("pedit", json!({"handles": ["100"], "option": "width", "width": 2.5})),
        ] {
            let drawing = source(spline, false);
            let original = polyline(&drawing).saved_fit().unwrap().clone();
            let mut session = Session::new();
            session.open_drawing(drawing, "x.dxf", None);
            session.execute(command, &params).unwrap();
            let width = params.get("width").is_some();
            if width {
                let fit = polyline(session.doc().unwrap()).saved_fit().unwrap();
                assert_eq!((fit.start_width, fit.end_width), (2.5, 2.5));
            }
            for back in std::iter::once(session.doc().unwrap().clone()).chain(saved(session.doc().unwrap())) {
                let p = polyline(&back);
                let fit = p.saved_fit().unwrap_or_else(|| panic!("{command}, {params}, spline={spline}: lost fit"));
                assert_eq!((fit.flags, fit.smooth_type), (original.flags, original.smooth_type));
                assert_eq!(fit.vertices.iter().map(|v| v.flags).collect::<Vec<_>>(), original.vertices.iter().map(|v| v.flags).collect::<Vec<_>>());
                assert_eq!(fit.vertices.len(), original.vertices.len());
                if width {
                    assert!(fit.vertices.iter().all(|v| v.start_width == 2.5 && v.end_width == 2.5));
                    assert!(p.vertices.iter().all(|v| v.start_width == 2.5 && v.end_width == 2.5));
                } else {
                    for control in [false, true] {
                        let expected: Vec<_> =
                            original.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).rev().map(|v| v.position).collect();
                        let actual: Vec<_> = fit.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).map(|v| v.position).collect();
                        assert_eq!(actual, expected);
                        let old: Vec<_> = original.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).collect();
                        let reversed: Vec<_> = fit.vertices.iter().filter(|v| (v.flags & 16 != 0) == control).collect();
                        for (i, vertex) in reversed.iter().enumerate() {
                            if i + 1 < old.len() {
                                let segment = old[old.len() - 2 - i];
                                assert_eq!(
                                    (vertex.start_width, vertex.end_width, vertex.bulge),
                                    (segment.end_width, segment.start_width, -segment.bulge)
                                );
                            }
                        }
                    }
                    for vertex in fit.vertices.iter().filter(|v| v.flags & 2 != 0) {
                        let old = original.vertices.iter().find(|v| v.position == vertex.position).unwrap();
                        assert!((vertex.tangent - old.tangent).cos() < -0.999999);
                    }
                }
            }
        }
    }
}

#[test]
fn shape_edits_decurve_without_restoring_the_source_controls() {
    for (command, params) in [
        ("pedit", json!({"handles": ["100"], "option": "vertex", "index": 1, "to": [5, 8]})),
        ("pedit", json!({"handles": ["100"], "option": "decurve"})),
        ("stretch", json!({"window": [[4, 2], [6, 4]], "delta": [0, 5]})),
    ] {
        for spline in [false, true] {
            let mut session = Session::new();
            session.open_drawing(source(spline, false), "x.dxf", None);
            session.execute(command, &params).unwrap();
            for back in saved(session.doc().unwrap()) {
                let p = polyline(&back);
                assert!(p.fit.is_none(), "{command}, {params}, spline={spline}");
                assert_eq!(p.vertices.len(), 3);
            }
        }
    }
}

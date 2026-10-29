use cadcraft_doc::{Common, Ellipse, EntityKind, Handle, Space};
use cadcraft_engine::{Session, grips::GripMode};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::json;

fn add(s: &mut Session, normal: Vec3, major: Vec3, ratio: f64) -> Handle {
    s.doc_mut()
        .unwrap()
        .add(
            &Space::Model,
            Common::default(),
            EntityKind::Ellipse(Ellipse { center: Vec3::new(10.0, 20.0, 3.0), major, normal, ratio, start: 0.3, end: 2.4 }),
        )
        .unwrap()
}

#[test]
fn displayed_plane_grips_drag_the_same_principal_axis() {
    for (normal, major, ratio) in [
        (-Vec3::Z, Vec3::new(5.0, 2.0, 0.0), 0.4),
        (Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6), 0.4),
        (Vec3::new(0.99, 0.0, 0.1), Vec3::new(0.1, 0.0, -0.99), 0.8),
    ] {
        let mut s = Session::new();
        let h = add(&mut s, normal, major, ratio);
        let before = s.grips_of(h).unwrap();
        let to = before[0] + (before[1] - before[0]) * 1.5;
        s.grip_edit(h, 1, to, GripMode::Stretch).unwrap();
        assert!(s.grips_of(h).unwrap()[1].near(to, 1e-10));
        let EntityKind::Ellipse(e) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("ellipse") };
        assert_eq!(e.normal, Vec3::Z);
        assert_eq!(e.major.z, 0.0);
    }
}

#[test]
fn an_edge_on_grip_edit_refuses_without_a_document_change() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    assert!(s.grip_edit(h, 1, Vec2::new(18.0, 20.0), GripMode::Stretch).is_err());
    assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before);
}

#[test]
fn an_edge_on_move_keeps_the_source_plane() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    s.execute("move", &json!({"handles": [h.hex()], "delta": [2, 3]})).unwrap();
    let EntityKind::Ellipse(e) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("ellipse") };
    assert_eq!(e.center, Vec3::new(12.0, 23.0, 3.0));
    assert_eq!(e.normal, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(e.major, Vec3::new(5.0, 0.0, 0.0));
}

#[test]
fn plane_ellipse_display_and_pick_share_projected_endpoints() {
    for (normal, major) in [
        (-Vec3::Z, Vec3::new(5.0, 2.0, 0.0)),
        (Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6)),
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0)),
    ] {
        let mut s = Session::new();
        let h = add(&mut s, normal, major, 0.4);
        let d = s.doc().unwrap();
        let entity = d.entity(h).unwrap();
        let EntityKind::Ellipse(e) = &entity.kind else { panic!("ellipse") };
        let minor = normal.normalized().cross(major).normalized() * (major.len() * e.ratio);
        let expected = [e.start, e.end].map(|t| (e.center + major * t.cos() + minor * t.sin()).xy());
        let picked = cadcraft_engine::select::hit_polylines(d, entity, 1e-5);
        let displayed =
            cadcraft_render::build_entities(d, std::iter::once(entity.as_ref()), &cadcraft_render::Options { tolerance: 1e-5, ..Default::default() });
        for endpoint in expected {
            assert!(picked.iter().flatten().any(|p| p.near(endpoint, 1e-10)));
            assert!(displayed.prims.iter().flat_map(|p| displayed.points(p)).any(|p| p.near(endpoint, 1e-10)));
        }
    }
}

#[test]
fn zero_base_wblock_keeps_the_source_ellipse_plane() {
    use cadcraft_engine::cmd::file::{IoHooks, set_io};
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |drawing, name| cadcraft_io::write(drawing, name).map_err(|e| e.to_string()),
        plot: None,
    });
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6), 0.4);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    let path = std::env::temp_dir().join(format!("cadcraft-ellipse-plane-wblock-{}.dxf", std::process::id()));
    s.execute("wblock", &json!({"handles": [h.hex()], "base": [0, 0], "path": path})).unwrap();
    let saved = cadcraft_io::read_dxf(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved.model.iter().next().unwrap().kind, before);
    let edge = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    s.execute("wblock", &json!({"handles": [edge.hex()], "base": [2, 3], "path": path})).unwrap();
    let saved = cadcraft_io::read_dxf(&std::fs::read(&path).unwrap()).unwrap();
    let EntityKind::Ellipse(e) = &saved.model.iter().next().unwrap().kind else { panic!("ellipse") };
    assert_eq!(e.center, Vec3::new(8.0, 17.0, 3.0));
    assert_eq!(e.normal, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(e.major, Vec3::new(5.0, 0.0, 0.0));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn nonlinear_block_expansion_boundaries_refuse_edge_on_ellipses() {
    for command in ["explode", "burst", "ncopy", "refedit"] {
        let mut s = Session::new();
        let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
        s.execute("block", &json!({"name": "Edge", "handles": [h.hex()], "base": [0, 0], "keep": "delete"})).unwrap();
        let result = s.execute("insert", &json!({"name": "Edge", "at": [0, 0], "rotation": 45})).unwrap();
        let insert = result["handle"].as_str().unwrap();
        let count = s.doc().unwrap().model.len();
        let result = s.execute(command, &json!({"handles": [insert], "handle": insert}));
        assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{command}: {result:?}");
        assert_eq!(s.doc().unwrap().model.len(), count, "{command}");
    }
}

#[test]
fn flatten_refuses_an_edge_on_ellipse_without_changing_the_document() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    let result = s.execute("flatten", &json!({"handles": [h.hex()]}));
    assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{result:?}");
    assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before);
}

#[test]
fn paste_moves_an_edge_on_ellipse_without_losing_its_plane() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    s.execute("copyclip", &json!({"handles": [h.hex()], "base": [0, 0]})).unwrap();
    let delta = Vec2::new(2.0, 3.0) - s.clipboard_base;
    let result = s.execute("pasteclip", &json!({"at": [2, 3]})).unwrap();
    let copied = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
    let EntityKind::Ellipse(e) = &s.doc().unwrap().entity(copied).unwrap().kind else { panic!("ellipse") };
    assert_eq!(e.center, Vec3::new(10.0 + delta.x, 20.0 + delta.y, 3.0));
    assert_eq!(e.normal, Vec3::new(0.0, 1.0, 0.0));
}

#[test]
fn scaled_import_and_exploded_file_insert_refuse_edge_on_ellipses() {
    use cadcraft_engine::cmd::file::{IoHooks, set_io};
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |drawing, name| cadcraft_io::write(drawing, name).map_err(|e| e.to_string()),
        plot: None,
    });
    let mut source = Session::new();
    add(&mut source, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let path = std::env::temp_dir().join(format!("cadcraft-ellipse-boundary-import-{}.dxf", std::process::id()));
    std::fs::write(&path, cadcraft_io::write_dxf(source.doc().unwrap())).unwrap();
    for (command, params) in [
        ("import", json!({"path": path, "scale": 2})),
        ("insert", json!({"file": path, "name": "ImportedEdge", "at": [0, 0], "rotation": 45, "explode": true})),
    ] {
        let mut s = Session::new();
        let result = s.execute(command, &params);
        assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{command}: {result:?}");
        assert_eq!(s.doc().unwrap().model.len(), 0);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn array_source_edit_refuses_an_edge_on_rotation() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let array =
        s.execute("arraypolar", &json!({"handles": [h.hex()], "center": [0, 0], "count": 4, "angle": 180, "associative": true})).unwrap()["handle"]
            .clone();
    let count = s.doc().unwrap().model.len();
    let result = s.execute("arrayedit", &json!({"handle": array, "source": {"item": [1, 0, 0]}}));
    assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{result:?}");
    assert_eq!(s.doc().unwrap().model.len(), count);
    s.execute("rotate", &json!({"handles": [array], "base": [0, 0], "angle": 45})).unwrap();
    let replacement = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let result = s.execute("arrayedit", &json!({"handle": array, "replace": {"handles": [replacement.hex()], "base": [0, 0], "items": [[1, 0, 0]]}}));
    assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{result:?}");
    assert!(s.doc().unwrap().entity(replacement).is_some());
}

#[test]
fn chspace_refuses_an_edge_on_scale_without_moving_the_entity() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let viewport = s.execute("layout.new", &json!({"name": "Sheet"})).unwrap()["viewport"].clone();
    s.execute("viewport.set", &json!({"handle": viewport, "scale": 2})).unwrap();
    let result = s.execute("chspace", &json!({"handles": [h.hex()], "to": "Sheet", "viewport": viewport}));
    assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{result:?}");
    assert_eq!(s.doc().unwrap().space_of(h), Some(Space::Model));
}

#[test]
fn reference_save_and_release_refuse_new_edge_on_geometry() {
    let mut s = Session::new();
    let line = s.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap()["handles"][0].clone();
    s.execute("block", &json!({"name": "Editable", "handles": [line], "base": [0, 0], "keep": "delete"})).unwrap();
    let insert = s.execute("insert", &json!({"name": "Editable", "at": [0, 0], "rotation": 45})).unwrap()["handle"].clone();
    let working = s.execute("refedit", &json!({"handle": insert})).unwrap()["working"][0].clone();
    let h = Handle::parse_hex(working.as_str().unwrap()).unwrap();
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            e.kind = EntityKind::Ellipse(Ellipse {
                center: Vec3::ZERO,
                major: Vec3::new(5.0, 0.0, 0.0),
                normal: Vec3::new(0.0, 1.0, 0.0),
                ratio: 0.4,
                start: 0.3,
                end: 2.4,
            })
        })
        .unwrap();
    for (command, params) in [("refset", json!({"remove": [working]})), ("refclose", json!({"save": true}))] {
        let result = s.execute(command, &params);
        assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{command}: {result:?}");
        assert!(s.doc().unwrap().entity(h).is_some());
    }
    s.execute("refclose", &json!({"save": false})).unwrap();
}

#[test]
fn array_close_refuses_new_edge_on_geometry() {
    let mut s = Session::new();
    let circle = s.execute("circle", &json!({"center": [10, 0], "radius": 1})).unwrap()["handle"].clone();
    let array = s.execute("arraypolar", &json!({"handles": [circle], "center": [0, 0], "count": 4, "angle": 180})).unwrap()["handle"].clone();
    s.execute("arrayedit", &json!({"handle": array, "source": {"item": [1, 0, 0]}})).unwrap();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let result = s.execute("arrayclose", &json!({"save": true}));
    assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{result:?}");
    assert!(s.doc().unwrap().entity(h).is_some());
    s.execute("arrayclose", &json!({"save": false})).unwrap();
}

#[test]
fn shared_transform_commands_refuse_unsupported_edge_on_operations() {
    for (command, mut params) in [
        ("rotate", json!({"base": [0, 0], "angle": 45})),
        ("scale", json!({"base": [0, 0], "factor": 2})),
        ("mirror", json!({"p1": [0, 0], "p2": [1, 1]})),
        ("align", json!({"s1": [0, 0], "d1": [0, 0], "s2": [1, 0], "d2": [0, 1]})),
        ("mocoro", json!({"base": [0, 0], "ops": [{"rotate": 45}]})),
        ("arraypolar", json!({"center": [0, 0], "count": 2, "angle": 180, "rotate": true, "associative": false})),
    ] {
        let mut s = Session::new();
        let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
        params["handles"] = json!([h.hex()]);
        let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
        let result = s.execute(command, &params);
        assert!(result.as_ref().is_err_and(|e| e.to_string().contains("ellipse")), "{command}: {result:?}");
        assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before);
    }
}

#[test]
fn an_exploded_typed_insert_refuses_before_it_creates_a_reference() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    s.execute("block", &json!({"name": "Edge", "handles": [h.hex()], "base": [0, 0], "keep": "delete"})).unwrap();
    s.script("-INSERT *Edge 0,0 1 45\n").unwrap();
    assert!(s.log.iter().any(|line| line.contains("ellipse")), "{:?}", s.log);
    assert_eq!(s.doc().unwrap().model.len(), 0);
}

#[test]
fn zero_offset_copy_keeps_the_source_ellipse_plane() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 0.8, 0.6), Vec3::new(5.0, 1.2, -1.6), 0.4);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    let result = s.execute("copy", &json!({"handles": [h.hex()], "delta": [0, 0]})).unwrap();
    let copied = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(s.doc().unwrap().entity(copied).unwrap().kind, before);
}

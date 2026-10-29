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
fn an_edge_on_move_refuses_without_a_document_change() {
    let mut s = Session::new();
    let h = add(&mut s, Vec3::new(0.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.4);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    let result = s.execute("move", &json!({"handles": [h.hex()], "delta": [2, 3]}));
    assert!(result.is_err(), "an unsupported edit must not report success: {result:?}");
    assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before);
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

//! A typed or given Z is kept: LINE, 3DPOLY and POINT keep the Z of each point, and a PLINE takes the Z of its first point as its
//! elevation (a polyline is planar). A point with no Z is at the current elevation (0 by default).

use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

fn last(s: &Session) -> EntityKind {
    s.doc().unwrap().model.iter().last().unwrap().kind.clone()
}

fn line_ends(s: &Session) -> (Vec3, Vec3) {
    match last(s) {
        EntityKind::Line(l) => (l.a, l.b),
        other => panic!("not a line: {other:?}"),
    }
}

#[test]
fn a_typed_line_keeps_the_z_of_each_point() {
    let mut s = Session::new();
    s.script("line 0,0,0 3,0,4\n\n").unwrap();
    assert_eq!(line_ends(&s), (Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 4.0)));
}

#[test]
fn relative_points_add_to_the_previous_z_and_a_point_without_z_follows_the_elevation() {
    let mut s = Session::new();
    s.script("line 0,0,1 @3,0,4 @2,2 5,5\n\n").unwrap();
    let ends: Vec<(Vec3, Vec3)> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .map(|e| match &e.kind {
            EntityKind::Line(l) => (l.a, l.b),
            other => panic!("not a line: {other:?}"),
        })
        .collect();
    assert_eq!(ends.len(), 3, "{ends:?}");
    assert_eq!(ends[0], (Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 0.0, 5.0)));
    // `@2,2` has no Z: it keeps the previous point's.
    assert_eq!(ends[1].1, Vec3::new(5.0, 2.0, 5.0));
    // `5,5` is absolute without Z: the current elevation, 0.
    assert_eq!(ends[2].1, Vec3::new(5.0, 5.0, 0.0));
}

#[test]
fn a_line_without_z_is_flat_as_before() {
    let mut s = Session::new();
    s.script("line 0,0 3,4\n\n").unwrap();
    assert_eq!(line_ends(&s), (Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 0.0)));
    s.execute("line", &json!({"points": [[1, 1], [2, 2]]})).unwrap();
    assert_eq!(line_ends(&s), (Vec3::new(1.0, 1.0, 0.0), Vec3::new(2.0, 2.0, 0.0)));
}

#[test]
fn the_json_line_keeps_the_third_number() {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0, 0], [3, 0, 4], [3, 0, 4], [3, 5, 9]], "closed": true})).unwrap();
    let ends: Vec<(Vec3, Vec3)> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .map(|e| match &e.kind {
            EntityKind::Line(l) => (l.a, l.b),
            other => panic!("not a line: {other:?}"),
        })
        .collect();
    // The repeated point makes no segment; the closing segment returns to the first point with its Z.
    assert_eq!(ends.len(), 3, "{ends:?}");
    assert_eq!(ends[0], (Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 4.0)));
    assert_eq!(ends[1], (Vec3::new(3.0, 0.0, 4.0), Vec3::new(3.0, 5.0, 9.0)));
    assert_eq!(ends[2], (Vec3::new(3.0, 5.0, 9.0), Vec3::new(0.0, 0.0, 0.0)));
}

#[test]
fn two_points_that_differ_only_in_z_make_a_line() {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0, 0], [0, 0, 5]]})).unwrap();
    assert_eq!(line_ends(&s), (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 5.0)));
}

#[test]
fn a_typed_3d_polyline_keeps_the_z_of_each_point() {
    let mut s = Session::new();
    s.script("3dpoly 0,0,0 3,0,4 3,5,9\n\n").unwrap();
    match last(&s) {
        EntityKind::Polyline3d(p) => {
            assert_eq!(p.points, vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 4.0), Vec3::new(3.0, 5.0, 9.0)]);
        }
        other => panic!("not a 3D polyline: {other:?}"),
    }
}

#[test]
fn a_point_keeps_its_z_typed_and_given() {
    let mut s = Session::new();
    s.script("point 1,2,3\n").unwrap();
    s.execute("point", &json!({"at": [4, 5, 6]})).unwrap();
    let zs: Vec<f64> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .map(|e| match &e.kind {
            EntityKind::Point(p) => p.p.z,
            other => panic!("not a point: {other:?}"),
        })
        .collect();
    assert_eq!(zs, vec![3.0, 6.0]);
}

#[test]
fn a_polyline_takes_the_z_of_its_first_point_as_its_elevation() {
    let mut s = Session::new();
    s.script("pline 0,0,7 4,0 4,3\n\n").unwrap();
    match last(&s) {
        EntityKind::LwPolyline(p) => {
            assert_eq!(p.elevation, 7.0);
            assert_eq!(p.vertices.len(), 3);
        }
        other => panic!("not a polyline: {other:?}"),
    }
    s.execute("pline", &json!({"vertices": [[0, 0, 2], [4, 0], [4, 3]]})).unwrap();
    match last(&s) {
        EntityKind::LwPolyline(p) => assert_eq!(p.elevation, 2.0),
        other => panic!("not a polyline: {other:?}"),
    }
    s.execute("pline", &json!({"vertices": [[0, 0], [4, 0]]})).unwrap();
    match last(&s) {
        EntityKind::LwPolyline(p) => assert_eq!(p.elevation, 0.0, "legacy control"),
        other => panic!("not a polyline: {other:?}"),
    }
}

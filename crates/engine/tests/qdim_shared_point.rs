//! QDIM radius and diameter take the Z of each dimension from the circle it was made from, not from whichever circle holds the same point.

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, Handle};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

fn two_circles_through_one_point() -> Session {
    // A: centre (0,0), r 10, Z 7. B: centre (5,0), r 5, Z 9. Towards (20,0) both give the point (10,0).
    let mut d = Drawing::new_metric();
    for (handle, centre, radius) in [(0x10, Vec3::new(0.0, 0.0, 7.0), 10.0), (0x11, Vec3::new(5.0, 0.0, 9.0), 5.0)] {
        d.model.push(Entity::new(Handle(handle), EntityKind::Circle(Circle { center: centre, radius })));
    }
    let mut s = Session::new();
    s.open_drawing(d, "x.dxf", None);
    s
}

fn p15_z(s: &Session, handle: &str) -> f64 {
    match &s.doc().unwrap().entity(Handle::parse_hex(handle).unwrap()).unwrap().kind {
        EntityKind::Dimension(dm) => dm.p15.z,
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_radial_dimension_takes_the_z_of_its_own_circle() {
    for mode in ["radius", "diameter"] {
        let mut s = two_circles_through_one_point();
        let made = s.execute("qdim", &json!({"handles": ["10", "11"], "at": [20, 0], "mode": mode})).unwrap();
        let handles: Vec<String> = made["handles"].as_array().unwrap().iter().map(|h| h.as_str().unwrap().to_string()).collect();
        assert_eq!(handles.len(), 2, "{mode}");
        assert_eq!(p15_z(&s, &handles[0]), 7.0, "{mode}: the dimension of circle A");
        assert_eq!(p15_z(&s, &handles[1]), 9.0, "{mode}: the dimension of circle B");
    }
}

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle};
use cadcraft_engine::Session;
use cadcraft_geom::{Vec2, Vec3};
use serde_json::json;

#[test]
fn zoom_extents_uses_available_world_bounds_and_reports_every_refusal() {
    let mut drawing = Drawing::new_metric();
    for (handle, normal) in [(0x200, Vec3::new(0.0, 0.0, -1.0)), (0x201, Vec3::ZERO), (0x202, Vec3::ZERO)] {
        let mut entity = Entity::new(Handle(handle), EntityKind::Circle(Circle { center: Vec3::new(100.0, 0.0, 0.0), radius: 2.0 }));
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common.extrusion = normal;
        drawing.model.push(entity);
    }
    let mut session = Session::new();
    session.open_drawing(drawing, "canonical extents", None);
    session.log.clear();
    session.zoom_extents().unwrap();
    let view = session.state().unwrap().view();
    assert_eq!(view.center, Vec2::new(-100.0, 0.0));
    assert!(view.height >= 4.2 && view.height < 10.0, "{view:?}");
    let properties = session.execute("properties", &json!({ "handles": ["201"] })).unwrap();
    let response = properties.to_string();
    assert!(response.contains("extentRefusals"), "{response}");
    assert!(response.contains("201"), "{response}");
    for name in ["EXTMIN", "EXTMAX"] {
        let refusal = cadcraft_engine::sysvars::get(&session, name).unwrap_err().to_string();
        assert!(refusal.contains("201") && refusal.contains("202"), "{refusal}");
    }
    let message = session.log.join("\n");
    for handle in [Handle(0x201), Handle(0x202)] {
        assert!(message.contains(&format!("object {:X}:", handle.0)), "missing {handle:?}: {message}");
    }
}

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, HVal, Handle};
use cadcraft_geom::Vec3;

#[test]
fn a_native_save_keeps_header_extents_without_a_display_geometry_query() {
    let mut drawing = Drawing::new_metric();
    let minimum = Vec3::new(-12.0, -13.0, -14.0);
    let maximum = Vec3::new(15.0, 16.0, 17.0);
    drawing.header.set("EXTMIN", HVal::Point(minimum));
    drawing.header.set("EXTMAX", HVal::Point(maximum));
    let mut entity = Entity::new(Handle(0x200), EntityKind::Circle(Circle { center: Vec3::new(100.0, 200.0, 300.0), radius: 2.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::ZERO;
    drawing.model.push(entity);
    assert!(cadcraft_render::space_extents(&drawing, &cadcraft_doc::Space::Model).is_err());
    for name in ["saved.dxf", "saved.dwg"] {
        let bytes = cadcraft_io::write(&drawing, name).unwrap();
        let reopened = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(reopened.header.point("EXTMIN"), Some(minimum), "{name}");
        assert_eq!(reopened.header.point("EXTMAX"), Some(maximum), "{name}");
    }
}

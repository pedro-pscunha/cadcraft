use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Handle};
use cadcraft_geom::Vec3;

#[test]
fn image_exports_refuse_incomplete_canonical_geometry() {
    let mut drawing = Drawing::new_metric();
    let mut entity = Entity::new(Handle(0x180), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 2.0 }));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::ZERO;
    drawing.model.push(entity);
    for name in ["invalid.png", "invalid.pdf", "invalid.svg"] {
        let error = cadcraft_io::write(&drawing, name).unwrap_err();
        assert!(error.to_string().contains("180") && error.to_string().contains("display geometry"), "{name}: {error}");
    }
}

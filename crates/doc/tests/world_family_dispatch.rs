use cadcraft_doc::{
    Entity, EntityKind, GeometryRevision, Handle, LeaderArrowLayout, Solid, Table, TableLayout, Text, WorldLayout, WorldPrimitive, WorldTextContent,
    solid_primitives, spline_primitives, text_primitives,
};
use cadcraft_geom::{Mat4, Spline, SplineSpace, Vec2, Vec3};

fn canonical(kind: EntityKind, normal: Vec3) -> Entity {
    let mut entity = Entity::new(Handle(0x120), kind);
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = normal;
    entity
}

fn parent() -> Mat4 {
    Mat4 { m: [[2.0, 0.5, 1.0, 4.0], [0.0, 1.0, 0.0, 5.0], [0.0, 0.0, 1.0, 6.0], [0.0, 0.0, 0.0, 1.0]] }
}

#[test]
fn annotation_dispatch_requires_the_correct_layout_and_keeps_the_full_parent() {
    let table = Table { insert: Vec3::new(1.0, 2.0, 3.0), ..Table::default() };
    let entity = canonical(EntityKind::Table(table), Vec3::new(0.0, 1.0, 0.0));
    assert!(entity.world_geometry(&parent()).is_err());
    let wrong = LeaderArrowLayout::default();
    assert!(entity.world_geometry_with_layout(&parent(), Some(WorldLayout::Leader(&wrong))).is_err());
    let layout = TableLayout { lines: vec![[Vec2::ZERO, Vec2::new(2.0, -1.0)]], ..TableLayout::default() };
    let geometry = entity.world_geometry_with_layout(&parent(), Some(WorldLayout::Table(&layout))).unwrap();
    assert_eq!(geometry.primitives(), [WorldPrimitive::Line { a: Vec3::new(10.0, 7.0, 9.0), b: Vec3::new(15.0, 7.0, 10.0) }]);
}

#[test]
fn solid_and_trace_dispatch_use_the_existing_ocs_builder() {
    let solid = Solid { corners: [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0), Vec3::new(2.0, 2.0, 0.0)] };
    let normal = Vec3::new(0.0, 1.0, 0.0);
    let expected = solid_primitives(&solid, normal, &parent()).unwrap();
    for kind in [EntityKind::Solid(solid.clone()), EntityKind::Trace(solid)] {
        let entity = canonical(kind, normal);
        assert_eq!(entity.world_geometry(&parent()).unwrap().primitives(), expected);
    }
}

#[test]
fn spatial_spline_dispatch_retains_the_control_z_values() {
    let mut spline = Spline::from_control(vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)], 1);
    spline.space = Some(SplineSpace { control: vec![5.0, 6.0], fit: Vec::new(), start_tangent: 0.0, end_tangent: 0.0 });
    let expected = spline_primitives(&spline, &parent()).unwrap();
    let entity = canonical(EntityKind::Spline(spline), Vec3::Z);
    assert_eq!(entity.world_geometry(&parent()).unwrap().primitives(), expected);
}

#[test]
fn text_dispatch_retains_the_complete_affine_frame_and_local_payload() {
    let text = Text {
        insert: Vec3::new(1.0, 2.0, 3.0),
        align_pt: None,
        height: 2.0,
        value: "T".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
        backward: false,
        upside_down: false,
    };
    let normal = Vec3::new(0.0, 1.0, 0.0);
    let expected = text_primitives(&text, normal, &parent()).unwrap();
    let entity = canonical(EntityKind::Text(text.clone()), normal);
    let geometry = entity.world_geometry(&parent()).unwrap();
    assert_eq!(geometry.primitives(), expected);
    let [WorldPrimitive::Text(world)] = geometry.primitives() else { panic!("not text") };
    assert_eq!(world.content, WorldTextContent::Text(text));
}

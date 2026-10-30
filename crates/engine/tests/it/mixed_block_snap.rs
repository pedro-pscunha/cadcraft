//! A legacy INSERT dispatches each descendant by its own geometry revision.
use cadcraft_doc::{Block, Circle, Common, Drawing, Entity, EntityKind, GeometryRevision, Insert, LwPolyline, Point, Space};
use cadcraft_engine::snap::{self, mode};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use std::sync::Arc;

fn insert(block: &str, at: Vec3) -> Insert {
    Insert {
        block: block.into(),
        insert: at,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    }
}
fn drawing(canonical: bool, circle: bool, nested: bool, array: bool, indexed: bool) -> Drawing {
    let mut d = Drawing::new_metric();
    let kind = if circle {
        EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 })
    } else {
        EntityKind::LwPolyline(LwPolyline {
            vertices: vec![PolyVertex::new(Vec2::new(1.0, 2.0)), PolyVertex::new(Vec2::new(3.0, 2.0))],
            closed: false,
            const_width: 0.0,
            elevation: 5.0,
            plinegen: false,
            heavy: false,
            fit: None,
            records: Default::default(),
        })
    };
    let mut child = Entity::new(d.new_handle(), kind);
    child.geometry_revision = if canonical { GeometryRevision::CanonicalV2 } else { GeometryRevision::LegacyProjectedV1 };
    child.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
    let mut block = Block::new("BODY");
    if nested {
        let mut inner = Block::new("INNER");
        inner.entities.push(child);
        d.blocks.insert(inner.name.clone(), Arc::new(inner));
        let mut reference = insert("INNER", Vec3::new(7.0, 8.0, 9.0));
        if array {
            reference.cols = 2;
            reference.col_spacing = 3.0;
        }
        block.entities.push(Entity::new(d.new_handle(), EntityKind::Insert(reference)));
    } else {
        block.entities.push(child);
    }
    d.blocks.insert(block.name.clone(), Arc::new(block));
    let mut reference = insert("BODY", Vec3::new(10.0, 20.0, 30.0));
    reference.scale = Vec3::new(2.0, 3.0, 4.0);
    reference.rotation = std::f64::consts::FRAC_PI_2;
    if array {
        reference.cols = 2;
        reference.rows = 2;
        reference.col_spacing = 10.0;
        reference.row_spacing = 20.0;
    }
    d.add(&Space::Model, Common::default(), EntityKind::Insert(reference)).unwrap();
    if indexed {
        for i in 0..300 {
            d.add(&Space::Model, Common::default(), EntityKind::Point(Point { p: Vec3::new(4000.0 + f64::from(i), 4000.0, 0.0), angle: 0.0 }))
                .unwrap();
        }
    }
    d
}

#[test]
fn canonical_children_snap_in_world_space_through_nested_insert_arrays() {
    // The child's OCS point (1,2,5) is WCS (-1,1.4,5.2). The outer INSERT maps it
    // to (5.8,18,50.8). Nested (7,8,9) adds (-24,14,36), and its second column adds (0,6,0).
    // The outer array's last cell adds (-20,10,0), outside its signed scale.
    for (nested, array, expected) in [
        (false, false, Vec3::new(5.8, 18.0, 50.8)),
        (true, false, Vec3::new(-18.2, 32.0, 86.8)),
        (false, true, Vec3::new(-14.2, 28.0, 50.8)),
        (true, true, Vec3::new(-38.2, 48.0, 86.8)),
    ] {
        for indexed in [false, true] {
            for circle in [false, true] {
                let d = drawing(true, circle, nested, array, indexed);
                let hit = snap::osnap(
                    &d,
                    &Space::Model,
                    expected.xy() + Vec2::new(0.01, 0.02),
                    0.1,
                    if circle { mode::CEN } else { mode::END },
                    None,
                    false,
                )
                .expect("canonical block leaf snap");
                assert!((hit.point - expected.xy()).len() < 1e-9, "{nested}/{array}/{indexed}: {:?}", hit.point);
                assert!((hit.world.expect("world point") - expected).len() < 1e-9, "{nested}/{array}/{indexed}: {:?}", hit.world);
            }
        }
    }
}

#[test]
fn a_canonical_nested_insert_keeps_its_own_plane() {
    let mut d = drawing(true, true, true, false, false);
    let block = Arc::make_mut(d.blocks.get_mut("BODY").unwrap());
    let mut child = (**block.entities.iter().next().unwrap()).clone();
    child.geometry_revision = GeometryRevision::CanonicalV2;
    child.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
    assert!(block.entities.replace(child));
    // Inner OCS placement turns the leaf center into (-6,1,17), then the outer
    // legacy affine maps that to (7,8,98).
    let hit = snap::osnap(&d, &Space::Model, Vec2::new(7.01, 8.02), 0.1, mode::CEN, None, false).unwrap();
    assert!((hit.point - Vec2::new(7.0, 8.0)).len() < 1e-9);
    assert!((hit.world.unwrap() - Vec3::new(7.0, 8.0, 98.0)).len() < 1e-9);
}

#[test]
fn canonical_insert_attributes_are_not_read_as_legacy_xy() {
    let mut d = drawing(false, true, false, false, false);
    let mut parent = (**d.model.iter().next().unwrap()).clone();
    let EntityKind::Insert(reference) = &mut parent.kind else { panic!("insert") };
    let text = cadcraft_doc::Text {
        insert: Vec3::new(1.0, 2.0, 5.0),
        align_pt: None,
        height: 1.0,
        value: "A".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: cadcraft_doc::HAlign::Left,
        valign: cadcraft_doc::VAlign::Baseline,
        backward: false,
        upside_down: false,
    };
    let mut attribute = cadcraft_doc::Attrib::new("A", text);
    attribute.geometry_revision = GeometryRevision::CanonicalV2;
    attribute.plane.normal = Vec3::new(0.0, 3.0, 4.0);
    reference.attribs.push(attribute);
    assert!(d.model.replace(parent));
    let hit = snap::osnap(&d, &Space::Model, Vec2::new(-0.99, 1.42), 0.1, mode::INS, None, false).expect("canonical attribute insertion");
    assert!((hit.point - Vec2::new(-1.0, 1.4)).len() < 1e-9);
}

#[test]
fn legacy_block_snaps_keep_their_projected_coordinates() {
    for (nested, array, expected) in [
        (false, false, Vec2::new(4.0, 22.0)),
        (true, false, Vec2::new(-20.0, 36.0)),
        (false, true, Vec2::new(-16.0, 32.0)),
        (true, true, Vec2::new(-40.0, 52.0)),
    ] {
        for indexed in [false, true] {
            let d = drawing(false, true, nested, array, indexed);
            let hit = snap::osnap(&d, &Space::Model, expected + Vec2::new(0.01, 0.02), 0.1, mode::CEN, None, false).unwrap();
            assert!((hit.point - expected).len() < 1e-9);
            assert_eq!(hit.world, None);
        }
    }
}

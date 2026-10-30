use std::sync::Arc;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{
    Block, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Linetype, MLine, MLineCaps, MLineCut, MLineElement, MLineJust, MLineStyle,
    MLineVertexFrame, Space, WorldPrimitive,
};
use cadcraft_geom::{Mat4, OcsFrame, Vec2, Vec3};
use cadcraft_render::{DisplayList, Kind, Options, build, entity_world_geometry};

fn drawing(canonical: bool, normal: Vec3, height: f64) -> Drawing {
    let style = MLineStyle {
        elements: vec![
            MLineElement { offset: 1.0, color: Color::Index(1), linetype: "DASHED".into() },
            MLineElement { offset: -1.0, color: Color::Index(3), linetype: "Continuous".into() },
        ],
        start_caps: MLineCaps { line: true, ..Default::default() },
        end_caps: MLineCaps { outer_arc: true, ..Default::default() },
        fill: Some(Color::Index(5)),
        joints: true,
        ..Default::default()
    };
    let mut mline = MLine::new(&style, &[Vec2::ZERO, Vec2::new(10.0, 0.0)], false, MLineJust::Zero, 1.0);
    mline.vertices[0].cuts.push(MLineCut { element: 0, start: 3.0, end: 5.0 });
    if canonical {
        let frame = OcsFrame::new(normal).unwrap();
        let directions = mline.directions();
        let miters = mline.miters();
        let offsets = mline.offsets();
        for (index, vertex) in mline.vertices.iter_mut().enumerate() {
            let direction = directions[index.min(directions.len() - 1)];
            let miter = miters[index];
            let mut params: Vec<_> = offsets.iter().map(|offset| vec![offset * miter.len(), 0.0]).collect();
            if index == 0 {
                params[0].extend_from_slice(&[3.0, 5.0]);
            }
            vertex.p = frame.to_world(vertex.p.xy().to3(height));
            vertex.frame = Some(MLineVertexFrame { dir: frame.to_world(direction.to3(0.0)), miter: frame.to_world(miter.to3(0.0)), params });
            vertex.cuts.clear();
        }
    }
    let mut entity = Entity::new(Handle(0x200), EntityKind::MLine(mline));
    entity.common.color = Color::Index(4);
    if canonical {
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common.extrusion = normal;
    }
    let mut drawing = Drawing::new_metric();
    drawing.linetypes.push(Linetype::simple("DASHED", "dashes", &[0.5, -0.5]));
    drawing.model.push(entity);
    drawing
}

fn display(drawing: &Drawing) -> DisplayList {
    let display = build(drawing, &Space::Model, &Options { tolerance: 1e-6, ..Options::default() });
    display.require_complete_geometry().unwrap();
    display
}

fn compare(actual: &DisplayList, local: &DisplayList, place: impl Fn(Vec2) -> Vec2) {
    assert!(!local.prims.is_empty());
    assert_eq!(actual.prims.len(), local.prims.len());
    assert_eq!(actual.verts.len(), local.verts.len());
    assert_eq!(actual.tris.len(), local.tris.len());
    for (actual, local) in actual.prims.iter().zip(&local.prims) {
        assert_eq!((actual.kind, actual.color, actual.lw), (local.kind, local.color, local.lw));
    }
    for (actual, local) in actual.verts.iter().chain(&actual.tris).zip(local.verts.iter().chain(&local.tris)) {
        let expected = place(*local);
        assert!((*actual - expected).len() < 1e-8, "{actual:?} != {expected:?}");
    }
}

#[test]
fn canonical_mline_keeps_element_looks_breaks_caps_and_fill_in_its_plane() {
    let legacy = display(&drawing(false, Vec3::Z, 0.0));
    let source = drawing(true, Vec3::new(0.0, 3.0, 4.0), 5.0);
    let before = source.model.clone();
    let entity = source.model.iter().next().unwrap();
    let geometry = entity_world_geometry(&source, entity, &Mat4::IDENTITY).unwrap();
    let WorldPrimitive::Line { a, b } = &geometry.primitives()[0] else { panic!("element line") };
    assert!((*a - Vec3::new(0.0, 2.2, 4.6)).len() < 1e-9);
    assert!((*b - Vec3::new(-3.0, 2.2, 4.6)).len() < 1e-9);
    let actual = display(&source);
    assert!(actual.prims.iter().any(|p| p.color == Rgb(255, 0, 0)));
    assert!(actual.prims.iter().any(|p| p.color == Rgb(0, 255, 0)));
    assert!(actual.prims.iter().any(|p| p.color == Rgb(0, 255, 255)));
    assert!(actual.prims.iter().any(|p| p.kind == Kind::Tris && p.color == Rgb(0, 0, 255)));
    compare(&actual, &legacy, |p| Vec2::new(-p.x, 3.0 - 0.8 * p.y));
    assert_eq!(source.model, before);
}

#[test]
fn canonical_mline_paint_in_xy_is_byte_identical_to_the_legacy_control() {
    let legacy = display(&drawing(false, Vec3::Z, 0.0));
    let canonical = display(&drawing(true, Vec3::Z, 0.0));
    let bits = |list: &DisplayList| list.verts.iter().chain(&list.tris).map(|p| (p.x.to_bits(), p.y.to_bits())).collect::<Vec<_>>();
    let roles = |list: &DisplayList| {
        list.prims.iter().map(|p| (p.handle, p.kind, p.color, p.aci7, p.lw.to_bits(), p.transparency, p.start, p.len)).collect::<Vec<_>>()
    };
    assert_eq!(bits(&canonical), bits(&legacy));
    assert_eq!(roles(&canonical), roles(&legacy));
}

#[test]
fn a_nested_mline_applies_its_complete_parent_once() {
    let legacy = display(&drawing(false, Vec3::Z, 0.0));
    let mut source = drawing(true, Vec3::new(0.0, 3.0, 4.0), 5.0);
    let child = source.model.iter().next().unwrap().as_ref().clone();
    source.model = Default::default();
    let mut block = Block::new("B");
    block.entities.push(child);
    source.blocks.insert("B".into(), Arc::new(block));
    let mut root = Entity::new(
        Handle(0x210),
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::new(5.0, 7.0, 11.0),
            scale: Vec3::new(2.0, 3.0, 4.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    );
    root.geometry_revision = GeometryRevision::CanonicalV2;
    root.common.extrusion = Vec3::new(0.0, 1.0, 0.0);
    source.model.push(root);
    let root = source.model.iter().next().unwrap();
    let geometry = entity_world_geometry(&source, root, &Mat4::IDENTITY).unwrap();
    let WorldPrimitive::Line { a, b } = &geometry.primitives()[0] else { panic!("element line") };
    assert!((*a - Vec3::new(-5.0, 29.4, 13.6)).len() < 1e-9);
    assert!((*b - Vec3::new(1.0, 29.4, 13.6)).len() < 1e-9);
    compare(&display(&source), &legacy, |p| Vec2::new(-5.0 + 2.0 * p.x, 27.0 + 2.4 * p.y));
}

#[test]
fn off_plane_mline_vertices_directions_and_miters_refuse_without_projection() {
    for part in 0..3 {
        let mut source = drawing(true, Vec3::Z, 0.0);
        let mut entity = source.model.iter().next().unwrap().as_ref().clone();
        let EntityKind::MLine(mline) = &mut entity.kind else { panic!("multiline") };
        match part {
            0 => mline.vertices[1].p.z += 0.25,
            1 => mline.vertices[0].frame.as_mut().unwrap().dir.z += 0.25,
            _ => mline.vertices[0].frame.as_mut().unwrap().miter.z += 0.25,
        }
        assert!(entity_world_geometry(&source, &entity, &Mat4::IDENTITY).is_err(), "part {part}");
        source.model = Default::default();
        source.model.push(entity);
        let display = build(&source, &Space::Model, &Options::default());
        assert!(display.prims.is_empty());
        assert_eq!(display.geometry_refusals.len(), 1);
        assert_eq!(display.geometry_refusals[0].handle, Handle(0x200));
        assert!(display.geometry_refusals[0].reason.contains("plane"));
    }
}

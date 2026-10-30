use std::sync::Arc;

use cadcraft_color::Color;
use cadcraft_doc::{Block, Drawing, Entity, EntityKind, GeometryRevision, Gradient, Handle, Hatch, HatchEdge, HatchLoop, Insert, Space};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_render::{DisplayList, Options, build};

fn square(lo: f64, hi: f64) -> HatchLoop {
    HatchLoop {
        vertices: [(lo, lo), (hi, lo), (hi, hi), (lo, hi)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect(),
        outer: lo == 0.0,
        ..Default::default()
    }
}

fn hatch(style: u8, pattern: &str, gradient: Option<&str>) -> Hatch {
    Hatch {
        pattern: pattern.into(),
        solid: pattern == "SOLID",
        loops: vec![square(0.0, 10.0), square(2.0, 8.0), square(4.0, 6.0)],
        scale: 0.1,
        angle: 0.2,
        associative: false,
        style,
        elevation: 0.0,
        gradient: gradient.map(|name| Gradient {
            name: name.into(),
            color1: Color::Index(1),
            color2: Color::Index(3),
            angle: 0.3,
            centered: true,
            source: Default::default(),
        }),
        origin: Vec2::new(0.2, 0.3),
        background: Some(Color::Index(5)),
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    }
}

fn drawing(mut hatch: Hatch, canonical: bool) -> Drawing {
    if canonical {
        hatch.elevation = 5.0;
        for boundary in &mut hatch.loops {
            boundary.edges = vec![HatchEdge::Polyline { vertices: std::mem::take(&mut boundary.vertices), closed: true }];
            boundary.flags = if boundary.outer { 3 } else { 2 };
        }
    }
    let mut entity = Entity::new(Handle(0x200), EntityKind::Hatch(hatch));
    entity.common.color = Color::Index(1);
    if canonical {
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        entity.common.extrusion = Vec3::new(0.0, 0.6, 0.8);
    }
    let mut drawing = Drawing::new_imperial();
    drawing.model.push(entity);
    drawing
}

fn display(drawing: &Drawing) -> DisplayList {
    let display = build(drawing, &Space::Model, &Options::default());
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
fn canonical_hatch_islands_patterns_backgrounds_and_gradients_keep_their_local_paint_roles() {
    for style in [0, 1, 2] {
        for (pattern, gradient) in [("ANSI31", None), ("SOLID", None), ("SOLID", Some("LINEAR")), ("SOLID", Some("SPHERICAL"))] {
            let payload = hatch(style, pattern, gradient);
            let local = display(&drawing(payload.clone(), false));
            let source = drawing(payload, true);
            let before = source.model.clone();
            let entity = source.model.iter().next().unwrap();
            let geometry = cadcraft_render::entity_world_geometry(&source, entity, &cadcraft_geom::Mat4::IDENTITY).unwrap();
            let cadcraft_doc::WorldPrimitive::Fill(fill) = &geometry.primitives()[0] else { panic!("hatch fill") };
            let expected = [Vec3::new(0.0, 3.0, 4.0), Vec3::new(-10.0, 3.0, 4.0), Vec3::new(-10.0, -5.0, 10.0), Vec3::new(0.0, -5.0, 10.0)];
            for (edge, expected) in fill.loops[0].iter().zip(expected) {
                let cadcraft_doc::WorldPrimitive::Line { a, .. } = edge else { panic!("square edge") };
                assert!((*a - expected).len() < 1e-9, "{a:?} != {expected:?}");
            }
            compare(&display(&source), &local, |point| Vec2::new(-point.x, 3.0 - 0.8 * point.y));
            assert_eq!(source.model, before);
        }
    }
}

#[test]
fn canonical_hatch_paint_in_xy_matches_the_legacy_buffers_bit_for_bit() {
    for (pattern, gradient) in [("ANSI31", None), ("SOLID", Some("LINEAR"))] {
        let payload = hatch(0, pattern, gradient);
        let legacy = display(&drawing(payload.clone(), false));
        let source = drawing(payload, true);
        let mut entity = source.model.iter().next().unwrap().as_ref().clone();
        entity.common.extrusion = Vec3::Z;
        let EntityKind::Hatch(hatch) = &mut entity.kind else { panic!("hatch") };
        hatch.elevation = 0.0;
        let mut flat = Drawing::new_imperial();
        flat.model.push(entity);
        let canonical = display(&flat);
        let bits = |list: &DisplayList| list.verts.iter().chain(&list.tris).map(|point| (point.x.to_bits(), point.y.to_bits())).collect::<Vec<_>>();
        let roles = |list: &DisplayList| {
            list.prims.iter().map(|p| (p.handle, p.kind, p.color, p.aci7, p.lw.to_bits(), p.transparency, p.start, p.len)).collect::<Vec<_>>()
        };
        assert_eq!(bits(&canonical), bits(&legacy));
        assert_eq!(roles(&canonical), roles(&legacy));
    }
}

#[test]
fn a_nested_hatch_keeps_its_style_layout_through_the_complete_spatial_parent() {
    let payload = hatch(1, "ANSI31", None);
    let local = display(&drawing(payload.clone(), false));
    let mut source = drawing(payload, true);
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
    let geometry = cadcraft_render::entity_world_geometry(&source, root, &cadcraft_geom::Mat4::IDENTITY).unwrap();
    let cadcraft_doc::WorldPrimitive::Fill(fill) = &geometry.primitives()[0] else { panic!("hatch fill") };
    let expected = [Vec3::new(-5.0, 27.0, 16.0), Vec3::new(15.0, 27.0, 16.0), Vec3::new(15.0, 51.0, -8.0), Vec3::new(-5.0, 51.0, -8.0)];
    for (edge, expected) in fill.loops[0].iter().zip(expected) {
        let cadcraft_doc::WorldPrimitive::Line { a, .. } = edge else { panic!("square edge") };
        assert!((*a - expected).len() < 1e-9, "{a:?} != {expected:?}");
    }
    compare(&display(&source), &local, |point| Vec2::new(-5.0 + 2.0 * point.x, 27.0 + 2.4 * point.y));
}

#[test]
fn a_canonical_hatch_never_uses_legacy_boundary_vertices() {
    let mut source = drawing(hatch(0, "ANSI31", None), false);
    let mut entity = source.model.iter().next().unwrap().as_ref().clone();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    source.model = Default::default();
    source.model.push(entity);
    let display = build(&source, &Space::Model, &Options::default());
    assert!(display.prims.is_empty());
    assert_eq!(display.geometry_refusals.len(), 1);
    assert_eq!(display.geometry_refusals[0].handle, Handle(0x200));
    assert!(display.geometry_refusals[0].reason.contains("exact edges"));
}

#[test]
fn a_canonical_arc_boundary_draws_without_replacing_its_source_edge() {
    let mut payload = hatch(0, "SOLID", None);
    payload.elevation = 5.0;
    payload.loops = vec![HatchLoop {
        outer: true,
        flags: 1,
        edges: vec![HatchEdge::Arc { center: Vec2::ZERO, radius: 2.0, start: 0.0, end: std::f64::consts::TAU, ccw: true }],
        ..Default::default()
    }];
    let mut source = Drawing::new_metric();
    let mut entity = Entity::new(Handle(0x201), EntityKind::Hatch(payload));
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 0.6, 0.8);
    source.model.push(entity);
    let before = source.model.clone();
    let display = display(&source);
    let area: f64 = display.tris.chunks_exact(3).map(|triangle| ((triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]) * 0.5).abs()).sum();
    assert!((area - 4.0 * std::f64::consts::PI * 0.8).abs() < 0.1, "{area}");
    assert_eq!(source.model, before);
}

//! Direct solver calls must not sample canonical descendants as projected rigid points.
use cadcraft_doc::{
    Block, Circle, Common, Constraint, ConstraintKind, Drawing, Entity, EntityKind, GeomRef, GeometryRevision, Handle, Insert, Line, Space,
};
use cadcraft_geom::{Vec2, Vec3};
use std::sync::Arc;

#[test]
fn a_definition_over_the_old_admission_budget_keeps_legacy_rigid_sampling() {
    use cadcraft_doc::Point;
    let (mut d, insert, _) = fixture(false, false);
    let block = Arc::make_mut(d.blocks.get_mut("BODY").unwrap());
    for i in 0..1_000_001_u64 {
        block.entities.push(Entity::new(Handle(0x1000 + i), EntityKind::Point(Point { p: Vec3::new(i as f64, 0.0, 0.0), angle: 0.0 })));
    }
    assert!(cadcraft_constraints::require_legacy_inputs(&d, [insert]).is_ok());
    let points = cadcraft_constraints::rigid_points(&d, d.entity(insert).unwrap()).unwrap();
    assert_eq!(points.len(), 256);
    assert_eq!(points.first(), Some(&Vec2::new(11.0, 22.0)));
    assert_eq!(points.last(), Some(&Vec2::new(264.0, 20.0)));
    let body = Handle(0x1000 + 1_000_000);
    let before = d.clone();
    let block = Arc::make_mut(d.blocks.get_mut("BODY").unwrap());
    let mut child = (**block.entities.get(body).unwrap()).clone();
    child.geometry_revision = GeometryRevision::CanonicalV2;
    assert!(block.entities.replace(child));
    assert!(
        matches!(cadcraft_constraints::require_legacy_inputs(&d,[insert]),Err(cadcraft_constraints::SolveError::CanonicalGeometry {handle}) if handle==body)
    );
    assert!(cadcraft_constraints::require_legacy_inputs(&before, [insert]).is_ok());
    Arc::make_mut(d.blocks.get_mut("BODY").unwrap()).entities.remove(body);
    assert!(cadcraft_constraints::require_legacy_inputs(&d, [insert]).is_ok());
}

fn fixture(canonical: bool, nested: bool) -> (Drawing, Handle, Handle) {
    let mut d = Drawing::new_metric();
    let mut child = Entity::new(d.new_handle(), EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }));
    if canonical {
        child.geometry_revision = GeometryRevision::CanonicalV2;
        child.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
    }
    let mut block = Block::new("BODY");
    block.entities.push(child);
    d.blocks.insert(block.name.clone(), Arc::new(block));
    let reference = |block: &str| Insert {
        block: block.into(),
        insert: Vec3::new(10.0, 20.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: vec![],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let root = if nested {
        let mut outer = Block::new("OUTER");
        outer.entities.push(Entity::new(d.new_handle(), EntityKind::Insert(reference("BODY"))));
        d.blocks.insert(outer.name.clone(), Arc::new(outer));
        "OUTER"
    } else {
        "BODY"
    };
    let insert = d.add(&Space::Model, Common::default(), EntityKind::Insert(reference(root))).unwrap();
    let line = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(3.0, 4.0, 0.0) })).unwrap();
    (d, insert, line)
}
fn constraint(id: u32, kind: ConstraintKind, handle: Handle) -> Constraint {
    Constraint { id, kind, refs: vec![GeomRef::whole(handle)], name: String::new(), expr: String::new() }
}
#[test]
fn direct_solver_refuses_mixed_block_graphs_before_moving_any_component() {
    for nested in [false, true] {
        for canonical_first in [false, true] {
            let (mut d, insert, line) = fixture(true, nested);
            let a = constraint(1, ConstraintKind::Fix, insert);
            let b = constraint(2, ConstraintKind::Horizontal, line);
            d.constraints = if canonical_first { vec![a, b] } else { vec![b, a] };
            let before = d.clone();
            let result = cadcraft_constraints::solve(&mut d, &Default::default());
            assert!(
                matches!(result, Err(cadcraft_constraints::SolveError::CanonicalGeometry { .. })),
                "canonical block graph was admitted: {result:?}"
            );
            assert!(cadcraft_constraints::rigid_points(&d, d.entity(insert).unwrap()).is_err());
            assert!(cadcraft_constraints::dof(&d, &[insert]).is_err());
            assert!(cadcraft_constraints::measure(&d, ConstraintKind::Radius, &[GeomRef::whole(insert)]).is_err());
            assert!(cadcraft_constraints::add(&mut d, constraint(0, ConstraintKind::Horizontal, line), &Default::default()).is_err());
            assert!(cadcraft_constraints::purge_invalid(&mut d).is_empty());
            assert!(d.model.same_as(&before.model));
            assert_eq!(d.constraints, before.constraints);
            assert!(matches!(cadcraft_constraints::ref_points(&d, insert), Err(cadcraft_constraints::SolveError::CanonicalGeometry { .. })));
            assert!(cadcraft_constraints::residuals(&d).is_err());
            assert!(cadcraft_constraints::glyphs(&d).is_err());
            assert!(cadcraft_constraints::all_satisfied(&d).is_err());
            assert!(cadcraft_constraints::restore_fixed(&mut d, &before).is_err());
            assert!(cadcraft_constraints::edited_points(&d, &before, insert).is_err());
            assert!(cadcraft_constraints::moved_anchors(&d, &before).is_err());
        }
    }
}
#[test]
fn inference_never_samples_part_of_a_mixed_batch() {
    let (mut d, insert, _) = fixture(true, true);
    let line = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(3.0, 0.0, 0.0) })).unwrap();
    let before = d.clone();
    assert!(matches!(
        cadcraft_constraints::infer(&d, &[line, insert], &Default::default()),
        Err(cadcraft_constraints::SolveError::CanonicalGeometry { .. })
    ));
    assert!(matches!(
        cadcraft_constraints::infer_placed(&d, &[line], &[insert], &Default::default()),
        Err(cadcraft_constraints::SolveError::CanonicalGeometry { .. })
    ));
    assert!(matches!(
        cadcraft_constraints::autoconstrain(&mut d, &[line, insert], &Default::default()),
        Err(cadcraft_constraints::SolveError::CanonicalGeometry { .. })
    ));
    assert!(d.model.same_as(&before.model));
    assert_eq!(d.constraints, before.constraints);
}
#[test]
fn legacy_rigid_points_and_solving_keep_their_existing_behavior() {
    let (mut d, insert, line) = fixture(false, false);
    let points = cadcraft_constraints::ref_points(&d, insert).unwrap();
    assert!(points.iter().any(|(_, p)| *p == Vec2::new(11.0, 22.0)));
    d.constraints.push(constraint(1, ConstraintKind::Horizontal, line));
    cadcraft_constraints::solve(&mut d, &Default::default()).unwrap();
    let EntityKind::Line(line) = &d.entity(line).unwrap().kind else { panic!("line") };
    assert!((line.a.y - line.b.y).abs() < 1e-7);
}

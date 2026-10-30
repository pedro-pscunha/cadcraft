//! Engine admission must refuse every mixed block input before a constraint can mutate geometry.
use cadcraft_doc::{Block, Circle, Common, Drawing, Entity, EntityKind, GeometryRevision, Insert, Line, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;
use std::sync::Arc;

#[test]
fn a_legacy_insert_with_a_canonical_child_refuses_in_both_constraint_orders() {
    for insert_first in [false, true] {
        let mut d = Drawing::new_metric();
        let mut block = Block::new("BODY");
        let mut child = Entity::new(d.new_handle(), EntityKind::Circle(Circle { center: Vec3::new(1.0, 2.0, 5.0), radius: 4.0 }));
        child.geometry_revision = GeometryRevision::CanonicalV2;
        child.common.extrusion = Vec3::new(0.0, 3.0, 4.0);
        block.entities.push(child);
        d.blocks.insert(block.name.clone(), Arc::new(block));
        let insert = d
            .add(
                &Space::Model,
                Common::default(),
                EntityKind::Insert(Insert {
                    block: "BODY".into(),
                    insert: Vec3::new(10.0, 20.0, 0.0),
                    scale: Vec3::new(1.0, 1.0, 1.0),
                    rotation: 0.0,
                    attribs: vec![],
                    cols: 1,
                    rows: 1,
                    col_spacing: 0.0,
                    row_spacing: 0.0,
                }),
            )
            .unwrap();
        let line = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(3.0, 4.0, 0.0) })).unwrap();
        let before = d.clone();
        let mut session = Session::new();
        session.open_drawing(d, "mixed.dxf", None);
        let (h1, p1, h2, p2) = if insert_first { (insert, "v0", line, "start") } else { (line, "start", insert, "v0") };
        let error = session.execute("gccoincident", &json!({"h1":h1.hex(),"p1":p1,"h2":h2.hex(),"p2":p2})).unwrap_err();
        assert!(matches!(error, cadcraft_engine::EngineError::Constraint(cadcraft_constraints::SolveError::CanonicalGeometry { .. })), "{error}");
        let error = session.execute("autoconstrain", &json!({"handles":[h1.hex(),h2.hex()]})).unwrap_err();
        assert!(matches!(error, cadcraft_engine::EngineError::Constraint(cadcraft_constraints::SolveError::CanonicalGeometry { .. })), "{error}");
        assert!(error.to_string().contains("canonical geometry"), "{error}");
        assert!(error.to_string().contains("Nothing was changed"), "{error}");
        let after = session.doc().unwrap();
        assert!(after.model.same_as(&before.model));
        assert_eq!(after.constraints, before.constraints);
    }
}

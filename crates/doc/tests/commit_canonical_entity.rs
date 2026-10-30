//! `Drawing::commit_canonical_entity` replaces a canonical entity by a canonical candidate of the same kind, or refuses with
//! the drawing as it was.

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, GeometryRevision, Line};
use cadcraft_geom::Vec3;

fn drawing(kind: EntityKind, revision: GeometryRevision) -> (Drawing, Entity) {
    let mut d = Drawing::new_metric();
    let h = d.new_handle();
    let mut e = Entity::new(h, kind);
    e.geometry_revision = revision;
    d.model.push(e.clone());
    (d, e)
}

fn line(b: Vec3) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::ZERO, b })
}

#[test]
fn a_canonical_candidate_of_the_same_kind_replaces_the_entity() {
    let (mut d, e) = drawing(line(Vec3::new(1.0, 2.0, 3.0)), GeometryRevision::CanonicalV2);
    let candidate = Entity { kind: line(Vec3::new(4.0, 5.0, 6.0)), ..e.clone() };
    d.commit_canonical_entity(candidate.clone()).unwrap();
    assert_eq!(d.entity(e.handle).unwrap().as_ref(), &candidate);
}

#[test]
fn everything_else_refuses_and_leaves_the_drawing_as_it_was() {
    let (mut d, e) = drawing(line(Vec3::new(1.0, 2.0, 3.0)), GeometryRevision::CanonicalV2);
    let before = d.model.clone();
    let kind_change = Entity { kind: EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }), ..e.clone() };
    let legacy_candidate = Entity { geometry_revision: GeometryRevision::LegacyProjectedV1, ..e.clone() };
    let not_finite = Entity { kind: line(Vec3::new(f64::NAN, 0.0, 0.0)), ..e.clone() };
    let unknown = Entity { handle: cadcraft_doc::Handle(0x7777), ..e.clone() };
    for (name, candidate) in [("kind", kind_change), ("revision", legacy_candidate), ("finite", not_finite), ("handle", unknown)] {
        assert!(d.commit_canonical_entity(candidate).is_err(), "{name}");
        assert_eq!(d.model, before, "{name}");
    }
}

#[test]
fn a_legacy_entity_is_not_committed_this_way() {
    let (mut d, e) = drawing(line(Vec3::new(1.0, 2.0, 3.0)), GeometryRevision::LegacyProjectedV1);
    let candidate = Entity { geometry_revision: GeometryRevision::CanonicalV2, kind: line(Vec3::new(4.0, 5.0, 6.0)), ..e.clone() };
    assert!(d.commit_canonical_entity(candidate).is_err());
    assert_eq!(d.entity(e.handle).unwrap().as_ref(), &e);
}

#[test]
fn a_candidate_with_a_legacy_nested_payload_refuses() {
    use cadcraft_doc::{Attrib, HAlign, Insert, Text, VAlign};
    let insert = |attribute_revision| {
        let mut a = Attrib::new(
            "T",
            Text {
                insert: Vec3::ZERO,
                align_pt: None,
                height: 1.0,
                value: "v".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: HAlign::Left,
                valign: VAlign::Baseline,
                backward: false,
                upside_down: false,
            },
        );
        a.geometry_revision = attribute_revision;
        EntityKind::Insert(Insert {
            block: "B".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![a],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        })
    };
    let (mut d, e) = drawing(insert(GeometryRevision::CanonicalV2), GeometryRevision::CanonicalV2);
    let bad = Entity { kind: insert(GeometryRevision::LegacyProjectedV1), ..e.clone() };
    assert!(d.commit_canonical_entity(bad).is_err());
    assert_eq!(d.entity(e.handle).unwrap().as_ref(), &e);
}

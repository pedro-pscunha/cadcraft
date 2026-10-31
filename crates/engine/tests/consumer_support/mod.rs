//! Fixtures for the consumer guard tests: a canonical copy of an entity placed in a drawing, and the check that a command refuses
//! it with a typed reason before it writes anything.
#![allow(dead_code)]

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, GeometryRevision, Handle, Line, RawCompound, RawRecord, RawTag, Space};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

pub fn create(session: &mut Session, command: &str, params: Value) -> Handle {
    session.execute(command, &params).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
    session.doc().unwrap().model.iter().last().unwrap().handle
}

/// A LINE from `a` to `b` with their Z values kept (the `line` command drops them).
pub fn spatial_line(session: &mut Session, a: [f64; 3], b: [f64; 3]) -> Handle {
    let line = Line { a: Vec3::new(a[0], a[1], a[2]), b: Vec3::new(b[0], b[1], b[2]) };
    session.doc_mut().unwrap().add(&Space::Model, Common::default(), EntityKind::Line(line)).unwrap()
}

/// The same entity, canonical, in the plane of the normal (0, 3, 4) / 5.
pub fn make_canonical(session: &mut Session, handle: Handle) {
    make_canonical_in(session, handle, Vec3::new(0.0, 0.6, 0.8));
}

/// The same entity, canonical, with `extrusion` as the normal of its plane.
pub fn make_canonical_in(session: &mut Session, handle: Handle, extrusion: Vec3) {
    let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = extrusion;
    session.doc_mut().unwrap().model.push(entity);
    assert_eq!(session.doc().unwrap().entity(handle).unwrap().geometry_revision, GeometryRevision::CanonicalV2);
}

/// Move a canonical CIRCLE, LINE or POINT to Z `z`: its elevation, in the frame it is stored in.
pub fn lift(session: &mut Session, handle: Handle, z: f64) {
    if z == 0.0 {
        return;
    }
    let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    match &mut entity.kind {
        EntityKind::Circle(c) => c.center.z = z,
        EntityKind::Line(l) => {
            l.a.z = z;
            l.b.z = z;
        }
        EntityKind::Point(p) => p.p.z = z,
        EntityKind::Ray(r) | EntityKind::XLine(r) => r.base.z = z,
        other => panic!("lift: no elevation for {other:?}"),
    }
    session.doc_mut().unwrap().model.push(entity);
}

/// The same entity as a raw unit: the groups of its record, kept verbatim and read-only, under the same handle. A single record only
/// (no INSERT with attributes, no POLYLINE with vertices).
pub fn make_raw(session: &mut Session, handle: Handle) {
    let entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    let (kind, groups) = cadcraft_io::entity_groups(session.doc().unwrap(), &entity);
    // TABLE requires document-level graphics/style context and is not emitted by entity_groups.
    let (kind, groups) = if kind.is_empty() {
        let text = cadcraft_io::write_dxf(session.doc().unwrap());
        cadcraft_dxf::sections(&cadcraft_dxf::parse(text.as_bytes()).unwrap())
            .into_iter()
            .filter(|section| matches!(section.name.as_str(), "ENTITIES" | "BLOCKS"))
            .flat_map(|section| cadcraft_dxf::records(&section.tags))
            .find(|(_, tags)| tags.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()) == Some(handle)))
            .unwrap_or_else(|| panic!("the complete DXF must expose Raw operand {handle:?}"))
    } else {
        (kind, groups)
    };
    let mut tags = vec![RawTag { code: 5, value: handle.hex() }, RawTag { code: 330, value: "1F".into() }];
    tags.extend(groups.iter().filter(|t| !matches!(t.code, 5 | 330)).map(|t| RawTag { code: t.code, value: t.str() }));
    let parent = RawRecord::new(kind, tags).expect("a raw record");
    let compound =
        RawCompound::new(parent, Vec::new(), entity.common.clone(), "The source has no exact geometry adapter.".into()).expect("a raw unit");
    session.doc_mut().unwrap().model.push(Entity::from_raw(compound));
    assert_eq!(session.doc().unwrap().entity(handle).unwrap().geometry_revision, GeometryRevision::RawSource);
}

pub fn session_of(drawing: &Drawing) -> Session {
    let mut session = Session::new();
    session.open_drawing(drawing.clone(), "drawing", None);
    session
}

/// `command` runs on the legacy drawing (the control); with each of `inputs` made canonical, in turn, it is refused with a reason that
/// says "canonical", and the drawing and its handle counter are as they were.
pub fn check_refusals(drawing: &Drawing, command: &str, params: Value, inputs: &[Handle]) {
    let mut control = session_of(drawing);
    control.execute(command, &params).unwrap_or_else(|error| panic!("invalid control {command} {params}: {error}"));
    for &input in inputs {
        let mut session = session_of(drawing);
        make_canonical(&mut session, input);
        let before = session.doc().unwrap().model.clone();
        let seed = session.doc().unwrap().handseed;
        let error = session.execute(command, &params).expect_err(&format!("{command} {params} accepted the canonical input {input:?}"));
        assert!(error.to_string().contains("canonical"), "{command} {params}: {error}");
        assert_eq!(session.doc().unwrap().model, before, "{command}: {input:?}");
        assert_eq!(session.doc().unwrap().handseed, seed, "{command}: {input:?}");
    }
}

pub fn point(x: f64, y: f64) -> Value {
    json!([x, y])
}

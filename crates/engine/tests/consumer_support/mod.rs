//! Fixtures for the consumer guard tests: a canonical copy of an entity placed in a drawing, and the check that a command refuses
//! it with a typed reason before it writes anything.
#![allow(dead_code)]

use cadcraft_doc::{Drawing, GeometryRevision, Handle};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

pub fn create(session: &mut Session, command: &str, params: Value) -> Handle {
    session.execute(command, &params).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
    session.doc().unwrap().model.iter().last().unwrap().handle
}

/// The same entity, canonical, in the plane of the normal (0, 3, 4) / 5.
pub fn make_canonical(session: &mut Session, handle: Handle) {
    let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
    entity.geometry_revision = GeometryRevision::CanonicalV2;
    entity.common.extrusion = Vec3::new(0.0, 0.6, 0.8);
    session.doc_mut().unwrap().model.push(entity);
    assert_eq!(session.doc().unwrap().entity(handle).unwrap().geometry_revision, GeometryRevision::CanonicalV2);
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

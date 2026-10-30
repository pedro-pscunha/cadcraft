use std::sync::Arc;

use cadcraft_doc::{GeometryRevision, Handle};
use cadcraft_engine::Session;
use serde_json::json;

#[test]
fn array_replacement_refuses_canonical_objects_before_centroid_or_block_creation() {
    for explicit_base in [false, true] {
        let mut session = Session::new();
        let circle = session.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap()["handle"].clone();
        let array = session.execute("arrayrect", &json!({"handles": [circle], "rows": 1, "cols": 2, "colSpacing": 3})).unwrap();
        let replacement = session.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap()["handle"].clone();
        let handle = Handle::parse_hex(replacement.as_str().unwrap()).unwrap();
        let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
        entity.geometry_revision = GeometryRevision::CanonicalV2;
        session.doc_mut().unwrap().model.push(entity);
        let drawing = session.doc().unwrap();
        let (model, blocks, seed) = (drawing.model.clone(), drawing.blocks.clone(), drawing.handseed);
        let mut params = json!({"handle": array["handle"], "replace": {"handles": [replacement], "items": "source"}});
        if explicit_base {
            params["replace"]["base"] = json!([0, 0]);
        }
        let error = session.execute("arrayedit", &params).unwrap_err();
        assert!(error.to_string().contains("canonical"), "{error}");
        let drawing = session.doc().unwrap();
        assert_eq!(drawing.model, model);
        assert_eq!(drawing.blocks, blocks);
        assert_eq!(drawing.handseed, seed);
    }
}

#[test]
fn array_source_extraction_refuses_canonical_references_and_nested_sources() {
    for canonical_source in [false, true] {
        let mut session = Session::new();
        let circle = session.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap()["handle"].clone();
        let array = session.execute("arrayrect", &json!({"handles": [circle], "rows": 1, "cols": 2, "colSpacing": 3})).unwrap();
        let handle = Handle::parse_hex(array["handle"].as_str().unwrap()).unwrap();
        if canonical_source {
            let drawing = session.doc_mut().unwrap();
            let source = drawing.array_params(drawing.entity(handle).unwrap()).unwrap().source;
            let block = Arc::make_mut(drawing.blocks.get_mut(&source).unwrap());
            let mut entity = block.entities.iter().next().unwrap().as_ref().clone();
            entity.geometry_revision = GeometryRevision::CanonicalV2;
            block.entities.push(entity);
        } else {
            let mut entity = session.doc().unwrap().entity(handle).unwrap().as_ref().clone();
            entity.geometry_revision = GeometryRevision::CanonicalV2;
            session.doc_mut().unwrap().model.push(entity);
        }
        let drawing = session.doc().unwrap();
        let (model, blocks, seed) = (drawing.model.clone(), drawing.blocks.clone(), drawing.handseed);
        let error = session.execute("arrayedit", &json!({"handle": handle.hex(), "source": {}})).unwrap_err();
        assert!(error.to_string().contains("canonical"), "{error}");
        let drawing = session.doc().unwrap();
        assert_eq!(drawing.model, model);
        assert_eq!(drawing.blocks, blocks);
        assert_eq!(drawing.handseed, seed);
    }
}

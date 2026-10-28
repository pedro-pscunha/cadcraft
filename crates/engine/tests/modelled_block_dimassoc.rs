//! Model-only associations need public DIMASSOC/geometry when a command copies them into a block.

use cadcraft_dxf::{Tag, records, sections};
use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

fn drawing() -> (Session, String, String) {
    let mut s = Session::new();
    let line = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"object":line,"at":[5,5]})).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("dimreassociate", &json!({"handles":[dim]})).unwrap();
    (s, line, dim)
}
fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

#[test]
fn dimension_copy_without_its_geometry_does_not_bind_to_original_objects() {
    let (mut s, _, dim) = drawing();
    let copy = s.execute("copy", &json!({"handles":[dim],"delta":[20,0]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let EntityKind::Dimension(copy) = &s.doc().unwrap().entity(Handle::parse_hex(&copy).unwrap()).unwrap().kind else { panic!("dimension") };
    assert!(copy.assoc.is_empty(), "dimension-only copy retained attachments to original geometry");
    let EntityKind::Dimension(original) = &s.doc().unwrap().entity(Handle::parse_hex(&dim).unwrap()).unwrap().kind else { panic!("dimension") };
    assert_eq!(original.assoc.len(), 2, "copy changed the original association");
}

#[test]
fn block_copy_writes_modelled_association_and_fresh_dimension_geometry() {
    let (mut s, line, dim) = drawing();
    s.execute("block", &json!({"name":"MEASURES","base":[0,0],"handles":[line,dim],"keep":"retain"})).unwrap();
    let block = s.doc().unwrap().block("MEASURES").unwrap();
    let dimension = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    let line = block.entities.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
    for dwg in [false, true] {
        let text = if dwg {
            let bytes = cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap();
            cadcraft_io::write_dxf(&cadcraft_io::read(&bytes, "x.dwg").unwrap())
        } else {
            cadcraft_io::write_dxf(s.doc().unwrap())
        };
        let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
        let secs = sections(&tags);
        let objects = records(&secs.iter().find(|s| s.name == "OBJECTS").unwrap().tags);
        let assoc = objects.iter().find(|(k, t)| {
            k == "DIMASSOC"
                && cadcraft_engine::doc::foreign::dimassoc_reference(t.iter().map(|t| (t.code, t.str()))).is_some_and(|(_, h)| h == dimension)
        });
        assert!(assoc.is_some(), "DWG={dwg}: modelled association inside a new block was not written");
        assert_eq!(
            assoc.unwrap().1.iter().filter(|t| t.code == 331).map(Tag::str).collect::<Vec<_>>(),
            vec![line.hex(); 2],
            "DWG={dwg}: block-local targets were not written"
        );
        let blocks = records(&secs.iter().find(|s| s.name == "BLOCKS").unwrap().tags);
        let dimension = &blocks.iter().find(|(k, t)| k == "DIMENSION" && val(t, 5) == dimension.hex()).unwrap().1;
        let name = val(dimension, 2);
        assert!(
            !name.is_empty() && blocks.iter().any(|(k, t)| k == "BLOCK" && val(t, 2) == name),
            "DWG={dwg}: new block dimension lacks its anonymous geometry block"
        );
    }
}

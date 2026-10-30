//! AutoCAD's dictionaries: each one in the named object dictionary reacts to it (`{ACAD_REACTORS`
//! with the root), while the extension dictionary of a block record, which holds its draw order,
//! has no reactor.

use cadcraft_doc::{Common, Drawing, EntityKind, Line, Space};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

fn objects(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags).into_iter().filter(|s| s.name == "OBJECTS").flat_map(|s| records(&s.tags)).collect()
}

fn reactors(body: &[Tag]) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for t in body {
        if t.code == 102 {
            inside = t.str().starts_with("{ACAD_REACTORS");
        } else if inside && t.code == 330 {
            out.push(t.str());
        }
    }
    out
}

fn owner(body: &[Tag]) -> Option<String> {
    cadcraft_doc::foreign::object_owner(body.iter().map(|t| (t.code, t.str()))).map(|h| h.hex())
}

#[test]
fn every_dictionary_of_the_named_object_dictionary_reacts_to_it() {
    let text = cadcraft_io::write_dxf(&Drawing::new_metric());
    let records = objects(&text);
    let root = records.iter().find(|(k, b)| k == "DICTIONARY" && owner(b).is_some_and(|o| o == "0")).expect("root dictionary");
    let root_handle = root.1.iter().find(|t| t.code == 5).unwrap().str();
    let children: Vec<_> = records.iter().filter(|(k, b)| k == "DICTIONARY" && owner(b).as_deref() == Some(root_handle.as_str())).collect();
    assert!(children.len() >= 4, "the groups, layouts and styles dictionaries");
    for (_, body) in children {
        assert_eq!(reactors(body), [root_handle.clone()], "{:?}", body.iter().find(|t| t.code == 5).map(Tag::str));
    }
}

#[test]
fn the_extension_dictionary_made_for_a_draw_order_has_no_reactor() {
    let mut drawing = Drawing::new_metric();
    let line = || EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) });
    drawing.add(&Space::Model, Common::default(), line()).unwrap();
    let top = drawing.add(&Space::Model, Common::default(), line()).unwrap();
    drawing.add(&Space::Model, Common::default(), line()).unwrap();
    drawing.space_mut(&Space::Model).unwrap().send_to_back(top);
    let records = objects(&cadcraft_io::write_dxf(&drawing));
    let holders: Vec<_> = records.iter().filter(|(k, b)| k == "DICTIONARY" && b.iter().any(|t| t.code == 3 && t.str() == "ACAD_SORTENTS")).collect();
    assert_eq!(holders.len(), 1);
    assert!(reactors(&holders[0].1).is_empty());
}

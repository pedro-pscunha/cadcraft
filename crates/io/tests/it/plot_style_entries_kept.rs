//! ACAD_PLOTSTYLENAME keeps every entry of the file, the unused placeholders too (AutoCAD lists
//! `ByLayer` beside `Normal`), while an entity or a layer that uses the logical name ByLayer still
//! points at no placeholder.

use cadcraft_doc::{Common, Drawing, EntityKind, Line, Space};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::Vec3;

fn objects(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags).into_iter().filter(|s| s.name == "OBJECTS").flat_map(|s| records(&s.tags)).collect()
}

/// The names and placeholder handles of the plot style dictionary.
fn entries(text: &str) -> Vec<(String, String)> {
    let objects = objects(text);
    let root = objects.iter().find(|(k, b)| k == "DICTIONARY" && b.iter().any(|t| t.code == 3 && t.str() == "ACAD_PLOTSTYLENAME")).unwrap();
    let handle = root.1.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ACAD_PLOTSTYLENAME").unwrap()[1].str();
    let dictionary = objects.iter().find(|(_, b)| b.iter().any(|t| t.code == 5 && t.str() == handle)).unwrap();
    dictionary.1.windows(2).filter(|p| p[0].code == 3).map(|p| (p[0].str(), p[1].str())).collect()
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.plot_style_names = vec!["ByLayer".into(), "Normal".into(), "Preto".into()];
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) })).unwrap();
    d
}

#[test]
fn the_dictionary_keeps_an_unused_by_layer_entry_with_its_placeholder() {
    let text = cadcraft_io::write_dxf(&drawing());
    let entries = entries(&text);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["ByLayer", "Normal", "Preto"]);
    let objects = objects(&text);
    for (name, handle) in &entries {
        let placeholder = objects.iter().find(|(k, b)| k == "ACDBPLACEHOLDER" && b.iter().any(|t| t.code == 5 && t.str() == *handle));
        assert!(placeholder.is_some(), "{name} needs a placeholder object");
    }
}

#[test]
fn an_entity_by_layer_points_at_no_placeholder() {
    let text = cadcraft_io::write_dxf(&drawing());
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let line = sections(&tags).into_iter().filter(|s| s.name == "ENTITIES").flat_map(|s| records(&s.tags)).find(|(k, _)| k == "LINE").unwrap();
    assert!(!line.1.iter().any(|t| t.code == 390 || t.code == 380), "{:?}", line.1);
}

#[test]
fn the_entries_survive_a_read() {
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&drawing()).as_bytes()).unwrap();
    assert_eq!(back.plot_style_names, ["ByLayer", "Normal", "Preto"]);
}

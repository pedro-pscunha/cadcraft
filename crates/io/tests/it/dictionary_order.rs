//! AutoCAD lists the entries of each dictionary by name, ignoring case: the named object
//! dictionary, the layout dictionary and the extension dictionaries alike. The tab order of the
//! layouts stays in their LAYOUT records.

use cadcraft_doc::{Drawing, Layout};
use cadcraft_dxf::{Tag, records, sections};

fn dictionaries(text: &str) -> Vec<Vec<String>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "OBJECTS")
        .flat_map(|s| records(&s.tags))
        .filter(|(kind, _)| cadcraft_doc::foreign::is_dictionary(kind))
        .map(|(_, body): (String, Vec<Tag>)| body.iter().filter(|t| t.code == 3).map(Tag::str).collect())
        .collect()
}

#[test]
fn every_dictionary_is_listed_by_name_ignoring_case() {
    let mut drawing = Drawing::new_metric();
    drawing.layouts.push(Layout::new("Zeta", 2));
    drawing.layouts.push(Layout::new("alpha", 3));
    let text = cadcraft_io::write_dxf(&drawing);
    let all = dictionaries(&text);
    assert!(all.iter().any(|names| names.iter().any(|n| n == "alpha")), "the layout dictionary");
    for names in all {
        let mut sorted = names.clone();
        sorted.sort_by_key(|n| n.to_lowercase());
        assert_eq!(names, sorted);
    }
}

#[test]
fn the_tab_order_of_the_layouts_survives_the_dictionary_order() {
    let mut drawing = Drawing::new_metric();
    drawing.layouts.push(Layout::new("Zeta", 2));
    drawing.layouts.push(Layout::new("alpha", 3));
    let before: Vec<String> = drawing.layouts.iter().map(|l| l.name.clone()).collect();
    let back = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&drawing).as_bytes()).unwrap();
    assert_eq!(back.layouts.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), before);
}

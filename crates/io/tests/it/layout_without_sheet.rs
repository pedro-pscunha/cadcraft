//! A layout the file gave viewports but no paper-space viewport (id 1) is saved as it was: AutoCAD does not add one (9 layouts of
//! Citycon, 40 viewports against the file's 31). A layout made here has its sheet written, and the file stays stable when it is read
//! and saved again.

use cadcraft_color::Color;
use cadcraft_doc::*;
use cadcraft_dxf::Tag;
use cadcraft_geom::{Vec2, Vec3};

fn viewport(id: u32) -> EntityKind {
    EntityKind::Viewport(Viewport {
        center: Vec3::new(50.0, 40.0, 0.0),
        width: 80.0,
        height: 60.0,
        view_center: Vec2::new(10.0, 10.0),
        view_height: 50.0,
        id,
        locked: false,
        frozen_layers: Vec::new(),
        layer_colors: Vec::<(String, Color)>::new(),
        clip: None,
        on: true,
        source_status: None,
    })
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let sheet = Space::Paper(d.layouts[0].name.clone());
    d.add(&sheet, Common::default(), viewport(2)).unwrap();
    d
}

/// The VIEWPORT records of a DXF text, each as its tags.
fn viewports(text: &str) -> Vec<Vec<Tag>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let starts: Vec<usize> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    starts
        .iter()
        .enumerate()
        .filter(|(_, s)| tags[**s].str() == "VIEWPORT")
        .map(|(n, s)| tags[*s..starts.get(n + 1).copied().unwrap_or(tags.len())].to_vec())
        .collect()
}

fn marked(v: &[Tag]) -> bool {
    v.iter().any(|t| t.code == 1000 && t.str() == "PAPERVIEW")
}

/// The text with the viewport records that carry the mark removed: a file whose layout has no sheet.
fn without_marked(text: &str) -> String {
    let mut tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let starts: Vec<usize> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    let doomed: Vec<(usize, usize)> = starts
        .iter()
        .enumerate()
        .filter(|(_, s)| tags[**s].str() == "VIEWPORT")
        .map(|(n, s)| (*s, starts.get(n + 1).copied().unwrap_or(tags.len())))
        .filter(|(a, b)| marked(&tags[*a..*b]))
        .collect();
    for (a, b) in doomed.into_iter().rev() {
        tags.drain(a..b);
    }
    cadcraft_dxf::write_ascii(&tags)
}

#[test]
fn a_layout_made_here_has_its_sheet_and_keeps_it_when_saved_again() {
    let text = cadcraft_io::write_dxf(&drawing());
    let vps = viewports(&text);
    assert_eq!(vps.len(), 2, "the viewport and the sheet written for it");
    assert_eq!(vps.iter().filter(|v| marked(v)).count(), 1);
    let again = cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
    assert_eq!(viewports(&again).len(), 2, "read implicit, written again");
}

#[test]
fn a_layout_the_file_gave_no_sheet_gets_none() {
    let source = without_marked(&cadcraft_io::write_dxf(&drawing()));
    assert_eq!(viewports(&source).len(), 1);
    let read = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert!(read.source_symbols.layouts_without_sheet.contains(&read.layouts[0].name.to_ascii_uppercase()));
    let saved = cadcraft_io::write_dxf(&read);
    assert_eq!(viewports(&saved).len(), 1, "no sheet added");
    assert!(viewports(&saved).iter().all(|v| !marked(v)));
}

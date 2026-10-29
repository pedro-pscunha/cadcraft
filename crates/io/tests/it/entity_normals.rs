//! Source planes and geometry survive a save without an edit.

use cadcraft_doc::EntityKind;
use cadcraft_dxf::Tag;

fn ellipse_source(normal: [f64; 3]) -> Vec<u8> {
    let mut tags = vec![
        Tag::s(0, "SECTION"),
        Tag::s(2, "ENTITIES"),
        Tag::s(0, "ELLIPSE"),
        Tag::s(5, "100"),
        Tag::s(100, "AcDbEntity"),
        Tag::s(8, "0"),
        Tag::s(100, "AcDbEllipse"),
    ];
    for (code, value) in [
        (10, 10.0),
        (20, 20.0),
        (30, 3.0),
        (11, 5.0),
        (21, 2.0),
        (31, -5e-9),
        (40, 0.3),
        (41, 0.2345678901234567),
        (42, 2.345678901234567),
        (210, normal[0]),
        (220, normal[1]),
        (230, normal[2]),
    ] {
        tags.push(Tag::f(code, value));
    }
    tags.extend([Tag::s(0, "ENDSEC"), Tag::s(0, "EOF")]);
    cadcraft_dxf::write_ascii(&tags).into_bytes()
}

fn assert_geometry(before: &[Tag], after: &[Tag]) {
    for code in [10, 20, 30, 11, 21, 31, 40, 41, 42, 210, 220, 230] {
        let old = before.iter().find(|t| t.code == code).unwrap().f64();
        let values: Vec<_> = after.iter().filter(|t| t.code == code).collect();
        assert_eq!(values.len(), 1, "group {code}");
        assert_eq!(values[0].f64().to_bits(), old.to_bits(), "group {code}");
    }
}

fn ellipse_tags(bytes: &[u8]) -> Vec<Tag> {
    cadcraft_dxf::sections(&cadcraft_dxf::parse(bytes).unwrap())
        .iter()
        .find(|s| s.name == "ENTITIES")
        .and_then(|s| cadcraft_dxf::records(&s.tags).into_iter().find(|(kind, _)| kind == "ELLIPSE"))
        .unwrap()
        .1
}

#[test]
fn an_ellipse_retains_source_parameters_in_the_model() {
    let d = cadcraft_io::read_dxf(&ellipse_source([-1e-9, 0.0, -1.0])).unwrap();
    let EntityKind::Ellipse(e) = &d.model.iter().next().unwrap().kind else { panic!("ellipse") };
    assert_eq!(e.start.to_bits(), 0.2345678901234567_f64.to_bits());
    assert_eq!(e.end.to_bits(), 2.345678901234567_f64.to_bits());
}

#[test]
fn an_ellipse_retains_its_normal_in_user_dxf() {
    let source = ellipse_source([-1e-9, 0.0, -1.0]);
    let d = cadcraft_io::read_dxf(&source).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    let before = ellipse_tags(&source);
    let after = ellipse_tags(saved.as_bytes());
    assert_geometry(&before, &after);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn an_ellipse_plane_and_numeric_bits_survive_two_dwg_saves() {
    let source = ellipse_source([-1e-9, 0.0, -1.0]);
    let before = ellipse_tags(&source);
    let mut drawing = cadcraft_io::read_dxf(&source).unwrap();
    for _ in 0..2 {
        let bytes = cadcraft_io::write(&drawing, "ellipse.dwg").unwrap();
        let dxf = cadcraft_dwg::dwg_to_dxf(&bytes).unwrap();
        assert_geometry(&before, &ellipse_tags(&dxf));
        drawing = cadcraft_io::read(&bytes, "ellipse.dwg").unwrap();
    }
}

#[test]
fn a_thin_ellipse_retains_its_exact_source_ratio() {
    let mut tags = cadcraft_dxf::parse(&ellipse_source([-1e-9, 0.0, -1.0])).unwrap();
    *tags.iter_mut().find(|t| t.code == 40).unwrap() = Tag::f(40, 1e-12);
    let source = cadcraft_dxf::write_ascii(&tags);
    let drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    assert_geometry(&ellipse_tags(source.as_bytes()), &ellipse_tags(cadcraft_io::write_dxf(&drawing).as_bytes()));
}

#[test]
fn an_ellipse_property_edit_keeps_source_geometry() {
    let source = ellipse_source([-1e-9, 0.0, -1.0]);
    let mut drawing = cadcraft_io::read_dxf(&source).unwrap();
    let handle = drawing.model.iter().next().unwrap().handle;
    drawing.modify_entity(handle, |e| e.common.layer = "changed".into()).unwrap();
    let text = cadcraft_io::write_dxf(&drawing);
    let saved = ellipse_tags(text.as_bytes());
    assert_geometry(&ellipse_tags(&source), &saved);
    assert_eq!(saved.iter().find(|t| t.code == 8).unwrap().str(), "changed");
}

//! A save gives every object of the file it kept (an extension dictionary and its XRECORD, a table style, a scale, ...) the handle the
//! file gave it, and the owners and entries that name it follow. An object CADCraft adds gets a handle above them all.

use std::sync::Arc;

use cadcraft_doc::{AssocSnap, Common, DimAssoc, DimKind, Dimension, Drawing, EntityKind, Handle, Line, SourceSymbols, Space};
use cadcraft_geom::Vec3;

use super::foreign_passthrough::{dxf, saved_twice, section, styled_objects, val};
use super::passthrough_image_roundtrip::foreign_dxf as image_dxf;

/// The handle, owner and (for a dictionary) entry targets of the object of `kind` whose group `code` is `value`.
fn object(text: &str, kind: &str, code: i32, value: &str) -> (String, String, Vec<String>) {
    let (_, tags) = section(text, "OBJECTS")
        .into_iter()
        .find(|(k, t)| k == kind && val(t, code) == value)
        .unwrap_or_else(|| panic!("no {kind} with group {code} {value}: {text}"));
    let owner = tags.iter().rfind(|t| t.code == 330).map(|t| t.str()).unwrap_or_default();
    (val(&tags, 5), owner, tags.iter().filter(|t| t.code == 350).map(|t| t.str()).collect())
}

#[test]
fn kept_objects_keep_their_handles_and_their_links() {
    let d = cadcraft_io::read_dxf(styled_objects().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        // The XRECORD of a layout's extension dictionary: handle 33, owned by the dictionary 32, which the layout (31) names.
        let (xrecord, owner, _) = object(&text, "XRECORD", 1, "thumbnail");
        assert_eq!((xrecord.as_str(), owner.as_str()), ("33", "32"), "{what}: XRECORD");
        let (dictionary, owner, entries) = object(&text, "DICTIONARY", 3, "ADSK_XREC_LAYOUTTHUMBNAIL");
        assert_eq!((dictionary.as_str(), owner.as_str(), entries.as_slice()), ("32", "31", ["33".to_string()].as_slice()), "{what}: dictionary");
        // The same for the style's extension dictionaries and the scale.
        for (text_of, handle, owner) in [("roundtrip", "43", "42"), ("cellstylemap", "53", "52")] {
            let (h, o, _) = object(&text, "XRECORD", 1, text_of);
            assert_eq!((h.as_str(), o.as_str()), (handle, owner), "{what}: {text_of}");
        }
        let (scale, _, _) = object(&text, "SCALE", 300, "A0");
        assert_eq!(scale, "61", "{what}: scale");
    }
}

/// The objects CADCraft writes itself follow the record they belong to: an image's definition reactor (source handle 42) keeps it, and
/// so does the SEQEND that ends an INSERT's attributes (source handle 22).
#[test]
fn written_records_that_follow_an_owner_keep_the_handle_of_the_file() {
    let d = cadcraft_io::read_dxf(image_dxf().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (_, reactor) = section(&text, "OBJECTS").into_iter().find(|(k, _)| k == "IMAGEDEF_REACTOR").unwrap();
        let (_, image) = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "IMAGE").unwrap();
        assert_eq!((val(&reactor, 5).as_str(), val(&image, 360).as_str()), ("42", "42"), "{what}: IMAGEDEF_REACTOR");
    }

    let source = dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "INSERT"),
        (5, "20"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockReference"),
        (66, "1"),
        (2, "NOPE"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (0, "ATTRIB"),
        (5, "21"),
        (330, "20"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbText"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (40, "1"),
        (1, "v"),
        (100, "AcDbAttribute"),
        (2, "T"),
        (70, "0"),
        (0, "SEQEND"),
        (5, "22"),
        (330, "20"),
        (100, "AcDbEntity"),
        (8, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (_, seqend) = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "SEQEND").unwrap();
        assert_eq!((val(&seqend, 5).as_str(), val(&seqend, 330).as_str()), ("22", "20"), "{what}: SEQEND");
    }
}

/// A dimension's DIMASSOC and the extension dictionary that holds it, written from the model, take the handles the file gave them.
#[test]
fn a_written_dimension_association_keeps_the_handles_of_the_file() {
    let mut d = Drawing::new_metric();
    let c = Common::default;
    let l1 = d.add(&Space::Model, c(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(0.0, 5.0, 0.0) })).unwrap();
    let l2 = d.add(&Space::Model, c(), EntityKind::Line(Line { a: Vec3::new(10.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) })).unwrap();
    let links = vec![
        DimAssoc { point: "p13".into(), handle: l1, snap: AssocSnap::Start },
        DimAssoc { point: "p14".into(), handle: l2, snap: AssocSnap::Start },
    ];
    let dim = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(10.0, 0.0, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: links,
        marks: Default::default(),
        leader_length: 0.0,
    };
    let dim = d.add(&Space::Model, c(), EntityKind::Dimension(dim)).unwrap();
    let mut source = SourceSymbols::default();
    source.handles.insert(("DIMASSOC".into(), dim.hex()), Handle(0x50));
    source.handles.insert(("DIMASSOC/XDICT".into(), dim.hex()), Handle(0x51));
    d.source_symbols = Arc::new(source);
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (_, assoc) = section(&text, "OBJECTS").into_iter().find(|(k, _)| k == "DIMASSOC").unwrap();
        let (_, dimension) = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "DIMENSION").unwrap();
        assert_eq!((val(&assoc, 5).as_str(), val(&assoc, 330).as_str(), val(&dimension, 360).as_str()), ("50", "51", "51"), "{what}: DIMASSOC");
    }
}

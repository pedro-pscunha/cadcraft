//! A save equals what AutoCAD saves: CADCraft writes its private `CADCRAFT` data only where the target release has no native
//! home for the value. An unedited save at R2018 (DXF and DWG) of a drawing with viewport frozen layers, a gradient hatch, a
//! built-in text font, an associative dimension and a current multileader style writes no `CADCRAFT` xdata, no `CADCRAFT_*`
//! XRECORD and no `CADCRAFT` APPID for them, and the file reopens with every value. One test per exception that stays.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::*;
use cadcraft_dxf::{Tag, parse, records, sections};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version};

fn square(size: f64) -> Vec<PolyVertex> {
    [(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect()
}

fn gradient() -> Gradient {
    Gradient {
        name: "SPHERICAL".into(),
        color1: Color::True(Rgb(91, 130, 69)),
        color2: Color::True(Rgb(188, 216, 162)),
        angle: 0.5,
        centered: true,
        source: Default::default(),
    }
}

fn hatch(background: Option<Color>) -> Hatch {
    Hatch {
        pattern: "SOLID".into(),
        solid: true,
        loops: vec![HatchLoop { vertices: square(10.0), outer: true, sources: Vec::new(), ..Default::default() }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: Some(gradient()),
        origin: Vec2::ZERO,
        background,
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    }
}

fn viewport(id: u32, frozen: &[&str]) -> EntityKind {
    EntityKind::Viewport(Viewport {
        center: Vec3::new(5.0, 4.0, 0.0),
        width: 8.0,
        height: 6.0,
        view_center: Vec2::new(5.0, 2.5),
        view_height: 12.0,
        id,
        locked: false,
        frozen_layers: frozen.iter().map(|l| l.to_string()).collect(),
        layer_colors: Vec::new(),
        clip: None,
        on: true,
        source_status: None,
    })
}

/// The fixture: every value that has a native home.
fn fixture() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Walls"));
    d.text_styles.push(TextStyle { name: "Body".into(), font: "Arial".into(), ..TextStyle::default() });
    d.mleader_styles.push(MLeaderStyle { name: "Notes".into(), ..MLeaderStyle::default() });
    d.header.set_str("CMLEADERSTYLE", "Notes");
    d.add(&Space::Paper("Layout1".into()), Common::default(), viewport(1, &[])).unwrap();
    d.add(&Space::Paper("Layout1".into()), Common::default(), viewport(2, &["Walls"])).unwrap();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(None))).unwrap();
    let line = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })).unwrap();
    let dimension = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(0.0, 3.0, 0.0),
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
        assoc: vec![
            DimAssoc { point: "p13".into(), handle: line, snap: AssocSnap::Start },
            DimAssoc { point: "p14".into(), handle: line, snap: AssocSnap::End },
        ],
        marks: Default::default(),
        leader_length: 0.0,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension)).unwrap();
    d
}

/// A save as the file's DXF text: a DXF as written, a DWG through the independent DXF rendition.
fn saved(d: &Drawing, name: &str, version: Version) -> (Vec<u8>, Vec<Tag>) {
    let bytes = cadcraft_io::write_with(d, name, &SaveOptions { version: Some(version), binary: false }).unwrap();
    let dxf = if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes.clone() };
    (bytes, parse(&dxf).unwrap())
}

/// The kinds of the records that carry `CADCRAFT` xdata.
fn private_xdata(tags: &[Tag]) -> Vec<String> {
    sections(tags)
        .into_iter()
        .flat_map(|s| records(&s.tags))
        .filter(|(kind, body)| kind != "APPID" && body.iter().any(|t| t.code == 1001 && t.str().trim() == "CADCRAFT"))
        .map(|(kind, _)| kind)
        .collect()
}

fn private_xrecords(tags: &[Tag]) -> Vec<String> {
    tags.iter().filter(|t| t.code == 3 && t.str().starts_with("CADCRAFT_")).map(Tag::str).collect()
}

fn appid_registered(tags: &[Tag]) -> bool {
    sections(tags)
        .into_iter()
        .flat_map(|s| records(&s.tags))
        .any(|(kind, body)| kind == "APPID" && body.iter().any(|t| t.code == 2 && t.str().trim() == "CADCRAFT"))
}

/// The CELLSTYLES xdata of a TABLESTYLE is written by another change (it keeps its APPID registered while it lasts).
fn assert_no_private_copy(tags: &[Tag], what: &str) {
    let xdata: Vec<String> = private_xdata(tags).into_iter().filter(|kind| kind != "TABLESTYLE").collect();
    assert!(xdata.is_empty(), "{what}: CADCRAFT xdata on {xdata:?}");
    assert!(private_xrecords(tags).is_empty(), "{what}: {:?}", private_xrecords(tags));
    // The application is registered only when some xdata names it.
    assert_eq!(appid_registered(tags), !private_xdata(tags).is_empty(), "{what}: APPID CADCRAFT");
}

fn viewport_frozen(d: &Drawing) -> Vec<Vec<String>> {
    d.layouts
        .iter()
        .flat_map(|l| l.entities.iter())
        .filter_map(|e| match &e.kind {
            EntityKind::Viewport(v) if v.id == 2 => Some(v.frozen_layers.clone()),
            _ => None,
        })
        .collect()
}

fn assert_values(d: &Drawing, what: &str) {
    assert_eq!(viewport_frozen(d), [vec!["Walls".to_string()]], "{what}: frozen layers");
    let hatch = d.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h) } else { None }).expect("hatch");
    assert_eq!(
        hatch.gradient.as_ref().map(|g| (g.name.as_str(), g.color1, g.color2, g.angle, g.centered)),
        Some(("SPHERICAL", gradient().color1, gradient().color2, 0.5, true)),
        "{what}: gradient"
    );
    assert_eq!(d.text_style("Standard").map(|s| s.font.as_str()), Some("CADCraft Stroke"), "{what}: built-in font");
    assert_eq!(d.text_style("Body").map(|s| s.font.as_str()), Some("Arial"), "{what}: font");
    assert_eq!(d.header.str("CMLEADERSTYLE", ""), "Notes", "{what}: current multileader style");
    let dimension = d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm) } else { None }).expect("dimension");
    assert_eq!(
        dimension.assoc.iter().map(|a| (a.point.as_str(), a.snap.clone())).collect::<Vec<_>>(),
        [("p13", AssocSnap::Start), ("p14", AssocSnap::End)],
        "{what}: associativity"
    );
}

#[test]
fn an_unedited_r2018_save_writes_no_private_copy_of_a_native_value() {
    let d = fixture();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, tags) = saved(&d, name, Version::R2018);
        // A DWG keeps the current multileader style as an XRECORD (see `exception_dwg_current_multileader_style`).
        let tags: Vec<Tag> =
            if name.ends_with(".dwg") { tags.into_iter().filter(|t| !(t.code == 3 && t.str() == "CADCRAFT_MLEADERSTYLE")).collect() } else { tags };
        assert_no_private_copy(&tags, name);
        assert_values(&cadcraft_io::read(&bytes, name).unwrap(), name);
    }
}

#[test]
fn a_save_of_a_save_is_the_same_file() {
    // The table records and objects are numbered above the entities' handles, not above the drawing's next handle.
    for name in ["x.dxf", "x.dwg"] {
        let options = SaveOptions { version: Some(Version::R2018), binary: false };
        let once = cadcraft_io::write_with(&fixture(), name, &options).unwrap();
        let twice = cadcraft_io::write_with(&cadcraft_io::read(&once, name).unwrap(), name, &options).unwrap();
        let thrice = cadcraft_io::write_with(&cadcraft_io::read(&twice, name).unwrap(), name, &options).unwrap();
        assert!(twice == thrice, "{name}: the second save differs from the third");
    }
}

/// Index of the end of the record that starts at `start` (the next group 0, or the end).
fn record_end(tags: &[Tag], start: usize) -> usize {
    tags.iter().enumerate().skip(start + 1).find(|(_, t)| t.code == 0).map_or(tags.len(), |(i, _)| i)
}

/// A file saved before the native homes: the viewport's frozen layers as xdata only, a text style's built-in font named in
/// xdata, the gradient hatch with its xdata beside the native groups, and the current multileader style in an XRECORD. All
/// are still read.
#[test]
fn files_saved_with_the_private_copies_still_reopen() {
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    let mut tags = parse(&cadcraft_io::write_with(&fixture(), "x.dxf", &options).unwrap()).unwrap();
    // The viewport: its 331 list gone, VPFROZEN xdata in its place.
    let vp = (0..tags.len())
        .find(|&i| tags[i].code == 0 && tags[i].str() == "VIEWPORT" && tags[i..record_end(&tags, i)].iter().any(|t| t.code == 331))
        .unwrap();
    let end = record_end(&tags, vp);
    tags.splice(
        vp..end,
        tags[vp..end]
            .iter()
            .filter(|t| t.code != 331)
            .cloned()
            .chain([Tag::s(1001, "CADCRAFT"), Tag::s(1000, "VPFROZEN"), Tag::s(1002, "{"), Tag::s(1000, "Walls"), Tag::s(1002, "}")])
            .collect::<Vec<_>>(),
    );
    // The style "Body": the file `txt` and a font named in xdata.
    let style = (0..tags.len())
        .find(|&i| tags[i].code == 0 && tags[i].str() == "STYLE" && tags[i..record_end(&tags, i)].iter().any(|t| t.code == 2 && t.str() == "Body"))
        .unwrap();
    let end = record_end(&tags, style);
    let mut record: Vec<Tag> = tags[style..end].iter().map(|t| if t.code == 3 { Tag::s(3, "txt") } else { t.clone() }).collect();
    record.extend([Tag::s(1001, "CADCRAFT"), Tag::s(1000, "FONT"), Tag::s(1000, "Legacy Face")]);
    tags.splice(style..end, record);
    // The multileader style: no $CMLEADERSTYLE, an XRECORD in the named object dictionary.
    let at = tags.iter().position(|t| t.code == 9 && t.str() == "$CMLEADERSTYLE").unwrap();
    tags.drain(at..at + 2);
    let objects = tags.iter().position(|t| t.code == 2 && t.str() == "OBJECTS").unwrap();
    let root = (objects..tags.len()).find(|&i| tags[i].code == 0 && tags[i].str() == "DICTIONARY").unwrap();
    let root_handle = tags[root..].iter().find(|t| t.code == 5).unwrap().str();
    let entry = (root..tags.len()).find(|&i| tags[i].code == 281).unwrap() + 1;
    tags.splice(entry..entry, [Tag::s(3, "CADCRAFT_MLEADERSTYLE"), Tag::s(350, "FFFF0")]);
    let endsec = (objects..tags.len()).find(|&i| tags[i].code == 0 && tags[i].str() == "ENDSEC").unwrap();
    tags.splice(
        endsec..endsec,
        [Tag::s(0, "XRECORD"), Tag::s(5, "FFFF0"), Tag::s(330, root_handle), Tag::s(100, "AcDbXrecord"), Tag::i(280, 1), Tag::s(1, "Notes")],
    );
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert_eq!(viewport_frozen(&d), [vec!["Walls".to_string()]], "VPFROZEN");
    assert_eq!(d.text_style("Body").map(|s| s.font.as_str()), Some("Legacy Face"), "FONT");
    assert_eq!(d.header.str("CMLEADERSTYLE", ""), "Notes", "CADCRAFT_MLEADERSTYLE");
    // And the next save writes the native homes only.
    assert_no_private_copy(&parse(cadcraft_io::write_with(&d, "x.dxf", &options).unwrap().as_slice()).unwrap(), "re-save");
}

/// The first release with the native gradient groups (450-470) is R2004; an older DXF and a DWG R2000 hold the gradient only
/// as `CADCRAFT` xdata, and reopen with it.
#[test]
fn exception_gradient_before_r2004_stays_xdata() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(None))).unwrap();
    for (name, version) in [("x.dxf", Version::R2000), ("x.dwg", Version::R2000)] {
        let (bytes, tags) = saved(&d, name, version);
        assert_eq!(private_xdata(&tags), ["HATCH"], "{name}");
        assert!(appid_registered(&tags), "{name}");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        let g = back.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { h.gradient.clone() } else { None }).expect("gradient");
        assert_eq!((g.name.as_str(), g.color1, g.color2), ("SPHERICAL", gradient().color1, gradient().color2), "{name}");
    }
}

/// The DXF Reference has no HATCH group for the background colour: it stays xdata at every release.
#[test]
fn exception_hatch_background_stays_xdata() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(Some(Color::True(Rgb(1, 2, 3)))))).unwrap();
    for (name, version) in [("x.dxf", Version::R2018), ("x.dwg", Version::R2018)] {
        let (bytes, tags) = saved(&d, name, version);
        assert_eq!(private_xdata(&tags), ["HATCH"], "{name}");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        let h = back.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).expect("hatch");
        assert_eq!(h.background, Some(Color::True(Rgb(1, 2, 3))), "{name}");
        assert!(h.gradient.is_some(), "{name}");
    }
}

/// A viewport's layer colour overrides (VP Color) have no group in the DXF Reference.
#[test]
fn exception_viewport_layer_colors_stay_xdata() {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Walls"));
    let EntityKind::Viewport(mut v) = viewport(2, &[]) else { unreachable!() };
    v.layer_colors = vec![("Walls".into(), Color::Index(1))];
    d.add(&Space::Paper("Layout1".into()), Common::default(), viewport(1, &[])).unwrap();
    d.add(&Space::Paper("Layout1".into()), Common::default(), EntityKind::Viewport(v)).unwrap();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, tags) = saved(&d, name, Version::R2018);
        assert_eq!(private_xdata(&tags), ["VIEWPORT"], "{name}");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        let colors: Vec<_> = back
            .layouts
            .iter()
            .flat_map(|l| l.entities.iter())
            .filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { Some(v.layer_colors.clone()) } else { None })
            .flatten()
            .collect();
        assert_eq!(colors, [("Walls".to_string(), Color::Index(1))], "{name}");
    }
}

/// A layout without a sheet viewport (id 1) gets the one AutoCAD writes. CADCraft's model keeps it implicit, so the file marks
/// it with `CADCRAFT` xdata for the reader to leave it out again: no group says that a sheet viewport was added.
#[test]
fn exception_the_implicit_sheet_viewport_stays_marked() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Paper("Layout1".into()), Common::default(), viewport(2, &[])).unwrap();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, tags) = saved(&d, name, Version::R2018);
        assert_eq!(private_xdata(&tags), ["VIEWPORT"], "{name}");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        let ids: Vec<u32> = back
            .layouts
            .iter()
            .flat_map(|l| l.entities.iter())
            .filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { Some(v.id) } else { None })
            .collect();
        assert_eq!(ids, [2], "{name}: the sheet viewport stays implicit");
    }
}

/// The DWG writer neither writes nor reads the header handle of the current multileader style: a DWG keeps it in an XRECORD,
/// and so does a DXF before R2007, which has no `$CMLEADERSTYLE`.
#[test]
fn exception_the_current_multileader_style_in_a_dwg_and_in_an_old_dxf() {
    let d = fixture();
    for (name, version) in [("x.dwg", Version::R2018), ("x.dwg", Version::R2004), ("x.dxf", Version::R2004)] {
        let (bytes, tags) = saved(&d, name, version);
        assert_eq!(private_xrecords(&tags), ["CADCRAFT_MLEADERSTYLE"], "{name} {version:?}");
        assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Notes", "{name} {version:?}");
    }
    // A DXF from R2007 holds it in the header.
    let (_, tags) = saved(&d, "x.dxf", Version::R2007);
    assert!(private_xrecords(&tags).is_empty());
    assert!(tags.windows(2).any(|w| w[0].code == 9 && w[0].str() == "$CMLEADERSTYLE" && w[1].str() == "Notes"));
}

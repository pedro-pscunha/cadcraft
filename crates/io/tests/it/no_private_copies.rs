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

/// The hatch background of the fixture: AutoCAD packs it as 0xC2 (RGB) then the colour.
const BACKGROUND: Color = Color::True(Rgb(1, 2, 3));
const BACKGROUND_PACKED: &str = "-1040121341";

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
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(Some(BACKGROUND)))).unwrap();
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
    assert_eq!(hatch.background, Some(BACKGROUND), "{what}: hatch background");
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
    // The multileader style: an XRECORD in the named object dictionary, and no variable dictionary.
    assert!(tags.iter().all(|t| !(t.code == 9 && t.str() == "$CMLEADERSTYLE")), "a DXF header has no $CMLEADERSTYLE");
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
    // The hatch: AutoCAD's background xdata gone, the `HATCH` xdata of an older CADCraft (origin, background name) in its place.
    let hatch = (0..tags.len()).find(|&i| tags[i].code == 0 && tags[i].str() == "HATCH").unwrap();
    let end = record_end(&tags, hatch);
    let xdata = tags[hatch..end].iter().position(|t| t.code == 1001 && t.str() == "HATCHBACKGROUNDCOLOR").unwrap() + hatch;
    tags.splice(
        xdata..xdata + 4,
        [Tag::s(1001, "CADCRAFT"), Tag::s(1000, "HATCH"), Tag::f(1011, 0.0), Tag::f(1021, 0.0), Tag::f(1031, 0.0), Tag::s(1000, "1,2,3")],
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

/// The background of a hatch has no group in the DXF Reference: AutoCAD keeps it in the xdata of the application
/// `HATCHBACKGROUNDCOLOR` (`1071` the packed colour, then two empty strings), which a save writes at every release.
#[test]
fn a_hatch_background_is_written_as_autocads_xdata() {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(Some(BACKGROUND)))).unwrap();
    for (name, version) in [("x.dxf", Version::R2018), ("x.dwg", Version::R2018), ("x.dxf", Version::R2000), ("x.dwg", Version::R2000)] {
        let (bytes, tags) = saved(&d, name, version);
        let at = tags.iter().position(|t| t.code == 1001 && t.str() == "HATCHBACKGROUNDCOLOR").expect("AutoCAD's background xdata");
        let run: Vec<(i32, String)> = tags[at + 1..].iter().take(3).map(|t| (t.code, t.str())).collect();
        assert_eq!(run, [(1071, BACKGROUND_PACKED.to_string()), (1000, String::new()), (1000, String::new())], "{name} {version:?}");
        let background_apps = tags.iter().filter(|t| t.code == 1001 && t.str() == "HATCHBACKGROUNDCOLOR").count();
        assert_eq!(background_apps, 1, "{name} {version:?}");
        let xdata: Vec<String> = private_xdata(&tags);
        assert!(xdata.iter().all(|kind| kind != "HATCH") || version < Version::R2004, "{name} {version:?}: CADCRAFT copy of the background");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        let h = back.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).expect("hatch");
        assert_eq!(h.background, Some(BACKGROUND), "{name} {version:?}");
    }
}

/// A background AutoCAD wrote stays in the file as it was: its place among the other applications' xdata and its value, an
/// edit changes the one copy, and a removed background becomes AutoCAD's "no colour" (0xC8).
#[test]
fn a_hatch_background_read_from_autocad_is_one_copy_that_follows_the_edits() {
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(Some(BACKGROUND)))).unwrap();
    let mut tags = parse(&cadcraft_io::write_with(&d, "x.dxf", &options).unwrap()).unwrap();
    // The file's value: the colour index 5 (0xC3000005).
    let at = tags.iter().position(|t| t.code == 1071).unwrap();
    tags[at] = Tag::i(1071, -1023410171);
    let file = cadcraft_dxf::write_ascii(&tags);
    let packed = |bytes: &[u8]| -> Vec<String> {
        let tags = parse(bytes).unwrap();
        tags.iter().filter(|t| t.code == 1071).map(Tag::str).collect()
    };
    let hatch_of = |d: &Drawing| d.model.iter().find(|e| matches!(e.kind, EntityKind::Hatch(_))).map(|e| e.as_ref().clone()).unwrap();
    let background_of = |e: &Entity| if let EntityKind::Hatch(h) = &e.kind { h.background } else { None };
    let mut read = cadcraft_io::read_dxf(file.as_bytes()).unwrap();
    assert_eq!(background_of(&hatch_of(&read)), Some(Color::Index(5)));
    let saved_again = cadcraft_io::write_with(&read, "x.dxf", &options).unwrap();
    assert_eq!(packed(&saved_again), ["-1023410171"], "unedited");
    assert!(!private_xdata(&parse(&saved_again).unwrap()).contains(&"HATCH".to_string()));
    // An edit.
    let mut e = hatch_of(&read);
    let EntityKind::Hatch(h) = &mut e.kind else { unreachable!() };
    h.background = Some(Color::Index(2));
    read.model.replace(e.clone());
    assert_eq!(packed(&cadcraft_io::write_with(&read, "x.dxf", &options).unwrap()), ["-1023410174"], "edited");
    // A removal.
    let EntityKind::Hatch(h) = &mut e.kind else { unreachable!() };
    h.background = None;
    read.model.replace(e);
    assert_eq!(packed(&cadcraft_io::write_with(&read, "x.dxf", &options).unwrap()), ["-939524096"], "removed");
}

/// A viewport's layer colour overrides (VP Color) have no group in the DXF Reference, and no AutoCAD file at hand holds AutoCAD's
/// form of them (the `ADSK_XREC_LAYER_RECONCILED` xrecords of the layer dictionaries are the layer's reconciled flag, not an
/// override), so the overrides CADCraft adds stay `CADCRAFT` xdata until a file shows the native record.
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

/// A layout that came from a file keeps its sheet viewport (id 1) and writes no marker: `PAPERVIEW` and `PSVIEW` appear only
/// for a sheet viewport CADCraft adds (the next test). An unedited save of AutoCAD's Citycon, DPInc and Toulouse drawings writes
/// none of either, in the DXF at R2018 and in the DWG.
#[test]
fn a_layout_read_from_a_file_gets_no_paper_marker_on_save() {
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    for name in ["x.dxf", "x.dwg"] {
        let once = cadcraft_io::write_with(&fixture(), name, &options).unwrap();
        let twice = cadcraft_io::write_with(&cadcraft_io::read(&once, name).unwrap(), name, &options).unwrap();
        let tags = parse(&if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&twice).unwrap() } else { twice }).unwrap();
        let markers: Vec<String> =
            tags.iter().filter(|t| t.code == 1000 && matches!(t.str().as_str(), "PAPERVIEW" | "PSVIEW")).map(Tag::str).collect();
        assert!(markers.is_empty(), "{name}: {markers:?}");
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

/// A release before R2004 has no variable dictionary for the current multileader style (AutoCAD's DXF has no `$CMLEADERSTYLE`, its
/// DWG header no field for it), so the style stays an XRECORD there. From R2004 it is the variable (see below).
#[test]
fn exception_the_current_multileader_style_before_r2004_stays_an_xrecord() {
    let d = fixture();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, tags) = saved(&d, name, Version::R2000);
        assert_eq!(private_xrecords(&tags), ["CADCRAFT_MLEADERSTYLE"], "{name}");
        assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Notes", "{name}");
    }
}

/// A file as AutoCAD writes it, with the variable dictionary: the current multileader style `style` (none: the dictionary has other
/// variables only).
fn autocad_variables_file(style: Option<&str>) -> String {
    let entry = if style.is_some() { "3\nCMLEADERSTYLE\n350\n2F8D\n" } else { "" };
    let variable = style.map_or(String::new(), |s| {
        format!("0\nDICTIONARYVAR\n5\n2F8D\n102\n{{ACAD_REACTORS\n330\n322\n102\n}}\n330\n322\n100\nDictionaryVariables\n280\n0\n1\n{s}\n")
    });
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n50000\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n100\n8\n0\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nAcDbVariableDictionary\n350\n322\n\
         0\nDICTIONARY\n5\n322\n102\n{{ACAD_REACTORS\n330\nC\n102\n}}\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nCANNOSCALE\n350\n2F8F\n{entry}\
         0\nDICTIONARYVAR\n5\n2F8F\n102\n{{ACAD_REACTORS\n330\n322\n102\n}}\n330\n322\n100\nDictionaryVariables\n280\n0\n1\n1:1\n{variable}\
         0\nENDSEC\n0\nEOF\n"
    )
}

/// The value of the CMLEADERSTYLE variable and the number of them (and of `$CMLEADERSTYLE` header variables) in a DXF.
fn mleader_variable(tags: &[Tag]) -> (Vec<String>, usize) {
    let values = sections(tags)
        .into_iter()
        .flat_map(|s| records(&s.tags))
        .filter(|(kind, body)| kind == "DICTIONARYVAR" && body.iter().any(|t| t.code == 330))
        .filter_map(|(_, body)| {
            let handle = body.iter().find(|t| t.code == 5)?.str();
            let owner_entry = tags.windows(2).any(|w| w[0].code == 3 && w[0].str() == "CMLEADERSTYLE" && w[1].str() == handle);
            owner_entry.then(|| body.iter().find(|t| t.code == 1).map(|t| t.str()).unwrap_or_default())
        })
        .collect();
    (values, tags.iter().filter(|t| t.code == 9 && t.str() == "$CMLEADERSTYLE").count())
}

/// From R2004 the current multileader style is the variable CMLEADERSTYLE of the variable dictionary, in the DXF and in the DWG, and
/// the DXF has no `$CMLEADERSTYLE` and no `CADCRAFT_MLEADERSTYLE` XRECORD: AutoCAD writes neither.
#[test]
fn the_current_multileader_style_is_the_variable_from_r2004() {
    let d = fixture();
    for name in ["x.dxf", "x.dwg"] {
        for version in [Version::R2004, Version::R2007, Version::R2018] {
            let (bytes, tags) = saved(&d, name, version);
            let (values, header) = mleader_variable(&tags);
            assert_eq!(values, ["Notes"], "{name} {version:?}: the variable");
            // The DXF rendition of a DWG is acadrust's, which writes the header variable.
            assert!(header == 0 || name.ends_with(".dwg"), "{name} {version:?}: $CMLEADERSTYLE");
            assert!(private_xrecords(&tags).is_empty(), "{name} {version:?}: {:?}", private_xrecords(&tags));
            assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Notes", "{name} {version:?}");
        }
    }
}

/// A file whose variable dictionary names a style keeps it through a save in the DXF and the DWG, a switch to Standard writes
/// Standard (the header cannot tell it from a name never set), and a variable dictionary without the variable gets one.
#[test]
fn the_current_multileader_style_of_an_autocad_file_follows_the_drawing() {
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    for name in ["x.dxf", "x.dwg"] {
        let mut d = cadcraft_io::read_dxf(autocad_variables_file(Some("Linha_MEDIO")).as_bytes()).unwrap();
        assert_eq!(d.header.str("CMLEADERSTYLE", ""), "Linha_MEDIO", "{name}: read");
        let kept = cadcraft_io::write_with(&d, name, &options).unwrap();
        assert_eq!(cadcraft_io::read(&kept, name).unwrap().header.str("CMLEADERSTYLE", ""), "Linha_MEDIO", "{name}: unedited");
        d.header.set_str("CMLEADERSTYLE", "Standard");
        let (bytes, tags) = saved(&d, name, Version::R2018);
        assert_eq!(mleader_variable(&tags).0, ["Standard"], "{name}: the variable of the switch");
        assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Standard", "{name}: switched");
        // A variable dictionary without the variable.
        let mut d = cadcraft_io::read_dxf(autocad_variables_file(None).as_bytes()).unwrap();
        assert_eq!(d.header.str("CMLEADERSTYLE", ""), "Standard", "{name}: none");
        d.header.set_str("CMLEADERSTYLE", "Notes");
        let (bytes, tags) = saved(&d, name, Version::R2018);
        assert_eq!(mleader_variable(&tags).0, ["Notes"], "{name}: a variable made");
        assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Notes", "{name}: made");
    }
}

/// Citycon keeps `Linha_MEDIO` as its current multileader style (a DICTIONARYVAR in the AutoCAD DWG and in AutoCAD's own DXF): it stays
/// through a read and a save as DXF and as DWG.
#[test]
#[ignore = "reads the AutoCAD drawings of /tmp/cad (Citycon, a DWG and AutoCAD's own R2018 DXF)"]
fn citycon_keeps_its_current_multileader_style() {
    for source in ["/tmp/cad/rt7/Citycon-Augusto_Miele.dwg", "/tmp/cad/save-compare/acad/Citycon-Augusto_Miele-2018.acad-saveas.dxf"] {
        let d = cadcraft_io::read(&std::fs::read(source).expect("the Citycon drawing"), source).unwrap();
        assert_eq!(d.header.str("CMLEADERSTYLE", ""), "Linha_MEDIO", "{source}: read");
        for name in ["x.dxf", "x.dwg"] {
            for version in [Version::R2010, Version::R2018] {
                let (bytes, _) = saved(&d, name, version);
                assert_eq!(cadcraft_io::read(&bytes, name).unwrap().header.str("CMLEADERSTYLE", ""), "Linha_MEDIO", "{source} {name} {version:?}");
            }
        }
    }
}

/// The other private data stays where the DXF Reference and the DWG have no group for it: a named view's layer state and layout,
/// a layout's saved paper-space view, an arc-length dimension's centre and a dimension's jog line. Each is written alone,
/// and reopens.
#[test]
fn exception_the_rest_of_the_private_data_has_no_native_home() {
    let mut d = Drawing::new_metric();
    d.views.push(NamedView {
        name: "Plan".into(),
        center: Vec2::new(5.0, 5.0),
        height: 10.0,
        width: 14.0,
        layer_state: Some("Walls only".into()),
        twist: 0.0,
        layout: None,
    });
    d.layouts[0].view = Some((Vec2::new(5.0, 4.0), 12.0));
    d.add(&Space::Paper("Layout1".into()), Common::default(), viewport(1, &[])).unwrap();
    let mut arc = match fixture().model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm.clone()) } else { None }) {
        Some(dm) => dm,
        None => unreachable!(),
    };
    arc.assoc.clear();
    arc.kind = DimKind::ArcLength;
    arc.p15 = Vec3::new(5.0, -4.0, 0.0);
    arc.marks.jog_line = Some(Vec3::new(5.0, 3.0, 0.0));
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(arc)).unwrap();
    for name in ["x.dxf", "x.dwg"] {
        let (bytes, tags) = saved(&d, name, Version::R2018);
        let mut kinds: Vec<String> = private_xdata(&tags).into_iter().filter(|k| k != "TABLESTYLE").collect();
        kinds.sort();
        assert_eq!(kinds, ["DIMENSION", "LAYOUT", "VIEW"], "{name}");
        let back = cadcraft_io::read(&bytes, name).unwrap();
        assert_eq!(back.views.first().and_then(|v| v.layer_state.as_deref()), Some("Walls only"), "{name}: view layer state");
        assert_eq!(back.layouts[0].view, Some((Vec2::new(5.0, 4.0), 12.0)), "{name}: paper-space view");
        let dim = back.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm.clone()) } else { None }).unwrap();
        assert_eq!(dim.kind, DimKind::ArcLength, "{name}");
        assert_eq!((dim.p15, dim.marks.jog_line), (Vec3::new(5.0, -4.0, 0.0), Some(Vec3::new(5.0, 3.0, 0.0))), "{name}");
    }
}

/// A file as AutoCAD writes it: the entity is low, the highest handle is an object (an XRECORD under a key CADCraft keeps raw), and
/// `$HANDSEED` is `seed` (above every handle, or, for a hostile file, below them).
fn autocad_file(seed: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n{seed}\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n100\n8\n0\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nKEEP_ME\n350\n4FFF\n\
         0\nXRECORD\n5\n4FFF\n330\nC\n100\nAcDbXrecord\n280\n1\n1\nx\n0\nENDSEC\n0\nEOF\n"
    )
}

/// Every record handle of a DXF (groups 5 and 105 outside the header).
fn record_handles(tags: &[Tag]) -> Vec<u64> {
    sections(tags)
        .into_iter()
        .filter(|s| s.name != "HEADER")
        .flat_map(|s| records(&s.tags))
        .filter_map(|(_, body)| body.iter().find(|t| matches!(t.code, 5 | 105)).and_then(|t| Handle::parse_hex(&t.str())).map(|h| h.0))
        .collect()
}

fn header_seed(tags: &[Tag]) -> u64 {
    tags.windows(2).find(|w| w[0].code == 9 && w[0].str() == "$HANDSEED").and_then(|w| Handle::parse_hex(&w[1].str())).map_or(0, |h| h.0)
}

/// A new entity takes a handle from the file's `$HANDSEED`, which AutoCAD keeps above every handle it ever gave (a deleted object's
/// handle is not used again), and the saved file has no handle twice, with `$HANDSEED` above them all. The same when the file's
/// seed is wrong (below its handles): the new handle is above the handles used.
#[test]
fn a_new_entity_takes_its_handle_from_the_files_next_handle() {
    for (seed, floor) in [("5000", 0x5000), ("10", 0x4FFF + 1)] {
        let mut d = cadcraft_io::read_dxf(autocad_file(seed).as_bytes()).unwrap();
        let line = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(3.0, 3.0, 0.0) })).unwrap();
        assert!(line.0 >= floor, "seed {seed}: the new entity has {line:?}");
        for name in ["x.dxf", "x.dwg"] {
            let (_, tags) = saved(&d, name, Version::R2018);
            let handles = record_handles(&tags);
            let mut unique = handles.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), handles.len(), "seed {seed} {name}: a handle is used twice");
            assert!(handles.contains(&line.0), "seed {seed} {name}: the new entity keeps its handle");
            assert!(header_seed(&tags) > unique.last().copied().unwrap_or(0), "seed {seed} {name}: $HANDSEED is above every handle written");
            if seed == "5000" && name == "x.dxf" {
                assert!(header_seed(&tags) >= 0x5000, "the file's own next handle is kept");
            }
        }
    }
}

/// The second save of a file AutoCAD wrote (with an entity added) is the third: the numbers CADCraft gave on the first save are
/// the ones it reads back, in the DXF and in the DWG, also when the file's `$HANDSEED` was wrong.
#[test]
fn a_save_of_an_autocad_file_is_the_same_file_again() {
    let options = SaveOptions { version: Some(Version::R2018), binary: false };
    for seed in ["5000", "10"] {
        for name in ["x.dxf", "x.dwg"] {
            let mut d = cadcraft_io::read_dxf(autocad_file(seed).as_bytes()).unwrap();
            d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(3.0, 3.0, 0.0) })).unwrap();
            let once = cadcraft_io::write_with(&d, name, &options).unwrap();
            let twice = cadcraft_io::write_with(&cadcraft_io::read(&once, name).unwrap(), name, &options).unwrap();
            let thrice = cadcraft_io::write_with(&cadcraft_io::read(&twice, name).unwrap(), name, &options).unwrap();
            assert!(twice == thrice, "seed {seed} {name}: the second save differs from the third");
        }
    }
}

//! The source boundary objects of an associative HATCH (group 97 count, 330 handles per loop,
//! DXF Reference) survive a DXF save and read (#451).

use cadcraft_doc::*;
use cadcraft_geom::{PolyVertex, Vec2};
use cadcraft_io::{read_dxf, write_dxf};

fn square(x0: f64, s: f64) -> Vec<PolyVertex> {
    [(x0, x0), (x0 + s, x0), (x0 + s, x0 + s), (x0, x0 + s)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect()
}

fn hatch(loops: Vec<HatchLoop>, associative: bool) -> Hatch {
    Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops,
        scale: 1.0,
        angle: 0.0,
        associative,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
    }
}

fn poly(d: &mut Drawing, vs: Vec<PolyVertex>) -> Handle {
    let k = EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: vs,
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
        heavy: false,
        fit: None,
    });
    d.add(&Space::Model, Common::default(), k).expect("add")
}

fn hatches(d: &Drawing) -> Vec<Hatch> {
    d.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect()
}

#[test]
fn an_imported_forward_hatch_link_does_not_invent_a_missing_reverse_reactor() {
    let mut drawing = Drawing::new_metric();
    let boundary = poly(&mut drawing, square(0.0, 10.0));
    let loops = vec![HatchLoop { vertices: square(0.0, 10.0), outer: true, sources: vec![boundary], ..Default::default() }];
    let first = drawing.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops.clone(), true))).unwrap();
    let second = drawing.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops.clone(), true))).unwrap();
    let mut tags = cadcraft_dxf::parse(write_dxf(&drawing).as_bytes()).unwrap();
    let at = tags.iter().position(|tag| tag.code == 5 && tag.str() == boundary.hex()).unwrap();
    let end = at + tags[at..].iter().position(|tag| tag.code == 0).unwrap();
    let removed = (at..end).find(|index| tags[*index].code == 330 && tags[*index].str() == second.hex()).unwrap();
    tags.remove(removed);
    let source = cadcraft_dxf::write_ascii(&tags);
    let imported = read_dxf(source.as_bytes()).unwrap();
    let reactors = |text: &str, handle: Handle| {
        let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
        let body = cadcraft_dxf::sections(&tags)
            .into_iter()
            .filter(|section| section.name == "ENTITIES")
            .flat_map(|section| cadcraft_dxf::records(&section.tags))
            .find(|(_, tags)| tags.iter().any(|tag| tag.code == 5 && tag.str() == handle.hex()))
            .unwrap()
            .1;
        let Some(open) = body.iter().position(|tag| tag.code == 102 && tag.str() == "{ACAD_REACTORS") else { return Vec::new() };
        body[open + 1..].iter().take_while(|tag| tag.code != 102).filter(|tag| tag.code == 330).map(cadcraft_dxf::Tag::str).collect::<Vec<_>>()
    };
    let mut edited = imported.clone();
    let added = edited.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops, true))).unwrap();
    let saved = write_dxf(&edited);
    assert_eq!(reactors(&saved, boundary), vec![first.hex(), added.hex()]);
    let other = poly(&mut edited, square(20.0, 10.0));
    edited
        .modify_entity(second, |entity| {
            let EntityKind::Hatch(hatch) = &mut entity.kind else { panic!("HATCH") };
            hatch.loops[0].sources = vec![other];
        })
        .unwrap();
    assert_eq!(reactors(&write_dxf(&edited), other), vec![second.hex()], "a changed forward link gets its new reverse link");
    eprintln!("CAD_ASSOCIATION_CONTROLS=pass");
    for (what, text) in ["DXF", "DWG"].into_iter().zip(super::foreign_passthrough::saved_twice(&imported)) {
        assert_eq!(reactors(&text, boundary), vec![first.hex()], "{what}: preserve the source's asymmetric relationship");
        let reopened = read_dxf(text.as_bytes()).unwrap();
        let EntityKind::Hatch(second_hatch) = &reopened.entity(second).unwrap().kind else { panic!("HATCH") };
        assert_eq!(second_hatch.loops[0].sources, vec![boundary], "the forward reference must remain");
    }
}

#[test]
fn source_boundary_objects_roundtrip() {
    let mut d = Drawing::new_imperial();
    let outer = poly(&mut d, square(0.0, 20.0));
    let island = poly(&mut d, square(5.0, 5.0));
    let loops = vec![
        HatchLoop { vertices: square(0.0, 20.0), outer: true, sources: vec![outer], ..Default::default() },
        HatchLoop { vertices: square(5.0, 5.0), outer: false, sources: vec![island], ..Default::default() },
    ];
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops.clone(), true))).expect("add");
    // A non-associative hatch writes no sources; a source that is gone isn't written.
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(vec![HatchLoop { sources: vec![outer], ..loops[0].clone() }], false)))
        .expect("add");
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(vec![HatchLoop { sources: vec![Handle(0xFFFF)], ..loops[0].clone() }], true)))
        .expect("add");
    let text = write_dxf(&d);
    let back = read_dxf(text.as_bytes()).expect("read");
    let hs = hatches(&back);
    assert_eq!(hs.len(), 3);
    assert!(hs[0].associative);
    assert_eq!(hs[0].loops.iter().map(|l| l.sources.clone()).collect::<Vec<_>>(), vec![vec![outer], vec![island]]);
    assert!(hs[1].loops[0].sources.is_empty());
    assert!(hs[2].loops[0].sources.is_empty());
    // Saved again, the links stay.
    let again = read_dxf(write_dxf(&back).as_bytes()).expect("read");
    assert_eq!(hatches(&again)[0].loops, hs[0].loops);
}

/// (group code, value) pairs of a DXF text.
fn pairs(text: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = text.lines().collect();
    lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim().to_string()))).collect()
}

fn join(p: &[(i32, String)]) -> String {
    p.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// A HATCH whose edge boundary has a spline edge with fit data (its own 97 count) followed by
/// the source objects, and hostile counts and handles.
#[test]
fn sources_after_spline_fit_data_and_hostile_counts() {
    let mut d = Drawing::new_imperial();
    let outer = poly(&mut d, square(0.0, 20.0));
    let loops = vec![HatchLoop { vertices: square(0.0, 20.0), outer: true, sources: vec![outer], ..Default::default() }];
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops, true))).expect("add");
    let p = pairs(&write_dxf(&d));
    let start = p.iter().position(|(c, v)| *c == 0 && v == "HATCH").expect("HATCH");
    let at = start + p.iter().skip(start).position(|(c, _)| *c == 92).expect("92");
    let end = at + p.iter().skip(at).position(|(c, _)| *c == 97).expect("97");
    assert_eq!(p.get(end + 1), Some(&(330, outer.hex())), "sources written after the boundary");
    let with = |boundary: &str, sources: &str| {
        let mut q: Vec<(i32, String)> = p[..at].to_vec();
        q.extend(pairs(boundary));
        q.extend(pairs(sources));
        q.extend(p[end + 2..].iter().cloned());
        read_dxf(join(&q).as_bytes()).expect("read")
    };
    let src = format!("97\n1\n330\n{}\n", outer.hex());
    // An edge boundary: one spline edge with two fit points.
    let spline = "92\n1\n93\n1\n72\n4\n94\n3\n73\n0\n74\n0\n95\n8\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n96\n4\n10\n0\n20\n0\n10\n20\n20\n0\n10\n20\n20\n20\n10\n0\n20\n20\n97\n2\n11\n0\n21\n0\n11\n0\n21\n20\n";
    assert_eq!(hatches(&with(spline, &src))[0].loops[0].sources, vec![outer]);
    let poly_loop = join(&p[at..end]);
    // Hostile: a huge count, unparsable and zero handles; a negative count.
    let back = with(&poly_loop, "97\n999999999999\n330\nZZZ\n330\n0\n330\n1F\n");
    assert_eq!(hatches(&back)[0].loops[0].sources, vec![Handle(0x1F)]);
    assert!(hatches(&with(&poly_loop, "97\n-5\n"))[0].loops[0].sources.is_empty());
}

/// The groups of the first record of type `kind` with handle `h`.
fn record(p: &[(i32, String)], kind: &str, h: Handle) -> Vec<(i32, String)> {
    let starts: Vec<usize> = p.iter().enumerate().filter(|(_, (c, _))| *c == 0).map(|(i, _)| i).collect();
    starts
        .iter()
        .enumerate()
        .map(|(n, &s)| p[s..starts.get(n + 1).copied().unwrap_or(p.len())].to_vec())
        .find(|r| r.first().is_some_and(|(_, v)| v == kind) && r.get(1) == Some(&(5, h.hex())))
        .unwrap_or_default()
}

/// An associative hatch inside a block keeps its boundary object, and the boundary carries the
/// hatch as a persistent reactor (as AutoCAD writes them), so a stretched boundary still drags
/// the fill along in AutoCAD (dynamic blocks).
#[test]
fn a_hatch_in_a_block_keeps_its_boundary_and_the_boundary_its_reactor() {
    let mut d = Drawing::new_imperial();
    let mut b = Block::new("Tag");
    let outline = d.new_handle();
    let fill = d.new_handle();
    let k = EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: square(0.0, 10.0),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
        heavy: false,
        fit: None,
    });
    b.entities.push(Entity::new(outline, k));
    let loops = vec![HatchLoop { vertices: square(0.0, 10.0), outer: true, sources: vec![outline], ..Default::default() }];
    b.entities.push(Entity::new(fill, EntityKind::Hatch(hatch(loops, true))));
    d.blocks.insert("Tag".into(), std::sync::Arc::new(b));
    // A model-space hatch whose source is in the block is not linked to it.
    let stray = vec![HatchLoop { vertices: square(0.0, 10.0), outer: true, sources: vec![outline], ..Default::default() }];
    let other = d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(stray, true))).expect("add");
    let p = pairs(&write_dxf(&d));
    let poly = record(&p, "LWPOLYLINE", outline);
    let reactors = ["{ACAD_REACTORS", &fill.hex(), "}"].map(String::from);
    assert!(poly.windows(3).any(|w| w.iter().map(|(_, v)| v.clone()).eq(reactors.iter().cloned())), "{poly:?}");
    assert!(!poly.iter().any(|(c, v)| *c == 330 && *v == other.hex()), "{poly:?}");
    let h = record(&p, "HATCH", fill);
    let at = h.iter().position(|(c, _)| *c == 97).expect("97");
    assert_eq!(h[at..at + 2], [(97, "1".to_string()), (330, outline.hex())]);
    assert!(record(&p, "HATCH", other).contains(&(97, "0".to_string())));
    // Read back, from DXF and from DWG.
    let back = read_dxf(join(&p).as_bytes()).expect("read");
    let dwg = cadcraft_io::write(&d, "x.dwg").expect("dwg");
    let from_dwg = cadcraft_io::read(&dwg, "x.dwg").expect("read dwg");
    for (what, back) in [("dxf", &back), ("dwg", &from_dwg)] {
        let blk = back.block("Tag").expect("block");
        let sources: Vec<Vec<Handle>> = blk
            .entities
            .iter()
            .filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.loops.iter().flat_map(|l| l.sources.clone()).collect()) } else { None })
            .collect();
        assert_eq!(sources, vec![vec![outline]], "{what}");
    }
}

#[test]
fn a_polyline_that_is_a_hatch_boundary_and_a_dimension_link_lists_both_reactors() {
    let mut d = Drawing::new_imperial();
    let outer = poly(&mut d, square(0.0, 20.0));
    let loops = vec![HatchLoop { vertices: square(0.0, 20.0), outer: true, sources: vec![outer], ..Default::default() }];
    let hatch = d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops, true))).expect("add");
    let links = vec![
        DimAssoc { point: "p13".into(), handle: outer, snap: AssocSnap::Start },
        DimAssoc { point: "p14".into(), handle: outer, snap: AssocSnap::Vertex { index: 2 } },
    ];
    let dimension = Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: geom_vec(20.0, 0.0),
        text_mid: geom_vec(0.0, 0.0),
        p13: geom_vec(0.0, 0.0),
        p14: geom_vec(20.0, 20.0),
        p15: geom_vec(0.0, 0.0),
        p16: geom_vec(0.0, 0.0),
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
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension)).expect("add");
    let text = write_dxf(&d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 5 && t.str() == outer.hex()).unwrap();
    let body: Vec<_> = tags[start..].iter().take_while(|t| t.code != 0).collect();
    let open = body.iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
    let reactors: Vec<String> = body[open + 1..].iter().take_while(|t| t.code == 330).map(|t| t.str()).collect();
    assert!(reactors.contains(&hatch.hex()), "the hatch: {reactors:?}");
    assert_eq!(reactors.len(), 2, "the hatch and the DIMASSOC: {reactors:?}");
}

fn geom_vec(x: f64, y: f64) -> cadcraft_geom::Vec3 {
    cadcraft_geom::Vec3::new(x, y, 0.0)
}

#[test]
fn a_hatch_boundary_keeps_the_reactors_the_file_had() {
    let mut groups: Vec<(i32, &str)> = vec![(0, "SECTION"), (2, "ENTITIES"), (0, "LWPOLYLINE"), (5, "30")];
    groups.extend([(102, "{ACAD_REACTORS"), (330, "50"), (102, "}"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbPolyline"), (90, "4"), (70, "1")]);
    groups.extend([(10, "0"), (20, "0"), (10, "20"), (20, "0"), (10, "20"), (20, "20"), (10, "0"), (20, "20")]);
    groups.extend([(0, "ENDSEC"), (0, "SECTION"), (2, "OBJECTS"), (0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary")]);
    groups.extend([(3, "KEEPME"), (350, "50"), (0, "XRECORD"), (5, "50"), (330, "C"), (100, "AcDbXrecord"), (280, "1"), (1, "x")]);
    groups.extend([(0, "ENDSEC"), (0, "EOF")]);
    let text: String = groups.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect();
    let mut d = read_dxf(text.as_bytes()).unwrap();
    let outer = d.model.iter().find(|e| matches!(e.kind, EntityKind::LwPolyline(_))).map(|e| e.handle).unwrap();
    let loops = vec![HatchLoop { vertices: square(0.0, 20.0), outer: true, sources: vec![outer], ..Default::default() }];
    let hatch = d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops, true))).expect("add");
    let saved = write_dxf(&d);
    let tags = cadcraft_dxf::parse(saved.as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 5 && t.str() == outer.hex()).unwrap();
    let body: Vec<_> = tags[start..].iter().take_while(|t| t.code != 0).collect();
    let open = body.iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
    let reactors: Vec<String> = body[open + 1..].iter().take_while(|t| t.code == 330).map(|t| t.str()).collect();
    assert!(reactors.contains(&hatch.hex()), "the hatch: {reactors:?}");
    assert_eq!(reactors.len(), 2, "the hatch and the file's own: {reactors:?}");
}

/// An associative hatch whose loop is a spline edge keeps its source object through a DWG: the bridge's DXF has the fit data count
/// (97) that acadrust takes the loop's own count for otherwise, and the association was dropped.
#[test]
fn a_spline_edge_hatch_keeps_its_source_in_a_dwg() {
    let spline = "0\nSPLINE\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbSpline\n70\n8\n71\n3\n72\n8\n73\n4\n74\n0\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
        10\n0\n20\n0\n30\n0\n10\n20\n20\n0\n30\n0\n10\n20\n20\n20\n30\n0\n10\n0\n20\n20\n30\n0\n";
    let knots = "40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n";
    let hatch = format!(
        "0\nHATCH\n5\n41\n100\nAcDbEntity\n8\n0\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n1\n91\n1\n92\n1\n93\n1\n72\n4\n94\n3\n73\n0\n74\n0\n95\n8\n96\n4\n{knots}\
         10\n0\n20\n0\n10\n20\n20\n0\n10\n20\n20\n20\n10\n0\n20\n20\n97\n0\n97\n1\n330\n40\n75\n0\n76\n1\n98\n0\n"
    );
    let text = format!("0\nSECTION\n2\nENTITIES\n{spline}{hatch}0\nENDSEC\n0\nEOF\n");
    let d = read_dxf(text.as_bytes()).expect("read");
    assert_eq!(hatches(&d)[0].loops[0].sources, vec![Handle(0x40)], "read from the DXF");
    let dwg = cadcraft_io::write(&d, "x.dwg").expect("dwg");
    let from_dwg = cadcraft_io::read(&dwg, "x.dwg").expect("read dwg");
    let h = &hatches(&from_dwg)[0];
    assert!(h.associative, "still associative");
    assert_eq!(h.loops[0].sources, vec![Handle(0x40)], "read from the DWG");
}

/// The persistent reactors of an entity keep the order the file listed them in (AutoCAD notifies them in that order and keeps it).
#[test]
fn the_reactors_of_a_boundary_keep_the_order_of_the_file() {
    let mut d = Drawing::new_imperial();
    let outer = poly(&mut d, square(0.0, 20.0));
    let mut hatches = Vec::new();
    for _ in 0..2 {
        let loops = vec![HatchLoop { vertices: square(0.0, 20.0), outer: true, sources: vec![outer], ..Default::default() }];
        hatches.push(d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch(loops, true))).expect("add"));
    }
    let reactors_of = |text: &str| -> Vec<String> {
        let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
        let start = tags.iter().position(|t| t.code == 5 && t.str() == outer.hex()).unwrap();
        let open = start + tags[start..].iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
        tags[open + 1..].iter().take_while(|t| t.code == 330).map(|t| t.str()).collect()
    };
    let written = write_dxf(&d);
    let ours = reactors_of(&written);
    assert_eq!(ours.len(), 2);
    // The file lists them the other way round.
    let mut tags = cadcraft_dxf::parse(written.as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 5 && t.str() == outer.hex()).unwrap();
    let open = start + tags[start..].iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
    tags[open + 1..open + 3].reverse();
    let swapped = cadcraft_dxf::write_ascii(&tags);
    assert_eq!(reactors_of(&swapped), [ours[1].clone(), ours[0].clone()]);
    let back = read_dxf(swapped.as_bytes()).unwrap();
    assert_eq!(reactors_of(&write_dxf(&back)), [ours[1].clone(), ours[0].clone()], "the file's order, not ours");
}

/// AutoCAD drops the association of a hatch it reads before its boundary objects (39 hatches of Citycon): the records go in the order
/// of their handles, and the draw order that differs from it is the SORTENTSTABLE's.
#[test]
fn a_hatch_is_written_after_its_boundary_and_keeps_its_draw_order() {
    let mut d = Drawing::new_imperial();
    let (boundary, fill) = (Handle(0x100), Handle(0x500));
    let k = EntityKind::LwPolyline(LwPolyline {
        records: Default::default(),
        vertices: square(0.0, 10.0),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
        heavy: false,
        fit: None,
    });
    let loops = vec![HatchLoop { vertices: square(0.0, 10.0), outer: true, sources: vec![boundary], ..Default::default() }];
    let mut b = Block::new("Tag");
    // Drawn first: the hatch, then its boundary.
    b.entities.push(Entity::new(fill, EntityKind::Hatch(hatch(loops, true))));
    b.entities.push(Entity::new(boundary, k));
    d.blocks.insert("Tag".into(), std::sync::Arc::new(b));
    d.bump_handseed(Handle(0x500));
    let text = write_dxf(&d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let at = |h: Handle| tags.iter().position(|t| t.code == 5 && t.str() == h.hex()).unwrap();
    assert!(at(boundary) < at(fill), "the boundary is written before the hatch");
    let back = read_dxf(text.as_bytes()).unwrap();
    let order: Vec<Handle> = back.block("Tag").unwrap().entities.iter().map(|e| e.handle).collect();
    assert_eq!(order, [fill, boundary], "the hatch is still drawn first");
}

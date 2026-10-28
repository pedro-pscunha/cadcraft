use cadcraft_dxf::Tag;
use cadcraft_engine::Session;
use cadcraft_engine::doc::{AssocSnap, DimAssoc, EntityKind, Handle};
use serde_json::json;

fn without_private_links(tags: &mut Vec<Tag>) {
    let mut own = false;
    tags.retain(|t| {
        if t.code == 0 {
            own = false;
        }
        if t.code == 1001 {
            own = t.str() == "CADCRAFT";
        }
        !own
    });
}

fn native_source(circle: bool) -> (Session, Handle, Handle) {
    let mut s = Session::new();
    let target = if circle {
        s.execute("circle", &json!({"center":[0,0],"radius":10})).unwrap()["handle"].as_str().unwrap().to_string()
    } else {
        s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string()
    };
    let y = if circle { 10 } else { 0 };
    let other = s.execute("line", &json!({"points":[[75,y],[80,y]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"p1":if circle {[0,10]}else{[10,0]},"p2":[80,y],"at":[40,y+5]})).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let h = Handle::parse_hex(&dim).unwrap();
    let target = Handle::parse_hex(&target).unwrap();
    let other = Handle::parse_hex(&other).unwrap();
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.assoc = vec![
                    DimAssoc {
                        point: "p13".into(),
                        handle: target,
                        snap: if circle { AssocSnap::OnCircle { angle: std::f64::consts::FRAC_PI_2 } } else { AssocSnap::End },
                    },
                    DimAssoc { point: "p14".into(), handle: other, snap: AssocSnap::End },
                ];
            }
        })
        .unwrap();
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    without_private_links(&mut tags);
    let mut inside = false;
    let mut reference = 0;
    tags.retain(|t| {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
        }
        !(inside && t.code == 301)
    });
    for t in &mut tags {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
            reference = 0;
        } else if inside {
            if t.code == 72 {
                reference += 1;
                if reference == 1 {
                    *t = Tag::i(72, if circle { 5 } else { 1 });
                }
            }
            if reference == 1 {
                match t.code {
                    10 => *t = Tag::f(10, if circle { 0.0 } else { 10.0 }),
                    20 => *t = Tag::f(20, 0.0),
                    30 => *t = Tag::f(30, if circle { 2e50 } else { 0.0 }),
                    _ => {}
                }
            }
        }
    }
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(h));
    s.open_drawing(d, "native.dxf", None);
    (s, h, target)
}

fn reopen_raw(s: &mut Session, dim: Handle) {
    let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(s.doc().unwrap()).as_bytes()).unwrap();
    without_private_links(&mut tags);
    let mut inside = false;
    tags.retain(|t| {
        if t.code == 0 {
            inside = t.str() == "DIMASSOC";
        }
        !(inside && t.code == 301)
    });
    let d = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
    assert!(d.has_raw_dimassoc(dim));
    s.open_drawing(d, "raw.dxf", None);
}

#[test]
fn a_newly_unresolvable_intersection_freezes_every_point_and_warns() {
    let mut s = Session::new();
    let horizontal = s.execute("line", &json!({"points":[[0,0],[10,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let vertical = s.execute("line", &json!({"points":[[5,-5],[5,5]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    let dim = s.execute("dimlinear", &json!({"p1":[5,0],"p2":[5,5],"at":[10,2]})).unwrap()["handle"].as_str().unwrap().to_string();
    let h = Handle::parse_hex(&dim).unwrap();
    let a = Handle::parse_hex(&horizontal).unwrap();
    let b = Handle::parse_hex(&vertical).unwrap();
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.assoc = vec![
                    DimAssoc { point: "p13".into(), handle: a, snap: AssocSnap::Intersection { other: b } },
                    DimAssoc { point: "p14".into(), handle: b, snap: AssocSnap::End },
                ]
            }
        })
        .unwrap();
    reopen_raw(&mut s, h);
    let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
    let result = s.execute("rotate", &json!({"handles":[vertical],"base":[5,0],"angle":90})).unwrap();
    assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before, "a failed intersection must not partly update the endpoint");
    assert!(result["warnings"].to_string().contains(&dim), "unresolvable changed reference was silent: {result}");
    let restored = s.execute("rotate", &json!({"handles":[vertical],"base":[5,0],"angle":-90})).unwrap();
    assert!(!s.doc().unwrap().raw_dimassoc_unupdated(h), "a transient unsolvable intersection must recover when every pending reference resolves");
    assert!(restored.get("warnings").is_none());
    s.execute("move", &json!({"handles":[vertical],"delta":[2,0]})).unwrap();
    let EntityKind::Dimension(dm) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!("dimension") };
    assert!((dm.p13.x - 7.0).abs() < 1e-9 && (dm.p14.x - 7.0).abs() < 1e-9, "recovered raw links did not follow");
}

#[test]
fn deleted_polyline_vertices_are_not_repaired_by_ordinal_or_partial_evaluation() {
    for vertex in [1, 2] {
        let mut s = Session::new();
        let poly = s.execute("pline", &json!({"vertices":[[0,0],[5,0],[10,0]]})).unwrap()["handle"].as_str().unwrap().to_string();
        let other = s.execute("line", &json!({"points":[[75,0],[80,0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
        let dim = s.execute("dimlinear", &json!({"p1":[vertex*5,0],"p2":[80,0],"at":[40,5]})).unwrap()["handle"].as_str().unwrap().to_string();
        let h = Handle::parse_hex(&dim).unwrap();
        let p = Handle::parse_hex(&poly).unwrap();
        let o = Handle::parse_hex(&other).unwrap();
        s.doc_mut()
            .unwrap()
            .modify_entity(h, |e| {
                if let EntityKind::Dimension(dm) = &mut e.kind {
                    dm.assoc = vec![
                        DimAssoc { point: "p13".into(), handle: p, snap: AssocSnap::Vertex { index: vertex } },
                        DimAssoc { point: "p14".into(), handle: o, snap: AssocSnap::End },
                    ]
                }
            })
            .unwrap();
        reopen_raw(&mut s, h);
        let before = s.doc().unwrap().entity(h).unwrap().kind.clone();
        let result = s.execute("grip.option", &json!({"handle":poly,"index":vertex*2,"option":"removeVertex"})).unwrap();
        assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before, "deleted vertex {vertex} must leave the whole dimension frozen");
        assert!(result["warnings"].to_string().contains(&dim), "deleted vertex {vertex} was silently reused: {result}");
        let again = s.execute("move", &json!({"handles":[p.hex()],"delta":[2,0]})).unwrap();
        assert_eq!(s.doc().unwrap().entity(h).unwrap().kind, before, "an invalidated ordinal must not bind to a later vertex");
        assert!(again["warnings"].to_string().contains(&dim));
    }
}

#[test]
fn quadrant_identity_uses_the_dimension_point_when_the_cache_is_absent() {
    let (mut s, dim, target) = native_source(true);
    let result = s.execute("move", &json!({"handles":[target.hex()],"delta":[20,0]})).unwrap();
    let EntityKind::Dimension(dm) = &s.doc().unwrap().entity(dim).unwrap().kind else { panic!("dimension") };
    assert!((dm.p13.x - 20.0).abs() < 1e-9 && (dm.p13.y - 10.0).abs() < 1e-9, "north quadrant did not follow: {dm:?}");
    assert!(result.get("warnings").is_none());
}

#[test]
fn endpoint_identity_survives_native_save_reopen_and_another_edit() {
    for dwg in [false, true] {
        let (mut s, dim, target) = native_source(false);
        s.execute("move", &json!({"handles":[target.hex()],"delta":[20,0]})).unwrap();
        let saved = if dwg { cadcraft_io::write(s.doc().unwrap(), "x.dwg").unwrap() } else { cadcraft_io::write_dxf(s.doc().unwrap()).into_bytes() };
        let back = cadcraft_io::read(&saved, if dwg { "x.dwg" } else { "x.dxf" }).unwrap();
        // Native point semantics must survive without relying on CADCraft-only xdata.
        let mut tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(&back).as_bytes()).unwrap();
        without_private_links(&mut tags);
        let back = cadcraft_io::read_dxf(cadcraft_dxf::write_ascii(&tags).as_bytes()).unwrap();
        let EntityKind::Dimension(dm) = &back.entity(dim).unwrap().kind else { panic!("dimension") };
        assert_eq!(
            dm.assoc.iter().find(|a| a.point == "p13").unwrap().snap,
            AssocSnap::End,
            "DWG={dwg}: stale absolute cache changed endpoint identity"
        );
        s.open_drawing(back, "native-again.dxf", None);
        s.execute("move", &json!({"handles":[target.hex()],"delta":[20,0]})).unwrap();
        let EntityKind::Dimension(dm) = &s.doc().unwrap().entity(dim).unwrap().kind else { panic!("dimension") };
        assert!((dm.p13.x - 50.0).abs() < 1e-9, "DWG={dwg}: second edit lost the endpoint");
    }
}

//! A save with no geometry edit writes the geometry groups of the source record exactly: the
//! entity plane (210/220/230), elevation, Z values, original angles and scale signs that the
//! model flattens. A property edit keeps them, a geometry edit writes the current model.
//! User DXF and DWG, and a second save. Numbers are compared by their bits.

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_dxf::{Tag, records, sections};

/// The `code value` lines of some records as DXF text.
fn groups(records: &[&str]) -> String {
    let mut text = String::new();
    for e in records {
        for line in e.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (code, value) = line.split_once(' ').unwrap_or((line, ""));
            text.push_str(&format!("{code}\n{value}\n"));
        }
    }
    text
}

/// A DXF text from `code value` groups.
fn dxf(entities: &[&str]) -> String {
    format!("0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n", groups(entities))
}

/// A DXF text with the block `B` (an attribute definition) and the entities.
fn dxf_with_block(block: &[&str], entities: &[&str]) -> String {
    format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n5\n200\n8\n0\n2\nB\n70\n2\n10\n0\n20\n0\n30\n0\n3\nB\n1\n\n{}0\nENDBLK\n5\n201\n8\n0\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        groups(block),
        groups(entities)
    )
}

/// The groups of the first record of `kind` in the ENTITIES section of a DXF text.
fn record(text: &str, kind: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").expect("ENTITIES");
    records(&section.tags).into_iter().find(|(k, _)| k == kind).unwrap_or_else(|| panic!("no {kind} in {text}")).1
}

/// The bits of every number with group `code`, in order. An entity plane left out is +Z, which
/// is what a DWG writes for it.
fn bits(record: &[Tag], code: i32) -> Vec<u64> {
    let found: Vec<u64> = record.iter().filter(|t| t.code == code).map(|t| t.str().trim().parse::<f64>().unwrap().to_bits()).collect();
    match code {
        210 | 220 | 230 if found.is_empty() => vec![if code == 230 { 1.0f64 } else { 0.0 }.to_bits()],
        _ => found,
    }
}

/// The text of a drawing after a save to DWG and a reopen: what a DWG keeps.
fn through_dwg(d: &Drawing) -> String {
    let dwg = cadcraft_io::write(d, "x.dwg").unwrap();
    cadcraft_io::write_dxf(&cadcraft_io::read(&dwg, "x.dwg").unwrap())
}

/// Every text a drawing is checked in, with a second save of each: the user DXF, and the DXF of
/// the DWG save.
fn saves(d: &Drawing) -> Vec<(&'static str, String)> {
    let dxf = cadcraft_io::write_dxf(d);
    let dwg = through_dwg(d);
    let again = |text: &str| cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
    vec![("DXF", dxf.clone()), ("DXF, second save", again(&dxf)), ("DWG", dwg.clone()), ("DWG, second save", again(&dwg))]
}

/// Check that the geometry groups `codes` of the first `kind` record of the source are bit for bit
/// those of every save. A DWG stores angles in radians, so `angles` only agree to the last digits
/// there. The groups `dxf_only` are checked in the user DXF saves only (the DWG format or acadrust
/// does not keep them).
fn assert_kept(source: &str, kind: &str, codes: &[i32], angles: &[i32], dxf_only: &[i32]) {
    let want = record(source, kind);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for (what, text) in saves(&d) {
        let got = record(&text, kind);
        for code in codes {
            if what.starts_with("DWG") && dxf_only.contains(code) {
                continue;
            }
            if what.starts_with("DWG") && angles.contains(code) {
                let (a, b) = (bits(&got, *code), bits(&want, *code));
                assert_eq!(a.len(), b.len(), "{what}: {kind} group {code}");
                for (a, b) in a.into_iter().zip(b) {
                    let (a, b) = (f64::from_bits(a), f64::from_bits(b));
                    assert!((a - b).abs() <= 1e-12 * b.abs().max(1.0), "{what}: {kind} group {code}: {a} vs {b}");
                }
            } else {
                assert_eq!(bits(&got, *code), bits(&want, *code), "{what}: {kind} group {code}");
            }
        }
    }
}

const LINE: &str = "0 LINE\n5 100\n100 AcDbEntity\n8 0\n100 AcDbLine\n39 0.25\n10 1.5\n20 2.5\n30 -3.36738523e-06\n\
    11 4.0\n21 5.0\n31 -3.02954696e-06\n210 6.3141e-10\n220 2.5497228e-07\n230 0.99999999999997";
const CIRCLES: [&str; 4] = [
    // exact -Z, near -Z, tilted, near +Z with elevation
    "0 CIRCLE\n5 101\n100 AcDbEntity\n8 0\n100 AcDbCircle\n39 -0.5\n10 3.25\n20 4.5\n30 7.75\n40 2.125\n210 0\n220 0\n230 -1",
    "0 CIRCLE\n5 102\n100 AcDbEntity\n8 0\n100 AcDbCircle\n10 3.25\n20 4.5\n30 7.75\n40 2.125\n210 1e-8\n220 -2e-8\n230 -0.99999999999999978",
    "0 CIRCLE\n5 103\n100 AcDbEntity\n8 0\n100 AcDbCircle\n10 3.25\n20 4.5\n30 7.75\n40 2.125\n210 0.6\n220 0\n230 0.8",
    "0 CIRCLE\n5 104\n100 AcDbEntity\n8 0\n100 AcDbCircle\n10 3.25\n20 4.5\n30 12.5\n40 2.125\n210 0\n220 0\n230 1",
];
const ARCS: [&str; 2] = [
    "0 ARC\n5 110\n100 AcDbEntity\n8 0\n100 AcDbCircle\n10 3.25\n20 4.5\n30 7.75\n40 2.125\n210 0\n220 0\n230 -1\n\
     100 AcDbArc\n50 30.000000000000004\n51 120.5",
    "0 ARC\n5 111\n100 AcDbEntity\n8 0\n100 AcDbCircle\n10 3.25\n20 4.5\n30 7.75\n40 2.125\n210 0.6\n220 0\n230 0.8\n\
     100 AcDbArc\n50 10\n51 350",
];

#[test]
fn a_line_circle_and_arc_keep_their_plane_and_geometry_through_every_save() {
    assert_kept(&dxf(&[LINE]), "LINE", &[39, 10, 20, 30, 11, 21, 31, 210, 220, 230], &[], &[]);
    for c in CIRCLES {
        assert_kept(&dxf(&[c]), "CIRCLE", &[39, 10, 20, 30, 40, 210, 220, 230], &[], &[]);
    }
    for a in ARCS {
        assert_kept(&dxf(&[a]), "ARC", &[10, 20, 30, 40, 210, 220, 230, 50, 51], &[50, 51], &[]);
    }
}

fn first_handle(d: &Drawing) -> cadcraft_doc::Handle {
    d.model.iter().next().unwrap().handle
}

#[test]
fn a_property_edit_keeps_the_geometry_and_a_geometry_edit_drops_it() {
    let source = dxf(&[LINE]);
    let want = record(&source, "LINE");
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let h = first_handle(&d);
    d.modify_entity(h, |e| e.common.layer = "Other".into()).unwrap();
    let got = record(&cadcraft_io::write_dxf(&d), "LINE");
    assert_eq!(bits(&got, 210), bits(&want, 210), "a layer edit keeps the normal");
    assert_eq!(got.iter().find(|t| t.code == 8).map(Tag::str).as_deref(), Some("Other"));
    d.modify_entity(h, |e| {
        if let EntityKind::Line(l) = &mut e.kind {
            l.b.x += 1.0;
        }
    })
    .unwrap();
    let got = record(&cadcraft_io::write_dxf(&d), "LINE");
    assert!(!got.iter().any(|t| t.code == 210), "a geometry edit writes the model: {got:?}");
    assert_eq!(bits(&got, 11), vec![5.0f64.to_bits()]);
}

const PLANES: [&str; 4] = ["210 0\n220 0\n230 -1", "210 1e-8\n220 -2e-8\n230 -0.99999999999999978", "210 0.6\n220 0\n230 0.8", "210 0\n220 0\n230 1"];

#[test]
fn a_point_and_a_solid_keep_their_plane_and_geometry_through_every_save() {
    for plane in PLANES {
        let point = format!("0 POINT\n5 120\n100 AcDbEntity\n8 0\n100 AcDbPoint\n39 0.5\n10 1.25\n20 2.5\n30 3.75\n{plane}\n50 12.5");
        assert_kept(&dxf(&[&point]), "POINT", &[39, 10, 20, 30, 210, 220, 230, 50], &[50], &[]);
        let solid = format!(
            "0 SOLID\n5 121\n100 AcDbEntity\n8 0\n100 AcDbTrace\n10 0.5\n20 0.25\n30 2.5\n11 3.5\n21 0.25\n31 2.5\n12 0.5\n22 4.5\n32 2.5\n13 3.5\n23 4.5\n33 2.5\n39 -1.5\n{plane}"
        );
        assert_kept(&dxf(&[&solid]), "SOLID", &[39, 10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 210, 220, 230], &[], &[]);
    }
}

#[test]
fn text_and_multiline_text_keep_their_plane_and_geometry_through_every_save() {
    for plane in PLANES {
        let text = format!(
            "0 TEXT\n5 130\n100 AcDbEntity\n8 0\n100 AcDbText\n39 0.5\n10 1.25\n20 2.5\n30 3.75\n40 2.5\n1 Hello\n50 33.5\n41 0.8\n51 12\n7 Standard\n72 1\n11 4.5\n21 5.5\n31 3.75\n{plane}\n100 AcDbText\n73 2"
        );
        assert_kept(&dxf(&[&text]), "TEXT", &[39, 10, 20, 30, 40, 50, 41, 51, 72, 11, 21, 31, 210, 220, 230, 73], &[50, 51], &[]);
        let mtext = format!(
            "0 MTEXT\n5 131\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 1.25\n20 2.5\n30 3.75\n40 2.5\n41 30\n71 5\n72 1\n1 Two\\Plines\n7 Standard\n{plane}\n11 0.6\n21 0.8\n31 0\n73 1\n44 1.5"
        );
        assert_kept(&dxf(&[&mtext]), "MTEXT", &[10, 20, 30, 40, 41, 71, 72, 11, 21, 31, 210, 220, 230, 44], &[], &[]);
    }
}

#[test]
fn inserts_and_attributes_keep_their_plane_and_geometry_through_every_save() {
    let attdef = "0 ATTDEF\n5 202\n100 AcDbEntity\n8 0\n100 AcDbText\n10 0.25\n20 0.5\n30 0.75\n40 0.5\n1 default\n50 15\n72 0\n100 AcDbAttributeDefinition\n3 Prompt\n2 TAG\n70 0\n74 2";
    for plane in PLANES {
        let insert = format!(
            "0 INSERT\n5 140\n100 AcDbEntity\n8 0\n100 AcDbBlockReference\n66 1\n2 B\n10 7.5\n20 -8.5\n30 9.5\n41 -1\n42 2.5\n43 0.5\n50 33.25\n{plane}"
        );
        let attrib = format!(
            "0 ATTRIB\n5 141\n100 AcDbEntity\n8 0\n100 AcDbText\n10 7.75\n20 -8.25\n30 9.75\n40 0.5\n1 value\n50 48.25\n{plane}\n100 AcDbAttribute\n2 TAG\n70 0\n74 1"
        );
        let source = dxf_with_block(&[attdef], &[&insert, &attrib, "0 SEQEND\n5 142\n100 AcDbEntity\n8 0"]);
        assert_kept(&source, "INSERT", &[10, 20, 30, 41, 42, 43, 50, 210, 220, 230], &[50], &[]);
        assert_kept(&source, "ATTRIB", &[10, 20, 30, 40, 50, 210, 220, 230, 74], &[50], &[]);
    }
}

#[test]
fn a_text_edit_keeps_the_plane_and_writes_the_new_text() {
    let source = dxf(&[
        "0 TEXT\n5 130\n100 AcDbEntity\n8 0\n100 AcDbText\n10 1.25\n20 2.5\n30 3.75\n40 2.5\n1 Hello\n210 0\n220 0\n230 -1\n100 AcDbText",
        "0 MTEXT\n5 131\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 1.25\n20 2.5\n30 3.75\n40 2.5\n41 30\n71 5\n1 Old\n210 0\n220 0\n230 -1",
    ]);
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let handles: Vec<_> = d.model.iter().map(|e| e.handle).collect();
    for h in &handles {
        d.modify_entity(*h, |e| match &mut e.kind {
            EntityKind::Text(t) => t.value = "Changed".into(),
            EntityKind::MText(t) => t.contents = "Changed".into(),
            _ => {}
        })
        .unwrap();
    }
    let text = cadcraft_io::write_dxf(&d);
    for kind in ["TEXT", "MTEXT"] {
        let got = record(&text, kind);
        assert_eq!(bits(&got, 230), vec![(-1.0f64).to_bits()], "{kind} keeps its plane");
        assert!(got.iter().any(|t| t.code == 1 && t.str() == "Changed"), "{kind} has the new text: {got:?}");
    }
    // A height edit is a geometry edit: the MTEXT is written from the model.
    d.modify_entity(handles[1], |e| {
        if let EntityKind::MText(t) = &mut e.kind {
            t.height = 4.0;
        }
    })
    .unwrap();
    let got = record(&cadcraft_io::write_dxf(&d), "MTEXT");
    assert!(!got.iter().any(|t| t.code == 210), "{got:?}");
    assert_eq!(bits(&got, 40), vec![4.0f64.to_bits()]);
}

/// A hatch with an edge loop (four lines) and a polyline loop with a bulge, the pattern ANSI31 at 45
/// degrees, in the plane `plane` with an elevation.
fn hatch(plane: &str) -> String {
    format!(
        "0 HATCH\n5 150\n100 AcDbEntity\n8 0\n100 AcDbHatch\n10 0\n20 0\n30 2.5\n{plane}\n2 ANSI31\n70 0\n71 0\n91 2\n\
         92 1\n93 4\n72 1\n10 0\n20 0\n11 10\n21 0\n72 1\n10 10\n20 0\n11 10\n21 5\n72 1\n10 10\n20 5\n11 0\n21 5\n72 1\n10 0\n20 5\n11 0\n21 0\n97 0\n\
         92 3\n72 1\n73 1\n93 3\n10 2\n20 1\n42 0.5\n10 8\n20 1\n10 5\n20 4\n97 0\n\
         75 0\n76 1\n52 45\n41 2\n77 0\n78 1\n53 135.00000000000003\n43 0\n44 0\n45 0\n46 2.8284271247461903\n79 0\n47 0.2\n98 0"
    )
}

/// The numbers of a hatch's boundary and pattern groups, as (code, bits) in order.
fn hatch_numbers(record: &[Tag]) -> Vec<(i32, u64)> {
    const CODES: [i32; 27] = [10, 20, 30, 210, 220, 230, 11, 21, 42, 52, 41, 53, 43, 44, 45, 46, 47, 91, 92, 93, 72, 73, 78, 79, 98, 75, 76];
    record.iter().filter(|t| CODES.contains(&t.code)).map(|t| (t.code, t.str().trim().parse::<f64>().unwrap().to_bits())).collect()
}

#[test]
fn a_hatch_keeps_its_plane_boundary_and_pattern_through_every_save() {
    for plane in PLANES {
        let source = dxf(&[&hatch(plane)]);
        let want = record(&source, "HATCH");
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        for (what, text) in saves(&d) {
            let got = record(&text, "HATCH");
            if what.starts_with("DWG") {
                // acadrust stores the boundary as it likes; the plane and the elevation are kept.
                for code in [30, 210, 220, 230] {
                    assert_eq!(bits(&got, code), bits(&want, code), "{what}: HATCH group {code}");
                }
            } else {
                assert_eq!(hatch_numbers(&got), hatch_numbers(&want), "{what}: HATCH {plane}");
            }
        }
    }
}

#[test]
fn a_spline_keeps_its_z_values_and_plane_through_every_save() {
    for plane in PLANES {
        let spline = format!(
            "0 SPLINE\n5 160\n100 AcDbEntity\n8 0\n100 AcDbSpline\n{plane}\n70 8\n71 3\n72 8\n73 4\n74 0\n42 1e-7\n43 1e-7\n\
             40 0\n40 0\n40 0\n40 0\n40 1.5\n40 1.5\n40 1.5\n40 1.5\n10 0.5\n20 0.25\n30 1.125\n10 2.5\n20 3.25\n30 -2.5\n10 4.5\n20 -1.25\n30 0.75\n10 6.5\n20 2.25\n30 3.5"
        );
        // A DWG spline has no plane: its points are in world coordinates.
        assert_kept(&dxf(&[&spline]), "SPLINE", &[210, 220, 230, 71, 72, 73, 40, 10, 20, 30], &[], &[210, 220, 230]);
    }
}

#[test]
fn a_leader_keeps_its_z_values_plane_and_offsets_without_a_second_owner() {
    for plane in PLANES {
        let leader = format!(
            "0 LEADER\n5 170\n100 AcDbEntity\n8 0\n100 AcDbLeader\n3 Standard\n71 1\n72 0\n73 3\n74 1\n75 1\n40 2.5\n41 10\n76 3\n\
             10 0.5\n20 0.25\n30 1.125\n10 2.5\n20 3.25\n30 -2.5\n10 4.5\n20 -1.25\n30 0.75\n{plane}\n211 0.6\n221 0.8\n231 0\n212 0.1\n222 0.2\n232 0.3\n213 0.4\n223 0.5\n233 0.6"
        );
        let source = dxf(&[&leader]);
        // acadrust's DXF reader does not take the annotation's width (41) into a DWG.
        assert_kept(&source, "LEADER", &[76, 10, 20, 30, 210, 220, 230, 211, 221, 231, 212, 222, 232, 213, 223, 233, 40, 41], &[], &[41]);
        // Each group once: the snapshot owns the geometry, the kept subclass groups the rest.
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        let got = record(&cadcraft_io::write_dxf(&d), "LEADER");
        for code in [40, 41, 76, 210, 211, 212, 213] {
            assert_eq!(got.iter().filter(|t| t.code == code).count(), 1, "group {code} once: {got:?}");
        }
        assert_eq!(got.iter().filter(|t| t.code == 10).count(), 3);
        // A vertex edit writes the model: its vertices once and no plane from the source.
        let mut d = d;
        let h = first_handle(&d);
        d.modify_entity(h, |e| {
            if let EntityKind::Leader(l) = &mut e.kind {
                l.vertices[0].x += 1.0;
            }
        })
        .unwrap();
        let got = record(&cadcraft_io::write_dxf(&d), "LEADER");
        assert_eq!(got.iter().filter(|t| t.code == 10).count(), 3, "{got:?}");
        assert!(!got.iter().any(|t| (210..=213).contains(&t.code)), "{got:?}");
    }
}

/// The records of `kind` of the ENTITIES section of a DXF text.
fn all_records(text: &str, kind: &str) -> Vec<Vec<Tag>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").expect("ENTITIES");
    records(&section.tags).into_iter().filter(|(k, _)| k == kind).map(|(_, t)| t).collect()
}

/// Every number of the `codes` groups of all the records, as (code, bits) in order.
fn numbers(records: &[Vec<Tag>], codes: &[i32]) -> Vec<(i32, u64)> {
    records.iter().flatten().filter(|t| codes.contains(&t.code)).map(|t| (t.code, t.str().trim().parse::<f64>().unwrap().to_bits())).collect()
}

#[test]
fn a_lightweight_polyline_keeps_its_plane_elevation_and_vertices_through_every_save() {
    for plane in PLANES {
        let polyline = format!(
            "0 LWPOLYLINE\n5 180\n100 AcDbEntity\n8 0\n100 AcDbPolyline\n90 4\n70 1\n43 0.25\n38 2.5\n39 -0.5\n\
             10 1.5\n20 2.5\n40 0.1\n41 0.2\n42 0.5\n10 6.5\n20 2.5\n10 6.5\n20 7.5\n42 -0.25\n10 1.5\n20 7.5\n{plane}"
        );
        // A DWG gives every vertex a start width, an end width and a bulge, zeros included.
        assert_kept(&dxf(&[&polyline]), "LWPOLYLINE", &[90, 70, 43, 38, 39, 10, 20, 40, 41, 42, 210, 220, 230], &[], &[40, 41, 42]);
    }
}

#[test]
fn a_lightweight_polyline_keeps_the_defaults_and_vertex_ids_its_file_wrote() {
    // Explicit zero elevation and constant width, and the vertex identifiers (91) of AutoCAD 2010 and later.
    let polyline = "0 LWPOLYLINE\n5 181\n100 AcDbEntity\n8 0\n100 AcDbPolyline\n90 2\n70 0\n43 0\n38 0\n39 0\n10 1.5\n20 2.5\n91 11\n10 6.5\n20 2.5\n91 12\n210 0\n220 0\n230 -1";
    assert_kept(&dxf(&[polyline]), "LWPOLYLINE", &[90, 70, 43, 38, 39, 10, 20, 91, 210, 220, 230], &[], &[43, 38, 39, 91]);
}

/// A heavy 2D polyline (widths, a bulge), a 3D polyline and a polygon mesh in `plane`.
fn polylines(plane: &str) -> String {
    let v2 = |x: f64, y: f64, extra: &str| format!("0 VERTEX\n8 0\n100 AcDbVertex\n100 AcDb2dVertex\n10 {x}\n20 {y}\n30 0\n{extra}70 0");
    let v3 = |x: f64, y: f64, z: f64| format!("0 VERTEX\n8 0\n100 AcDbVertex\n100 AcDb3dPolylineVertex\n10 {x}\n20 {y}\n30 {z}\n70 32");
    let vm = |x: f64, y: f64, z: f64| format!("0 VERTEX\n8 0\n100 AcDbVertex\n100 AcDbPolygonMeshVertex\n10 {x}\n20 {y}\n30 {z}\n70 64");
    let seqend = "0 SEQEND\n8 0";
    [
        format!("0 POLYLINE\n5 190\n100 AcDbEntity\n8 0\n100 AcDb2dPolyline\n66 1\n10 0\n20 0\n30 2.5\n39 -0.5\n70 1\n40 0.25\n41 0.25\n{plane}"),
        v2(1.5, 2.5, "40 0.1\n41 0.2\n42 0.5\n"),
        v2(6.5, 2.5, ""),
        v2(6.5, 7.5, "42 -0.25\n"),
        seqend.to_string(),
        format!("0 POLYLINE\n5 191\n100 AcDbEntity\n8 0\n100 AcDb3dPolyline\n66 1\n10 0\n20 0\n30 0\n70 8\n{plane}"),
        v3(1.0, 2.0, 3.0),
        v3(4.0, 5.0, -6.0),
        seqend.to_string(),
        format!("0 POLYLINE\n5 192\n100 AcDbEntity\n8 0\n100 AcDbPolygonMesh\n66 1\n10 0\n20 0\n30 0\n70 16\n71 2\n72 2\n{plane}"),
        vm(0.0, 0.0, 1.0),
        vm(1.0, 0.0, 2.0),
        vm(0.0, 1.0, 3.0),
        vm(1.0, 1.0, 4.0),
        seqend.to_string(),
    ]
    .join("\n")
}

#[test]
fn polylines_and_meshes_keep_their_plane_and_vertices_through_every_save() {
    for plane in PLANES {
        let source = dxf(&[&polylines(plane)]);
        let want = all_records(&source, "POLYLINE");
        let want_vertices = all_records(&source, "VERTEX");
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        for (what, text) in saves(&d) {
            let (got, got_vertices) = (all_records(&text, "POLYLINE"), all_records(&text, "VERTEX"));
            assert_eq!(got.len(), 3, "{what}: {text}");
            if what.starts_with("DWG") {
                // A DWG keeps the plane of the heavy polyline; a 3D polyline and a mesh have none.
                for code in [210, 220, 230] {
                    assert_eq!(bits(&got[0], code), bits(&want[0], code), "{what}: POLYLINE group {code}");
                }
            } else {
                let codes = [10, 20, 30, 39, 70, 40, 41, 71, 72, 73, 74, 75, 210, 220, 230];
                assert_eq!(numbers(&got, &codes), numbers(&want, &codes), "{what}: POLYLINE parents {plane}");
                assert_eq!(
                    numbers(&got_vertices, &[10, 20, 30, 40, 41, 42, 70]),
                    numbers(&want_vertices, &[10, 20, 30, 40, 41, 42, 70]),
                    "{what}: VERTEX records {plane}"
                );
            }
        }
    }
}

#[test]
fn a_tolerance_keeps_its_position_direction_and_plane_through_every_save() {
    for plane in PLANES {
        let tolerance = format!(
            "0 TOLERANCE\n5 200\n100 AcDbEntity\n8 0\n100 AcDbFcf\n3 Standard\n10 1.5\n20 2.5\n30 3.5\n1 {{\\Fgdt;j}}%%v0.5\n{plane}\n11 0.6\n21 0.8\n31 0"
        );
        assert_kept(&dxf(&[&tolerance]), "TOLERANCE", &[10, 20, 30, 11, 21, 31, 210, 220, 230], &[], &[]);
    }
}

#[test]
fn a_helix_keeps_its_spline_and_helix_groups_through_every_save() {
    for plane in PLANES {
        let helix = format!(
            "0 HELIX\n5 201\n100 AcDbEntity\n8 0\n100 AcDbSpline\n{plane}\n70 8\n71 3\n72 8\n73 4\n74 0\n42 1e-7\n43 1e-7\n\
             40 0\n40 0\n40 0\n40 0\n40 1\n40 1\n40 1\n40 1\n10 1\n20 0\n30 0\n10 1.5\n20 2.5\n30 1\n10 -1.5\n20 2.5\n30 2\n10 -1\n20 0\n30 3\n\
             100 AcDbHelix\n90 29\n91 63\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0\n12 0\n22 0\n32 1\n40 1\n41 1\n42 3\n290 1\n280 2"
        );
        assert_kept(&dxf(&[&helix]), "HELIX", &[210, 220, 230, 71, 72, 73, 40, 10, 20, 30, 41, 42, 290, 280], &[], &[210, 220, 230]);
    }
}

#[test]
fn a_wipeout_keeps_its_frame_and_clipping_boundary_through_every_save() {
    let wipeout = "0 WIPEOUT\n5 202\n100 AcDbEntity\n8 0\n100 AcDbWipeout\n90 0\n10 1.5\n20 2.5\n30 3.5\n11 0.1\n21 0\n31 0.2\n12 0\n22 0.1\n32 0.3\n\
         13 1\n23 1\n70 7\n280 1\n281 50\n282 50\n283 0\n71 2\n91 4\n14 -0.5\n24 -0.5\n14 0.5\n24 -0.5\n14 0.5\n24 0.5\n14 -0.5\n24 0.5";
    // A DWG closes the boundary with one more vertex.
    assert_kept(&dxf(&[wipeout]), "WIPEOUT", &[10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 71, 91, 14, 24], &[], &[71, 91, 14, 24]);
}

/// A dimension of `kind`: its AcDbDimension groups (type flags `ty`), then its own subclasses.
fn dimension(ty: i32, plane: &str, own: &str) -> String {
    format!(
        "0 DIMENSION\n5 210\n100 AcDbEntity\n8 0\n100 AcDbDimension\n2 *D1\n10 5.5\n20 6.5\n30 1.25\n11 3.5\n21 4.5\n31 1.25\n70 {ty}\n53 12.5\n{plane}\n3 Standard\n{own}"
    )
}

#[test]
fn dimensions_keep_their_plane_and_definition_points_through_every_save() {
    let base = [10, 20, 30, 11, 21, 31, 53, 210, 220, 230];
    for plane in PLANES {
        let cases: [(&str, i32, &str, &[i32]); 6] = [
            (
                "linear",
                32,
                "100 AcDbAlignedDimension\n13 0.5\n23 1.5\n33 1.25\n14 8.5\n24 1.5\n34 1.25\n50 15.5\n100 AcDbRotatedDimension",
                &[13, 14, 50],
            ),
            ("aligned", 33, "100 AcDbAlignedDimension\n13 0.5\n23 1.5\n33 1.25\n14 8.5\n24 2.5\n34 1.25", &[13, 14]),
            ("radial", 36, "100 AcDbRadialDimension\n15 7.5\n25 8.5\n35 1.25\n40 0", &[15]),
            ("diameter", 35, "100 AcDbDiametricDimension\n15 7.5\n25 8.5\n35 1.25\n40 0", &[15]),
            (
                "angular",
                34,
                "100 AcDb2LineAngularDimension\n13 0.5\n23 1.5\n33 1.25\n14 8.5\n24 1.5\n34 1.25\n15 1.5\n25 1.5\n35 1.25\n16 4.5\n26 4.5\n36 1.25",
                &[13, 14, 15, 16],
            ),
            ("ordinate", 38, "100 AcDbOrdinateDimension\n13 0.5\n23 1.5\n33 1.25\n14 8.5\n24 1.5\n34 1.25", &[13, 14]),
        ];
        for (name, ty, own, own_codes) in cases {
            let source = dxf(&[&dimension(ty, plane, own)]);
            let codes: Vec<i32> =
                base.iter().copied().chain(own_codes.iter().flat_map(|c| [*c, c + 10, c + 20].into_iter().filter(|c| *c <= 50))).collect();
            // A DWG's two-line angular dimension has a 2D fourth point.
            assert_kept(&source, "DIMENSION", &codes, &[53, 50], &[36]);
            let _ = name;
        }
    }
}

#[test]
fn a_dimension_text_edit_keeps_the_plane_and_a_point_edit_drops_it() {
    let own = "100 AcDbAlignedDimension\n13 0.5\n23 1.5\n33 1.25\n14 8.5\n24 2.5\n34 1.25";
    let source = dxf(&[&dimension(33, PLANES[0], own)]);
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let h = first_handle(&d);
    d.modify_entity(h, |e| {
        if let EntityKind::Dimension(dm) = &mut e.kind {
            dm.text = "Over".into();
        }
    })
    .unwrap();
    let got = record(&cadcraft_io::write_dxf(&d), "DIMENSION");
    assert_eq!(bits(&got, 230), vec![(-1.0f64).to_bits()]);
    assert!(got.iter().any(|t| t.code == 1 && t.str() == "Over"), "{got:?}");
    d.modify_entity(h, |e| {
        if let EntityKind::Dimension(dm) = &mut e.kind {
            dm.p14.x += 1.0;
        }
    })
    .unwrap();
    let got = record(&cadcraft_io::write_dxf(&d), "DIMENSION");
    assert!(!got.iter().any(|t| t.code == 210), "{got:?}");
}

/// A drawing saved as 2018 DXF: the version with a native MULTILEADER.
fn save_2018(d: &Drawing) -> String {
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap()
}

/// The (depth, code, bits) of the geometry groups of a MULTILEADER record: the content context (depth 1),
/// a leader root (2) and a leader line (3).
fn mleader_numbers(record: &[Tag]) -> Vec<(usize, i32, u64)> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for t in record {
        let value = t.str();
        if matches!(t.code, 301 | 303 | 305) && value.trim() == "}" {
            depth = depth.saturating_sub(1);
        }
        let geometry = match depth {
            1 => matches!(t.code, 10..=16 | 20..=26 | 30..=36 | 42 | 43 | 46 | 47 | 110..=112 | 120..=122 | 130..=132),
            2 => matches!(t.code, 10 | 11 | 20 | 21 | 30 | 31 | 40),
            3 => matches!(t.code, 10 | 20 | 30),
            _ => false,
        };
        if geometry && let Ok(v) = value.trim().parse::<f64>() {
            out.push((depth, t.code, v.to_bits()));
        }
        if matches!(t.code, 300 | 302 | 304) && value.trim().ends_with('{') {
            depth += 1;
        }
    }
    out
}

/// A multileader the way CADCraft writes it, with the content plane and the landing's Z of the source changed.
fn mleader_source(plane: (f64, f64, f64)) -> String {
    use cadcraft_doc::{Common, MLeader, MText, Space};
    use cadcraft_geom::Vec3;
    let mut d = Drawing::new_imperial();
    let text = MText {
        insert: Vec3::new(8.0, 4.0, 0.0),
        height: 0.18,
        width: 3.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Note".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    };
    let m = MLeader {
        roots: vec![cadcraft_doc::MLeaderRoot::new(
            Vec3::new(5.0, 4.0, 0.5),
            0.36,
            false,
            vec![vec![Vec3::new(0.0, 0.0, 0.5), Vec3::new(2.0, 1.0, 0.5)]],
        )],
        text: Some(text),
        style: "Standard".into(),
        arrow_size: 0.18,
        spline: false,
        block: None,
        arrow_block: String::new(),
        context: cadcraft_doc::MLeaderContext::default(),
    };
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(m)).unwrap();
    let text = save_2018(&d);
    // Change the content's plane (11, 21, 31 under the CONTEXT_DATA) and its axes (110-112).
    let mut tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.str() == "MULTILEADER").unwrap();
    let context = tags.iter().skip(start).position(|t| t.code == 300).unwrap() + start;
    let normal = tags.iter().skip(context).position(|t| t.code == 11).unwrap() + context;
    for (k, v) in [plane.0, plane.1, plane.2].into_iter().enumerate() {
        tags[normal + k] = Tag::f(11 + 10 * k as i32, v);
    }
    let axes = tags.iter().skip(context).position(|t| t.code == 111).unwrap() + context;
    for (k, v) in [0.6, 0.8, 1e-9].into_iter().enumerate() {
        tags[axes + k] = Tag::f(111 + 10 * k as i32, v);
    }
    cadcraft_dxf::write_ascii(&tags)
}

#[test]
fn a_multileader_keeps_its_context_planes_and_leader_geometry_through_every_save() {
    for plane in [(0.0, 0.0, -1.0), (0.6, 0.0, 0.8), (1e-8, -2e-8, -0.9999999999999999)] {
        let source = mleader_source(plane);
        let want = mleader_numbers(&all_records(&source, "MULTILEADER")[0]);
        assert!(want.len() > 20, "{want:?}");
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        let first = save_2018(&d);
        let second = save_2018(&cadcraft_io::read_dxf(first.as_bytes()).unwrap());
        let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
        let through = cadcraft_io::read(&dwg, "x.dwg").unwrap();
        let (dwg_first, dwg_second) =
            (save_2018(&through), save_2018(&cadcraft_io::read(&cadcraft_io::write(&through, "x.dwg").unwrap(), "x.dwg").unwrap()));
        for (what, text) in [("DXF", first), ("DXF, second save", second), ("DWG", dwg_first), ("DWG, second save", dwg_second)] {
            assert_eq!(mleader_numbers(&all_records(&text, "MULTILEADER")[0]), want, "{what}: {plane:?}");
        }
    }
}

/// A table the way CADCraft writes it, with a plane, an insertion Z, a direction and odd sizes.
fn table_source(plane: (f64, f64, f64)) -> String {
    use cadcraft_doc::{Common, Space, Table, TableCell};
    use cadcraft_geom::Vec3;
    let mut d = Drawing::new_imperial();
    let cell = |t: &str| TableCell { text: t.into(), ..Default::default() };
    let t = Table {
        insert: Vec3::new(1.0, 10.0, 0.0),
        col_widths: vec![3.0, 2.0],
        row_heights: vec![0.6, 0.4],
        cells: vec![vec![cell("a"), cell("b")], vec![cell("c"), cell("d")]],
        ..Table::default()
    };
    d.add(&Space::Model, Common::default(), EntityKind::Table(t)).unwrap();
    let mut tags = cadcraft_dxf::parse(save_2018(&d).as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.str() == "ACAD_TABLE").unwrap();
    let ten = tags.iter().skip(start).position(|t| t.code == 10).unwrap() + start;
    tags[ten + 2] = Tag::f(30, 2.5);
    for (k, v) in [plane.0, plane.1, plane.2].into_iter().enumerate() {
        tags.insert(ten + 3 + k, Tag::f(210 + 10 * k as i32, v));
    }
    let table = tags.iter().skip(ten).position(|t| t.code == 11).unwrap() + ten;
    for (k, v) in [0.6, 0.8, 1e-9].into_iter().enumerate() {
        tags[table + k] = Tag::f(11 + 10 * k as i32, v);
    }
    let rows = tags.iter().skip(ten).position(|t| t.code == 141).unwrap() + ten;
    tags[rows] = Tag::f(141, 0.123456789012345);
    cadcraft_dxf::write_ascii(&tags)
}

#[test]
fn a_table_keeps_its_plane_direction_and_sizes_through_every_save() {
    for plane in [(0.0, 0.0, -1.0), (0.6, 0.0, 0.8)] {
        let source = table_source(plane);
        let want = all_records(&source, "ACAD_TABLE").remove(0);
        let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
        let first = save_2018(&d);
        let second = save_2018(&cadcraft_io::read_dxf(first.as_bytes()).unwrap());
        for (what, text) in [("DXF", first), ("DXF, second save", second)] {
            let got = all_records(&text, "ACAD_TABLE").remove(0);
            let codes = [10, 20, 30, 210, 220, 230, 11, 21, 31, 141, 142];
            assert_eq!(numbers(&[got], &codes), numbers(std::slice::from_ref(&want), &codes), "{what}: {plane:?}");
        }
    }
}

/// A column text as acadrust writes it for 2018: the main groups, then `101 Embedded Object` and a second set of the
/// same codes (the column rectangle). An unedited save keeps exactly one of each main group, and the embedded block.
#[test]
fn a_column_text_keeps_its_main_groups_and_its_embedded_object_apart() {
    let mtext = "0 MTEXT\n5 A1\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 13772.5\n20 -10439.9\n30 0\n40 0.15\n41 2.055\n71 1\n72 5\n1 PA_JARDL\n7 Standard\n\
        210 0\n220 0\n230 1\n11 1\n21 0\n31 0\n42 1.348\n43 0.164\n73 1\n44 1\n\
        101 Embedded Object\n70 1\n10 1\n20 0\n30 0\n11 13772.5\n21 -10439.9\n31 0\n40 2.055\n41 0\n42 0\n43 0\n71 2\n72 1\n44 2.055\n45 0.75\n73 0\n74 0\n46 0.164";
    let source = dxf(&[mtext]);
    let want = record(&source, "MTEXT");
    let main_of = |r: &[Tag]| r[..r.iter().position(|t| t.code == 101).unwrap_or(r.len())].to_vec();
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let opts = cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2018), binary: false };
    let user = String::from_utf8(cadcraft_io::write_with(&d, "x.dxf", &opts).unwrap()).unwrap();
    let again = String::from_utf8(cadcraft_io::write_with(&cadcraft_io::read_dxf(user.as_bytes()).unwrap(), "x.dxf", &opts).unwrap()).unwrap();
    for (what, text) in [("DXF", user), ("DXF, second save", again)] {
        let got = record(&text, "MTEXT");
        let main = main_of(&got);
        for code in [10, 11, 40, 41, 71, 72, 44, 73] {
            assert_eq!(main.iter().filter(|t| t.code == code).count(), 1, "{what}: group {code} once in the main subclass: {got:?}");
        }
        for code in [10, 20, 30, 40, 41, 11, 21, 31] {
            assert_eq!(bits(&main, code), bits(&main_of(&want), code), "{what}: main group {code}");
        }
    }
    // Through a DWG the insertion point, the direction, the height and the attachment survive.
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    let (EntityKind::MText(a), EntityKind::MText(b)) = (&d.model.iter().next().unwrap().kind, &back.model.iter().next().unwrap().kind) else {
        panic!("MTEXT")
    };
    assert_eq!((a.insert, a.height, a.attach), (b.insert, b.height, b.attach));
    assert!((a.rotation - b.rotation).abs() < 1e-9, "{} vs {}", a.rotation, b.rotation);
}

/// Group 48 of the AcDbMText subclass is the width of a column, not the linetype scale of the entity.
#[test]
fn the_column_width_of_a_text_is_not_its_linetype_scale() {
    let mtext = "0 MTEXT\n5 221\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 1\n20 2\n30 0\n40 2.5\n41 30\n71 1\n72 1\n1 Two\\Pcolumns\n7 Standard\n73 1\n44 1\n75 2\n76 2\n78 0\n79 0\n48 14.5\n49 1.0\n50 12.5\n50 12.5";
    let d = cadcraft_io::read_dxf(dxf(&[mtext]).as_bytes()).unwrap();
    assert_eq!(d.model.iter().next().unwrap().common.ltscale, 1.0);
    assert!(!record(&cadcraft_io::write_dxf(&d), "MTEXT").iter().take_while(|t| t.code != 100 || t.str() == "AcDbEntity").any(|t| t.code == 48));
    let line = "0 LINE\n5 222\n100 AcDbEntity\n8 0\n48 0.5\n100 AcDbLine\n10 0\n20 0\n30 0\n11 1\n21 0\n31 0";
    let d = cadcraft_io::read_dxf(dxf(&[line]).as_bytes()).unwrap();
    assert_eq!(d.model.iter().next().unwrap().common.ltscale, 0.5, "the linetype scale of the entity subclass still reads");
}

/// The groups of the leader roots of a MULTILEADER record (depth 2 and 3), as (code, value) text.
fn root_groups(record: &[Tag]) -> Vec<(i32, String)> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for t in record {
        let value = t.str();
        if matches!(t.code, 301 | 303 | 305) && value.trim() == "}" {
            depth = depth.saturating_sub(1);
        }
        if depth >= 1 && (t.code == 302 || depth >= 2) {
            // Numbers by their bits, so that "1" and "1.0" are one value.
            let v = value.trim().parse::<f64>().map_or_else(|_| value.trim().to_string(), |v| format!("{:x}", v.to_bits()));
            out.push((t.code, v));
        }
        if matches!(t.code, 300 | 302 | 304) && value.trim().ends_with('{') {
            depth += 1;
        }
    }
    out
}

/// A multileader of the shape CADCraft writes, with a second leader root after the first: its own landing,
/// its direction, dogleg and one leader line.
fn two_root_source() -> String {
    let mut tags = cadcraft_dxf::parse(mleader_source((0.0, 0.0, 1.0)).as_bytes()).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.str() == "MULTILEADER").unwrap();
    let close = tags.iter().skip(start).position(|t| t.code == 303).unwrap() + start;
    let second = [
        Tag::s(302, "LEADER{"),
        Tag::i(290, 1),
        Tag::i(291, 1),
        Tag::f(10, 9.5),
        Tag::f(20, 9.25),
        Tag::f(30, 0.5),
        Tag::f(11, -1.0),
        Tag::f(21, 0.0),
        Tag::f(31, 0.0),
        Tag::i(90, 1),
        Tag::f(40, 0.4375),
        Tag::s(304, "LEADER_LINE{"),
        Tag::f(10, 7.0),
        Tag::f(20, 8.0),
        Tag::f(30, 0.5),
        Tag::f(10, 6.5),
        Tag::f(20, 6.0),
        Tag::f(30, 0.5),
        Tag::i(91, 1),
        Tag::s(305, "}"),
        Tag::i(271, 0),
        Tag::s(303, "}"),
    ];
    for (i, t) in second.into_iter().enumerate() {
        tags.insert(close + 1 + i, t);
    }
    cadcraft_dxf::write_ascii(&tags)
}

#[test]
fn a_multileader_with_two_leader_roots_keeps_them_apart() {
    let source = two_root_source();
    let want = root_groups(&all_records(&source, "MULTILEADER")[0]);
    assert_eq!(want.iter().filter(|(c, _)| *c == 302).count(), 2, "{want:?}");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let first = save_2018(&d);
    let second = save_2018(&cadcraft_io::read_dxf(first.as_bytes()).unwrap());
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let through = cadcraft_io::read(&dwg, "x.dwg").unwrap();
    let dwg_first = save_2018(&through);
    for (what, text) in [("DXF", first), ("DXF, second save", second)] {
        assert_eq!(root_groups(&all_records(&text, "MULTILEADER")[0]), want, "{what}");
    }
    // A DWG gives every leader line properties of its own: its roots, landings and vertices are compared.
    let places = |g: Vec<(i32, String)>| g.into_iter().filter(|(c, _)| matches!(c, 302 | 10 | 20 | 30 | 11 | 21 | 31)).collect::<Vec<_>>();
    assert_eq!(places(root_groups(&all_records(&dwg_first, "MULTILEADER")[0])), places(want), "DWG");
    // A vertex edit of one root keeps both roots: the edited line is the model's, the sibling root keeps its
    // landing, direction, dogleg and line, in a DXF and in a DWG.
    let mut edited = d.clone();
    let h = first_handle(&edited);
    edited
        .modify_entity(h, |e| {
            if let EntityKind::MLeader(m) = &mut e.kind {
                m.roots[0].branches[0].vertices[0].x += 1.0;
            }
        })
        .unwrap();
    let sibling = |text: &str| {
        let g = root_groups(&all_records(text, "MULTILEADER")[0]);
        let at = g.iter().rposition(|(c, v)| *c == 302 && v.starts_with("LEADER")).expect("a second root");
        g[at..].to_vec()
    };
    let got = root_groups(&all_records(&save_2018(&edited), "MULTILEADER")[0]);
    assert_eq!(got.iter().filter(|(c, _)| *c == 302).count(), 2, "{got:?}");
    assert_eq!(sibling(&save_2018(&edited)), sibling(&source), "the sibling root of the DXF");
    let through = cadcraft_io::read(&cadcraft_io::write(&edited, "x.dwg").unwrap(), "x.dwg").unwrap();
    let dwg_roots = root_groups(&all_records(&save_2018(&through), "MULTILEADER")[0]);
    assert_eq!(dwg_roots.iter().filter(|(c, _)| *c == 302).count(), 2, "{dwg_roots:?}");
}

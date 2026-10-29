//! A save with no geometry edit writes the geometry groups of the source record exactly: the
//! entity plane (210/220/230), elevation, Z values, original angles and scale signs that the
//! model flattens. A property edit keeps them, a geometry edit writes the current model.
//! User DXF and DWG, and a second save. Numbers are compared by their bits.

use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_dxf::{Tag, records, sections};

/// A DXF text from `code value` groups.
fn dxf(entities: &[&str]) -> String {
    let mut text = String::from("0\nSECTION\n2\nENTITIES\n");
    for e in entities {
        for line in e.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (code, value) = line.split_once(' ').unwrap_or((line, ""));
            text.push_str(&format!("{code}\n{value}\n"));
        }
    }
    text + "0\nENDSEC\n0\nEOF\n"
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
/// there.
fn assert_kept(source: &str, kind: &str, codes: &[i32], angles: &[i32]) {
    let want = record(source, kind);
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for (what, text) in saves(&d) {
        let got = record(&text, kind);
        for code in codes {
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
    assert_kept(&dxf(&[LINE]), "LINE", &[39, 10, 20, 30, 11, 21, 31, 210, 220, 230], &[]);
    for c in CIRCLES {
        assert_kept(&dxf(&[c]), "CIRCLE", &[39, 10, 20, 30, 40, 210, 220, 230], &[]);
    }
    for a in ARCS {
        assert_kept(&dxf(&[a]), "ARC", &[10, 20, 30, 40, 210, 220, 230, 50, 51], &[50, 51]);
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

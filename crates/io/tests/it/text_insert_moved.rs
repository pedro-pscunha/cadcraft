//! The baseline start (group 10) of a justified text that was moved or rotated keeps its place relative to the alignment
//! point (group 11) as the file had it, whatever font measured it. A text with another value takes CADCraft's own
//! measure.

use cadcraft_doc::{Drawing, EntityKind, Handle, Text};
use cadcraft_geom::Vec3;

/// A centred TEXT whose baseline start lies 3 left of its alignment point: wider than CADCraft's font puts it for "HI".
const SRC: &str = "0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbText\n10\n7\n20\n0\n30\n0\n40\n2\n1\nHI\n72\n1\n11\n10\n21\n0\n31\n0\n100\nAcDbText\n0\nENDSEC\n0\nEOF\n";

fn drawing() -> (Drawing, Handle) {
    let d = cadcraft_io::read_dxf(SRC.as_bytes()).unwrap();
    let h = d.model.iter().next().map(|e| e.handle).unwrap();
    (d, h)
}

fn edit(d: &mut Drawing, h: Handle, f: impl FnOnce(&mut Text)) {
    d.model
        .modify(h, |e| {
            if let EntityKind::Text(t) = &mut e.kind {
                f(t);
            }
        })
        .unwrap();
}

fn insert_after(d: &Drawing, name: &str) -> Vec3 {
    let back = cadcraft_io::read(&cadcraft_io::write(d, name).unwrap(), name).unwrap();
    back.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.insert) } else { None }).expect("a text")
}

fn near(a: Vec3, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 1e-9 && (a.y - y).abs() < 1e-9
}

#[test]
fn a_moved_justified_text_keeps_the_offset_the_file_gave_it() {
    let (mut d, h) = drawing();
    edit(&mut d, h, |t| {
        t.insert = t.insert + Vec3::new(5.0, 3.0, 0.0);
        t.align_pt = t.align_pt.map(|a| a + Vec3::new(5.0, 3.0, 0.0));
    });
    for name in ["x.dxf", "x.dwg"] {
        let q = insert_after(&d, name);
        assert!(near(q, 12.0, 3.0), "{name}: {q:?}");
    }
}

#[test]
fn a_rotated_justified_text_turns_the_offset_with_it() {
    let (mut d, h) = drawing();
    edit(&mut d, h, |t| t.rotation = std::f64::consts::FRAC_PI_2);
    for name in ["x.dxf", "x.dwg"] {
        let q = insert_after(&d, name);
        assert!(near(q, 10.0, -3.0), "{name}: {q:?}");
    }
}

#[test]
fn a_moved_text_with_another_value_takes_the_computed_baseline_start() {
    let (mut d, h) = drawing();
    edit(&mut d, h, |t| {
        t.value = "HELLO WORLD".into();
        t.insert = t.insert + Vec3::new(5.0, 3.0, 0.0);
        t.align_pt = t.align_pt.map(|a| a + Vec3::new(5.0, 3.0, 0.0));
    });
    let q = insert_after(&d, "x.dxf");
    assert!(!near(q, 12.0, 3.0), "the file's offset belongs to the old value: {q:?}");
    assert!(q.x < 11.0 && (q.y - 3.0).abs() < 1e-9, "left of the alignment point, on the baseline: {q:?}");
}

#[test]
fn a_moved_attribute_keeps_the_offset_the_file_gave_it() {
    let mut src = String::new();
    for line in super::multiline_attributes::FIXTURE.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        src.push_str(&format!("{code}\n{value}\n"));
    }
    let mut d = cadcraft_io::read_dxf(src.as_bytes()).unwrap();
    let h = d.model.iter().next().map(|e| e.handle).unwrap();
    d.model
        .modify(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                for a in &mut i.attribs {
                    a.text.insert = a.text.insert + Vec3::new(5.0, 3.0, 0.0);
                    a.text.align_pt = a.text.align_pt.map(|p| p + Vec3::new(5.0, 3.0, 0.0));
                    if let Some(m) = &mut a.multiline {
                        m.mtext.insert = m.mtext.insert + Vec3::new(5.0, 3.0, 0.0);
                    }
                }
            }
        })
        .unwrap();
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    let q =
        back.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { i.attribs.first().map(|a| a.text.insert) } else { None }).unwrap();
    assert!(near(q, 107.0, 55.0), "{q:?}");
}

//! Groups of a DIMENSION that its file gave and a CADCraft move does not change: the text position (11) of a dimension whose
//! text was not placed by hand, and the leader length (40) of a radius or diameter dimension. A dimension whose lines changed
//! lays its text out again.

use cadcraft_doc::{Common, DimKind, Dimension, Drawing, EntityKind, Handle, Space};
use cadcraft_geom::Vec3;

fn linear() -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(5.0, 3.0, 0.0),
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
        leader_length: 0.0,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    }
}

fn radius() -> Dimension {
    Dimension { kind: DimKind::Radius, defpt: Vec3::new(0.0, 0.0, 0.0), p15: Vec3::new(4.0, 0.0, 0.0), p13: Vec3::ZERO, p14: Vec3::ZERO, ..linear() }
}

/// `text` (an ASCII DXF) with the first group `code` of the first record of kind `kind` set to `value`.
fn set_group(text: &str, kind: &str, code: i32, value: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let (mut in_kind, mut done) = (false, false);
    for pair in lines.chunks(2) {
        let (c, v) = (pair[0].trim(), pair.get(1).copied().unwrap_or(""));
        if c == "0" {
            in_kind = v.trim() == kind;
        }
        out.push(pair[0].to_string());
        if in_kind && !done && c == code.to_string() {
            out.push(value.to_string());
            done = true;
        } else if let Some(v) = pair.get(1) {
            out.push((*v).to_string());
        }
    }
    assert!(done, "no group {code} in {kind}");
    out.join("\n") + "\n"
}

/// The drawing, as a file whose dimension has the text position (`text_at`) CADCraft's layout does not give.
fn source(dm: Dimension, edits: &[(i32, &str)]) -> (Drawing, Handle) {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Dimension(dm)).unwrap();
    let mut text = String::from_utf8(cadcraft_io::write(&d, "x.dxf").unwrap()).unwrap();
    for (code, value) in edits {
        text = set_group(&text, "DIMENSION", *code, value);
    }
    let d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let h = d.model.iter().next().map(|e| e.handle).unwrap();
    (d, h)
}

fn edit(d: &mut Drawing, h: Handle, f: impl FnOnce(&mut Dimension)) {
    d.model
        .modify(h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                f(dm);
            }
        })
        .unwrap();
}

fn shift(dm: &mut Dimension, by: Vec3) {
    for p in [&mut dm.defpt, &mut dm.text_mid, &mut dm.p13, &mut dm.p14, &mut dm.p15, &mut dm.p16] {
        *p = *p + by;
    }
    dm.block = None;
}

fn dimension(d: &Drawing) -> Dimension {
    d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm.clone()) } else { None }).expect("a dimension")
}

fn reread(d: &Drawing, name: &str) -> Dimension {
    dimension(&cadcraft_io::read(&cadcraft_io::write(d, name).unwrap(), name).unwrap())
}

fn near(a: Vec3, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 1e-9 && (a.y - y).abs() < 1e-9
}

#[test]
fn a_moved_dimension_keeps_the_text_position_its_file_gave_it() {
    let (mut d, h) = source(linear(), &[(11, "6.5"), (21, "4.2")]);
    assert!(near(dimension(&d).text_mid, 6.5, 4.2));
    edit(&mut d, h, |dm| shift(dm, Vec3::new(10.0, 5.0, 0.0)));
    for name in ["x.dxf", "x.dwg"] {
        let dm = reread(&d, name);
        assert!(near(dm.text_mid, 16.5, 9.2), "{name}: {:?}", dm.text_mid);
    }
}

#[test]
fn a_dimension_whose_lines_changed_lays_its_text_out_again() {
    let (mut d, h) = source(linear(), &[(11, "6.5"), (21, "4.2")]);
    edit(&mut d, h, |dm| {
        shift(dm, Vec3::new(10.0, 5.0, 0.0));
        dm.defpt.y += 1.0;
    });
    let dm = reread(&d, "x.dxf");
    assert!(!near(dm.text_mid, 16.5, 9.2), "the file's position belongs to the old lines: {:?}", dm.text_mid);
}

#[test]
fn a_moved_radius_dimension_keeps_its_leader_length() {
    let (mut d, h) = source(radius(), &[(40, "2.5")]);
    assert_eq!(dimension(&d).leader_length, 2.5);
    edit(&mut d, h, |dm| shift(dm, Vec3::new(10.0, 5.0, 0.0)));
    for name in ["x.dxf", "x.dwg"] {
        assert_eq!(reread(&d, name).leader_length, 2.5, "{name}");
    }
}

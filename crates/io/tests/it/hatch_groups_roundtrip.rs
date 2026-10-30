//! HATCH groups that a CADCraft edit does not change survive a save after the edit: the pattern type (76), the double flag
//! (77), the seed points (98), and the gradient's shift (461), colour mode (452), tint (462) and indexed colours (63).
//! Seed points follow the hatch when it moves and are dropped when a reshaped boundary leaves them outside.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::*;
use cadcraft_geom::{PolyVertex, Vec2};

fn square(side: f64) -> HatchLoop {
    let v = [(0.0, 0.0), (side, 0.0), (side, side), (0.0, side)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect();
    HatchLoop { vertices: v, outer: true, sources: Vec::new(), ..Default::default() }
}

fn pattern_hatch() -> Hatch {
    Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![square(10.0)],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
        pattern_type: 2,
        pattern_double: true,
        seed_points: vec![Vec2::new(2.0, 3.0), Vec2::new(8.0, 8.0)],
    }
}

fn gradient_hatch() -> Hatch {
    let source = GradientSource { shift: Some(0.5), color_mode: 1, tint: 0.25, aci: [Some((30, Rgb(10, 200, 30))), None] };
    let gradient =
        Gradient { name: "SPHERICAL".into(), color1: Color::True(Rgb(10, 200, 30)), color2: Color::Index(7), angle: 0.5, centered: false, source };
    Hatch {
        pattern: "SOLID".into(),
        solid: true,
        gradient: Some(gradient),
        pattern_type: 1,
        pattern_double: false,
        seed_points: Vec::new(),
        ..pattern_hatch()
    }
}

fn drawing(h: Hatch) -> (Drawing, Handle) {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(h)).unwrap();
    let handle = d.model.iter().next().map(|e| e.handle).unwrap();
    (d, handle)
}

fn hatch(d: &Drawing) -> Hatch {
    d.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).expect("a hatch")
}

fn reread(d: &Drawing, name: &str) -> Hatch {
    hatch(&cadcraft_io::read(&cadcraft_io::write(d, name).unwrap(), name).unwrap())
}

fn edit(d: &mut Drawing, h: Handle, f: impl FnOnce(&mut Hatch)) {
    d.model
        .modify(h, |e| {
            if let EntityKind::Hatch(x) = &mut e.kind {
                f(x);
            }
        })
        .unwrap();
}

fn move_by(h: &mut Hatch, by: Vec2) {
    h.loops.iter_mut().flat_map(|l| l.vertices.iter_mut()).for_each(|v| v.p += by);
    h.seed_points.iter_mut().for_each(|p| *p += by);
}

#[test]
fn the_pattern_groups_and_seed_points_of_a_moved_hatch_survive() {
    let (mut d, h) = drawing(pattern_hatch());
    edit(&mut d, h, |x| move_by(x, Vec2::new(5.0, 7.0)));
    for name in ["x.dxf", "x.dwg"] {
        let back = reread(&d, name);
        assert_eq!((back.pattern_type, back.pattern_double), (2, true), "{name}");
        assert_eq!(back.seed_points, vec![Vec2::new(7.0, 10.0), Vec2::new(13.0, 15.0)], "{name}");
    }
}

#[test]
fn a_seed_point_a_reshaped_boundary_leaves_outside_is_dropped() {
    let (mut d, h) = drawing(pattern_hatch());
    // The boundary shrinks to 5 by 5: (2, 3) stays inside, (8, 8) is outside.
    edit(&mut d, h, |x| x.loops = vec![square(5.0)]);
    let back = reread(&d, "x.dxf");
    assert_eq!(back.seed_points, vec![Vec2::new(2.0, 3.0)]);
}

#[test]
fn a_hatch_made_here_is_predefined_and_single_with_no_seed_points() {
    let (d, _) = drawing(Hatch { pattern_type: 1, pattern_double: false, seed_points: Vec::new(), ..pattern_hatch() });
    let back = reread(&d, "x.dxf");
    assert_eq!((back.pattern_type, back.pattern_double, back.seed_points.len()), (1, false, 0));
}

#[test]
fn the_gradient_groups_survive_a_save() {
    let (d, _) = drawing(gradient_hatch());
    let want = hatch(&d).gradient.unwrap();
    let back = reread(&d, "x.dxf").gradient.unwrap();
    assert_eq!(back, want, "DXF");
    // A DWG keeps the shift, the colour mode and the tint.
    let dwg = reread(&d, "x.dwg").gradient.unwrap();
    assert_eq!((dwg.centered, dwg.name.as_str()), (false, "SPHERICAL"));
    assert_eq!((dwg.source.shift, dwg.source.color_mode, dwg.source.tint), (want.source.shift, want.source.color_mode, want.source.tint), "DWG");
    // The DWG reader gives the indexed colour of a true colour again from its RGB value, not the file's (acadrust).
    assert_eq!(dwg.source.aci[0].map(|(_, rgb)| rgb), want.source.aci[0].map(|(_, rgb)| rgb), "DWG");
}

#[test]
fn a_changed_true_colour_takes_the_nearest_indexed_colour() {
    let (mut d, h) = drawing(gradient_hatch());
    edit(&mut d, h, |x| {
        if let Some(g) = &mut x.gradient {
            g.color1 = Color::True(Rgb(250, 0, 0));
        }
    });
    let back = reread(&d, "x.dxf").gradient.unwrap();
    assert_eq!(back.source.aci[0], Some((cadcraft_color::nearest_aci(Rgb(250, 0, 0)) as i16, Rgb(250, 0, 0))));
}

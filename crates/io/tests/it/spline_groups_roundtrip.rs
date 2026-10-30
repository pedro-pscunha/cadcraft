//! SPLINE groups that a CADCraft edit does not change survive a save after the edit: the knot and control point tolerances
//! (42, 43) and the linear flag (bit 16 of group 70, kept while the spline is degree 1).

use cadcraft_doc::{Common, Drawing, EntityKind, Handle, Space};
use cadcraft_geom::{Spline, SplineSource, Vec2};

fn linear_spline() -> Spline {
    let control = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0)];
    let source = SplineSource { knot_tolerance: 1e-7, control_tolerance: 2e-7, linear: true };
    Spline { source: Some(Box::new(source)), ..Spline::from_control(control, 1) }
}

fn cubic() -> Spline {
    Spline::from_control(vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 3.0), Vec2::new(4.0, 0.0), Vec2::new(6.0, 3.0)], 3)
}

fn drawing(sp: Spline) -> (Drawing, Handle) {
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Spline(sp)).unwrap();
    let h = d.model.iter().next().map(|e| e.handle).unwrap();
    (d, h)
}

fn edit(d: &mut Drawing, h: Handle, f: impl FnOnce(&mut Spline)) {
    d.model
        .modify(h, |e| {
            if let EntityKind::Spline(s) = &mut e.kind {
                f(s);
            }
        })
        .unwrap();
}

fn reread(d: &Drawing, name: &str) -> Spline {
    let back = cadcraft_io::read(&cadcraft_io::write(d, name).unwrap(), name).unwrap();
    back.model.iter().find_map(|e| if let EntityKind::Spline(s) = &e.kind { Some(s.clone()) } else { None }).expect("a spline")
}

#[test]
fn a_moved_spline_keeps_its_tolerances_and_its_linear_flag() {
    let (mut d, h) = drawing(linear_spline());
    edit(&mut d, h, |s| s.control.iter_mut().for_each(|c| *c += Vec2::new(5.0, 5.0)));
    assert_eq!(reread(&d, "x.dxf").source(), linear_spline().source());
    // The DWG keeps the tolerances. It gives the linear flag back false: acadrust does not carry it through a DWG.
    let dwg = reread(&d, "x.dwg").source();
    assert_eq!((dwg.knot_tolerance, dwg.control_tolerance), (1e-7, 2e-7));
}

#[test]
fn a_spline_that_is_no_longer_degree_1_is_no_longer_linear() {
    let (mut d, h) = drawing(linear_spline());
    edit(&mut d, h, |s| *s = Spline { source: s.source.clone(), ..cubic() });
    let back = reread(&d, "x.dxf").source();
    assert_eq!((back.knot_tolerance, back.linear), (1e-7, false));
}

#[test]
fn a_spline_made_here_writes_1e_10_and_no_linear_flag() {
    let (d, _) = drawing(cubic());
    assert_eq!(reread(&d, "x.dxf").source, None);
    assert_eq!(SplineSource::default().knot_tolerance, 1e-10);
}

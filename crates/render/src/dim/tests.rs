use super::*;
use cadcraft_geom::Vec3;

fn lin(p1: (f64, f64), p2: (f64, f64), on: (f64, f64), rot: f64) -> Dimension {
    Dimension {
        kind: DimKind::Linear { rotation: rot },
        defpt: Vec3::new(on.0, on.1, 0.0),
        text_mid: Vec3::ZERO,
        p13: Vec3::new(p1.0, p1.1, 0.0),
        p14: Vec3::new(p2.0, p2.1, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn st_with(f: impl Fn(&mut DimStyle)) -> DimStyle {
    let mut s = DimStyle::default();
    f(&mut s);
    s
}

fn bbox(pts: &[Vec<Vec2>]) -> cadcraft_geom::Bounds2 {
    cadcraft_geom::Bounds2::from_points(pts.iter().flatten().copied())
}

#[test]
fn two_line_angular_measurements_follow_the_quadrant_with_the_arc_point() {
    for (arc_degrees, expected_degrees) in [(22.5_f64, 45.0_f64), (112.5, 135.0), (202.5, 45.0), (292.5, 135.0)] {
        let mut d = lin((0.0, 0.0), (10.0, 0.0), (10.0, 10.0), 0.0);
        d.kind = DimKind::Angular;
        d.p15 = Vec3::ZERO;
        let angle = arc_degrees.to_radians();
        d.p16 = Vec3::new(5.0 * angle.cos(), 5.0 * angle.sin(), 0.0);
        let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
        assert!((g.measurement - expected_degrees.to_radians()).abs() < 1e-12, "arc={arc_degrees}: measured {}", g.measurement.to_degrees());
    }
}

#[test]
fn two_line_angular_measurement_uses_a_nonzero_ray_when_an_endpoint_is_the_vertex() {
    let mut d = lin((0.0, 0.0), (-6.0, 6.0), (0.0, 10.0), 0.0);
    d.kind = DimKind::Angular;
    d.p15 = Vec3::ZERO;
    d.p16 = Vec3::new(-2.692582403567251, 4.663689526544413, 0.0);
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert!((g.measurement - std::f64::consts::FRAC_PI_4).abs() < 1e-12, "{}", g.measurement);
}

#[test]
fn horizontal_measures_dx() {
    let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 3.0), (0.0, 5.0), 0.0), &DimStyle::default(), 1.0);
    assert_eq!(g.value, "10.0000");
    assert_eq!(g.measurement, 10.0);
    assert!(!g.text.is_empty());
    assert_eq!(g.fills.len(), 2);
    assert_eq!(g.lines_of(LineRole::Ext).count(), 2);
}

#[test]
fn vertical_measures_dy() {
    let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 3.0), (15.0, 0.0), std::f64::consts::FRAC_PI_2), &DimStyle::default(), 1.0);
    assert_eq!(g.value, "3.0000");
    assert!((g.measurement - 3.0).abs() < 1e-12, "{}", g.measurement);
}

#[test]
fn override_text() {
    let mut d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    d.text = "<> TYP".into();
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert_eq!(g.value, "10.0000 TYP");
    d.text = " ".into();
    assert_eq!(dimension_geometry(&d, &DimStyle::default(), 1.0).value, "");
}

#[test]
fn iso_suppresses_zeros() {
    let g = dimension_geometry(&lin((0.0, 0.0), (12.5, 0.0), (0.0, 5.0), 0.0), &DimStyle::iso25(), 1.0);
    assert_eq!(g.value, "12.5");
}

#[test]
fn ticks_replace_arrows_and_extend_line() {
    let st = st_with(|s| {
        s.tick_size = 0.1;
        s.dim_line_extend = 0.2;
    });
    let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0), &st, 1.0);
    assert!(g.fills.is_empty(), "no filled arrows with ticks");
    // Two 45° ticks of half-length 0.1 centred on the feet.
    let ticks: Vec<_> = g.lines.iter().filter(|l| l.len() == 2 && ((l[1] - l[0]).angle().to_degrees() - 45.0).abs() < 1e-6).collect();
    assert_eq!(ticks.len(), 2);
    for t in &ticks {
        assert!((t[0].dist(t[1]) - 0.2).abs() < 1e-9);
        assert!((t[0].mid(t[1]).y - 5.0).abs() < 1e-9);
    }
    // The dimension line runs past both extension lines by DIMDLE.
    let b = bbox(&g.lines_of(LineRole::Dim).cloned().collect::<Vec<_>>());
    assert!((b.min.x + 0.2).abs() < 1e-9 && (b.max.x - 10.2).abs() < 1e-9, "{b:?}");
}

#[test]
fn arrowhead_shapes() {
    let tip = Vec2::new(1.0, 1.0);
    let dir = Vec2::X;
    let cf = arrowhead(Arrowhead::ClosedFilled, tip, dir, 1.0);
    assert_eq!(cf.tris.len(), 3);
    assert!(cf.tris.contains(&tip));
    let cb = arrowhead(Arrowhead::ClosedBlank, tip, dir, 1.0);
    assert!(cb.tris.is_empty() && cb.lines.len() == 1 && cb.lines[0].len() == 4);
    assert_eq!(cb.lines[0].first(), cb.lines[0].last());
    let dot = arrowhead(Arrowhead::Dot, tip, dir, 1.0);
    let r = dot.tris.iter().map(|p| p.dist(tip)).fold(0.0, f64::max);
    assert!((r - 0.25).abs() < 1e-9, "dot radius {r}");
    let ob = arrowhead(Arrowhead::Oblique, tip, dir, 1.0);
    assert_eq!(ob.lines.len(), 1);
    assert!(((ob.lines[0][1] - ob.lines[0][0]).angle().to_degrees() - 45.0).abs() < 1e-9);
    let at = arrowhead(Arrowhead::ArchTick, tip, dir, 1.0);
    assert_eq!(at.tris.len(), 6);
    let open = arrowhead(Arrowhead::Open, tip, dir, 1.0);
    assert!(open.tris.is_empty() && open.lines[0].len() == 3 && open.lines[0][1] == tip);
    let o90 = arrowhead(Arrowhead::Open90, tip, dir, 1.0);
    let (a, b) = (o90.lines[0][0] - tip, o90.lines[0][2] - tip);
    assert!(a.dot(b).abs() < 1e-9, "open 90 wings are perpendicular");
    let origin = arrowhead(Arrowhead::Origin, tip, dir, 1.0);
    assert!(origin.lines[0].iter().all(|p| (p.dist(tip) - 0.5).abs() < 1e-9));
    assert_eq!(arrowhead(Arrowhead::None, tip, dir, 1.0), ArrowGeom::default());
    assert_eq!(Arrowhead::parse("_ARCHTICK"), Arrowhead::ArchTick);
    assert_eq!(Arrowhead::parse("Architectural tick"), Arrowhead::ArchTick);
    assert_eq!(Arrowhead::parse("closed blank"), Arrowhead::ClosedBlank);
    assert_eq!(Arrowhead::parse(""), Arrowhead::ClosedFilled);
    assert_eq!(Arrowhead::parse("_ORIGIN"), Arrowhead::Origin);
    assert_eq!(Arrowhead::parse("whatever"), Arrowhead::ClosedFilled);
}

#[test]
fn dimblk_dot_and_none() {
    let st = st_with(|s| s.arrow_block = "_DOT".into());
    let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0), &st, 1.0);
    assert_eq!(g.fills.len(), 48, "two 24-triangle dots");
    let st = st_with(|s| {
        s.separate_arrows = true;
        s.arrow_block1 = "_NONE".into();
        s.arrow_block2 = "_OPEN".into();
    });
    let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0), &st, 1.0);
    assert!(g.fills.is_empty());
    assert!(g.lines.iter().any(|l| l.len() == 3));
}

#[test]
fn text_above_and_centred() {
    let d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    let centred = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert!((centred.text_pos.y - 5.0).abs() < 1e-9);
    // Centred text breaks the dimension line in two.
    assert_eq!(centred.lines_of(LineRole::Dim).count(), 2);
    let above = dimension_geometry(&d, &st_with(|s| s.text_above = 1), 1.0);
    assert!((above.text_pos.y - (5.0 + 0.09 + 0.18 * 0.5)).abs() < 1e-6, "{}", above.text_pos.y);
    assert_eq!(above.lines_of(LineRole::Dim).count(), 1);
    let below = dimension_geometry(&d, &st_with(|s| s.text_above = 4), 1.0);
    assert!(below.text_pos.y < 5.0);
    // Outside: away from the measured points (they are below the line here).
    let outside = dimension_geometry(&d, &st_with(|s| s.text_above = 2), 1.0);
    assert!(outside.text_pos.y > 5.0);
    let d2 = lin((0.0, 10.0), (10.0, 10.0), (0.0, 5.0), 0.0);
    let outside2 = dimension_geometry(&d2, &st_with(|s| s.text_above = 2), 1.0);
    assert!(outside2.text_pos.y < 5.0);
}

#[test]
fn dimtvp_uses_the_dimension_text_height() {
    let d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    let style = st_with(|s| {
        s.text_above = 0;
        s.text_vertical_pos = 2.0;
        s.text_height = 0.18;
    });
    let font = DimText { fixed_height: 0.8, width_factor: 1.0, ..Default::default() };
    let g = dimension_geometry_with(&d, &style, 1.0, &font);
    assert!((g.text_pos.y - 6.6).abs() < 1e-6, "text y={} height={}", g.text_pos.y, g.text_height);
}

#[test]
fn dimtad_one_uses_autocad_text_clearance() {
    let d = lin((0.0, 0.10), (0.0, 0.70), (0.30, 0.40), std::f64::consts::FRAC_PI_2);
    let st = st_with(|s| {
        s.scale = 50.0;
        s.text_height = 0.0025;
        s.text_gap = 0.0008;
        s.arrow_size = 0.0015;
        s.text_above = 1;
        s.text_inside_horizontal = false;
        s.decimals = 2;
    });
    let g = dimension_geometry(&d, &st, 50.0);
    // Half the text height (0.0625) plus DIMGAP (0.04) put the centre at 0.1025: AutoCAD LT 2025 puts the bottom of the text box a gap
    // from the line (offset = gap + box height / 2, box height 1.0 for digits in SHX and 1.02 in Arial, 1.33 and 1.29 with a
    // descender; /tmp/cad/elev-oracle12, 16 dimensions, DIMREGEN changes nothing). An earlier reading of AutoCAD 2026 gave 0.115 (#2004).
    assert!(
        (0.30 - g.text_pos.x - 0.1025).abs() < 0.001,
        "text x={} offset={} angle={} text={}",
        g.text_pos.x,
        0.30 - g.text_pos.x,
        g.text_angle,
        g.value
    );
}

#[test]
fn text_alignment_vertical_dimension() {
    let d = lin((0.0, 0.0), (0.0, 10.0), (5.0, 0.0), std::f64::consts::FRAC_PI_2);
    let h = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert!(h.text_angle.abs() < 1e-12, "DIMTIH on: horizontal");
    let a = dimension_geometry(&d, &st_with(|s| s.text_inside_horizontal = false), 1.0);
    assert!((a.text_angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "aligned: {}", a.text_angle);
}

#[test]
fn justification_moves_text() {
    let d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    let j1 = dimension_geometry(&d, &st_with(|s| s.text_just = 1), 1.0);
    let j2 = dimension_geometry(&d, &st_with(|s| s.text_just = 2), 1.0);
    assert!(j1.text_pos.x < 5.0 && j2.text_pos.x > 5.0);
    let j3 = dimension_geometry(&d, &st_with(|s| s.text_just = 3), 1.0);
    assert!((j3.text_angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    assert!(j3.text_pos.y > 5.0 && j3.text_pos.x < 0.0);
}

#[test]
fn text_moves_outside_when_it_does_not_fit() {
    let d = lin((0.0, 0.0), (0.5, 0.0), (0.0, 5.0), 0.0);
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert!(g.text_pos.x > 0.5, "text beyond the second extension line: {}", g.text_pos.x);
}

#[test]
fn text_that_does_not_fit_follows_dimtmove_2() {
    // Vertical, measured points left of the line: the text goes over the middle of the
    // line, on the side away from them, and the line is not carried out to it.
    let d = lin((0.0, 0.0), (0.0, 0.5), (5.0, 0.0), std::f64::consts::FRAC_PI_2);
    let g = dimension_geometry(&d, &st_with(|s| s.text_move = 2), 1.0);
    assert!((g.text_pos.y - 0.25).abs() < 1e-9 && g.text_pos.x > 5.2, "{:?}", g.text_pos);
    assert!(g.lines_of(LineRole::Dim).flatten().all(|p| p.y <= 0.5 + 1e-9 && p.x <= 5.0 + 1e-9), "no leader, no line out to the text");
    // Horizontal, measured points above the line: below it.
    let d = lin((0.0, 10.0), (0.5, 10.0), (0.0, 5.0), 0.0);
    let g = dimension_geometry(&d, &st_with(|s| s.text_move = 2), 1.0);
    assert!((g.text_pos.x - 0.25).abs() < 1e-9 && g.text_pos.y < 4.8, "{:?}", g.text_pos);
    assert!(g.lines_of(LineRole::Dim).flatten().all(|p| p.x <= 0.5 + 1e-9 && p.y >= 5.0 - 1e-9), "no leader, no line out to the text");
}

#[test]
fn text_that_does_not_fit_follows_dimtmove_1() {
    // Moved off the line with a leader from the middle of the dimension line.
    let d = lin((0.0, 0.0), (0.5, 0.0), (0.0, 5.0), 0.0);
    let g = dimension_geometry(&d, &st_with(|s| s.text_move = 1), 1.0);
    assert!(g.text_pos.y > 5.2, "{:?}", g.text_pos);
    let mid = Vec2::new(0.25, 5.0);
    assert!(g.lines_of(LineRole::Dim).any(|l| l.len() == 2 && l[0].dist(mid) < 1e-9 && l[1].y > 5.1), "leader");
    // Text placed with DIMTEDIT stays where it is.
    let mut placed = d.clone();
    placed.user_text_pos = true;
    placed.text_mid = Vec3::new(3.0, 7.0, 0.0);
    let g = dimension_geometry(&placed, &st_with(|s| s.text_move = 1), 1.0);
    assert!(g.text_pos.dist(Vec2::new(3.0, 7.0)) < 1e-9, "{:?}", g.text_pos);
}

#[test]
fn tolerance_and_limits() {
    let d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    let sym = dimension_geometry(
        &d,
        &st_with(|s| {
            s.tolerance = true;
            s.tol_plus = 0.05;
            s.tol_minus = 0.05;
            s.decimals = 2;
            s.tol_decimals = 2;
        }),
        1.0,
    );
    assert_eq!(sym.value, "10.00±0.05");
    let dev = dimension_geometry(
        &d,
        &st_with(|s| {
            s.tolerance = true;
            s.tol_plus = 0.1;
            s.tol_minus = 0.2;
            s.decimals = 2;
            s.tol_decimals = 1;
        }),
        1.0,
    );
    assert_eq!(dev.value, "10.00+0.1/-0.2");
    assert!(dev.mtext.contains("\\S+0.1^-0.2;"));
    // Stacked: the text extends above and below a single line of text.
    let plain = dimension_geometry(&d, &st_with(|s| s.decimals = 2), 1.0);
    assert!(bbox(&dev.text).height() > bbox(&plain.text).height() * 1.3);
    let lim = dimension_geometry(
        &d,
        &st_with(|s| {
            s.limits = true;
            s.tol_plus = 0.1;
            s.tol_minus = 0.2;
            s.decimals = 1;
        }),
        1.0,
    );
    assert_eq!(lim.value, "10.1/9.8");
}

#[test]
fn unit_formats_and_post() {
    let d = lin((0.0, 0.0), (18.5, 0.0), (0.0, 5.0), 0.0);
    let arch = dimension_geometry(&d, &st_with(|s| s.linear_unit = 4), 1.0);
    assert_eq!(arch.value, "1'-6 1/2\"");
    let frac = dimension_geometry(&d, &st_with(|s| s.linear_unit = 5), 1.0);
    assert_eq!(frac.value, "18 1/2");
    let sci = dimension_geometry(
        &d,
        &st_with(|s| {
            s.linear_unit = 1;
            s.decimals = 2;
        }),
        1.0,
    );
    assert_eq!(sci.value, "1.85E+01");
    let post = dimension_geometry(
        &d,
        &st_with(|s| {
            s.post = "<> mm".into();
            s.decimals = 1;
        }),
        1.0,
    );
    assert_eq!(post.value, "18.5 mm");
    let alt = dimension_geometry(
        &lin((0.0, 0.0), (1.0, 0.0), (0.0, 5.0), 0.0),
        &st_with(|s| {
            s.alt = true;
            s.decimals = 2;
        }),
        1.0,
    );
    assert_eq!(alt.value, "1.00 [25.40]");
}

#[test]
fn overrides_apply_per_dimension() {
    let mut d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    d.overrides.insert("DIMDEC".into(), serde_json::json!(1));
    d.overrides.insert("textAbove".into(), serde_json::json!(1));
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert_eq!(g.value, "10.0");
    assert!(g.text_pos.y > 5.0);
}

#[test]
fn radial_and_angular_text() {
    let mut d = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
    d.kind = DimKind::Radius;
    d.defpt = Vec3::new(0.0, 0.0, 0.0);
    d.p15 = Vec3::new(3.0, 0.0, 0.0);
    let g = dimension_geometry(&d, &st_with(|s| s.decimals = 1), 1.0);
    assert_eq!(g.value, "R3.0");
    d.kind = DimKind::Diameter;
    d.defpt = Vec3::new(-3.0, 0.0, 0.0);
    let g = dimension_geometry(&d, &st_with(|s| s.decimals = 1), 1.0);
    assert_eq!(g.value, "⌀6.0");
    assert_eq!(g.mtext_contents(), "%%c6.0", "the *D block's text (#1645)");
    let mut a = lin((10.0, 0.0), (0.0, 10.0), (3.0, 3.0), 0.0);
    a.kind = DimKind::Angular3P;
    a.p15 = Vec3::ZERO;
    let g = dimension_geometry(&a, &DimStyle::default(), 1.0);
    assert_eq!(g.value, "90°");
    let g = dimension_geometry(
        &a,
        &st_with(|s| {
            s.angular_unit = 3;
            s.angular_decimals = 2;
        }),
        1.0,
    );
    assert_eq!(g.value, "1.57r");
}

#[test]
fn hostile_styles_do_not_panic() {
    let d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
    for st in [
        st_with(|s| s.arrow_size = f64::NAN),
        st_with(|s| s.text_height = -1.0),
        st_with(|s| s.tick_size = f64::INFINITY),
        st_with(|s| s.text_just = 200),
        st_with(|s| s.text_above = 99),
        st_with(|s| s.scale = 0.0),
        st_with(|s| s.decimals = 255),
    ] {
        let _ = dimension_geometry(&d, &st, 1.0);
    }
    let z = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
    let _ = dimension_geometry(&z, &DimStyle::default(), 1.0);
}

#[test]
fn jogged_radius_with_nan_radius_draws_no_lines() {
    let mut d = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
    d.kind = DimKind::Radius;
    d.p15 = Vec3::new(10.0, 0.0, 0.0);
    d.marks.jogged = Some(cadcraft_doc::JoggedRadius { center: Vec3::new(0.0, -4.0, 0.0), jog: Vec3::new(5.0, -2.0, 0.0) });
    // Finite control: the jogged line runs from the arrowhead on the arc to the override centre.
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    let jog = g.lines.iter().find(|l| l.len() == 4).expect("jogged dimension line");
    assert!(jog[0].near(Vec2::new(10.0, 0.0), 1e-9) && jog[3].near(Vec2::new(0.0, -4.0), 1e-9));
    // A NaN in the tip makes the radius NaN: nothing usable to draw, and no panic.
    d.p15 = Vec3::new(f64::NAN, 0.0, 0.0);
    let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
    assert!(g.lines.is_empty() && g.fills.is_empty(), "{:?}", g.lines);
}

/// #1645: the MTEXT contents give back the dimension's text when decoded, with the diameter
/// sign as `%%c` (no SHX font has U+2300) and a `%` that would start a code escaped.
#[test]
fn mtext_contents_write_the_diameter_sign_as_a_code() {
    let with = |v: &str| DimGeometry { value: v.into(), ..DimGeometry::default() };
    for (value, contents) in [
        ("⌀60", "%%c60"),
        ("2x ⌀8.5 THRU", "2x %%c8.5 THRU"),
        ("R3.0", "R3.0"),
        ("36.9° ±0.1", "36.9° ±0.1"),
        ("50%", "50%"),
        ("5%⌀6", "5%%%%%c6"),
        ("%%", "%%%%"),
        ("", ""),
    ] {
        let g = with(value);
        assert_eq!(g.mtext_contents(), contents, "{value:?}");
        assert_eq!(decode_value(&g.mtext_contents()), value, "{value:?} reads back");
        assert!(!g.mtext_contents().contains('⌀'));
    }
}

#[test]
fn text_above_the_line_sits_a_gap_above_its_own_box() {
    // AutoCAD LT 2025: the centre of the text is a gap plus half the height of the text's box from the line, so a string with a
    // descender sits higher than digits (/tmp/cad/elev-oracle12: 0.5 and 0.667 of the text height in SHX).
    let st = st_with(|s| s.text_above = 1);
    let mut offsets = Vec::new();
    for text in ["10", "Agy"] {
        let mut d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
        d.text = text.into();
        let g = dimension_geometry(&d, &st, 1.0);
        let params = cadcraft_fonts::MTextParams { attach: 5, ..cadcraft_fonts::MTextParams::new(Vec2::ZERO, 0.18) };
        let ink = cadcraft_fonts::layout_mtext_with(text, &params).strokes.concat();
        let box_height = ink.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max) - ink.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let offset = g.text_pos.y - 5.0;
        assert!((offset - (0.09 + box_height / 2.0)).abs() < 1e-6, "{text}: offset {offset}, box {box_height}");
        offsets.push(offset);
    }
    assert!(offsets[1] > offsets[0], "a descender raises the text: {offsets:?}");
}

/// A radius dimension of the circle of radius `r` at the origin, towards 45 degrees, with the text above the line (DIMTAD 1) in ISO-25 sizes.
fn radial_beside(r: f64, diameter: bool, inside: bool) -> (Dimension, DimStyle) {
    let u = Vec2::from_angle(std::f64::consts::FRAC_PI_4);
    let mut d = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
    let tip = u * r;
    d.kind = if diameter { DimKind::Diameter } else { DimKind::Radius };
    d.defpt = if diameter { Vec3::new(-tip.x, -tip.y, 0.0) } else { Vec3::ZERO };
    d.p15 = Vec3::new(tip.x, tip.y, 0.0);
    d.user_text_pos = inside;
    d.text_mid = Vec3::new(f64::NAN, f64::NAN, 0.0);
    let st = st_with(|s| {
        s.text_above = 1;
        s.text_height = 2.5;
        s.arrow_size = 2.5;
        s.text_gap = 0.625;
        s.force_line_inside = true;
        s.text_inside_horizontal = false;
        s.text_outside_horizontal = false;
    });
    (d, st)
}

#[test]
fn a_radial_dimension_made_inside_the_circle_puts_the_leader_inward_from_one_arrowhead() {
    // AutoCAD LT 2025 (/tmp/cad/elev-oracle11, radii 4 to 15 with the location inside): one arrowhead inside pointing out, one line from its
    // base inward for an arrowhead, a gap, the text and a gap, no line across the circle, the text above that line.
    for diameter in [false, true] {
        let (d, st) = radial_beside(30.0, diameter, true);
        let g = dimension_geometry(&d, &st, 1.0);
        let u = Vec2::from_angle(std::f64::consts::FRAC_PI_4);
        let (tw, _) = (DimConfig::new(&d, &st, 1.0, &DimText { width_factor: 1.0, ..Default::default() }).measure(&g.mtext).0, 0.0);
        assert_eq!(g.lines.len(), 1, "diameter {diameter}: {:?}", g.lines);
        assert_eq!(g.fills.len(), 1, "one arrowhead");
        let line = &g.lines[0];
        let base = u * (30.0 - 2.5);
        assert!(line[0].dist(base) < 1e-9, "the leader starts at the base of the arrowhead: {line:?}");
        let length = line[0].dist(line[1]);
        assert!((length - (2.5 + 2.0 * 0.625 + tw)).abs() < 1e-9, "length {length}, text {tw}");
        assert!((line[1] - line[0]).normalized().dot(-u) > 1.0 - 1e-9, "inward");
        let along = (g.text_pos - base).dot(-u);
        assert!((along - (2.5 + 0.625 + tw / 2.0)).abs() < 1e-9, "the text starts an arrowhead and a gap along: {along}");
        let above = (g.text_pos - base).dot(u.perp());
        assert!((above - (0.625 + 1.25)).abs() < 1e-6, "above the leader: {above}");
    }
}

#[test]
fn a_moved_radial_text_inside_the_circle_has_the_leader_under_it() {
    let (mut d, st) = radial_beside(30.0, false, true);
    let u = Vec2::from_angle(std::f64::consts::FRAC_PI_4);
    let default = dimension_geometry(&d, &st, 1.0);
    let farther = default.text_pos - u * 3.0;
    d.text_mid = Vec3::new(farther.x, farther.y, 0.0);
    let moved = dimension_geometry(&d, &st, 1.0);
    assert!((moved.text_pos - farther).len() < 1e-9);
    let (before, after) = (default.lines[0][0].dist(default.lines[0][1]), moved.lines[0][0].dist(moved.lines[0][1]));
    assert!((after - before - 3.0).abs() < 1e-9, "{before} -> {after}: {:?} {:?}", default.lines, moved.lines);
}

#[test]
fn a_jog_goes_where_autocad_puts_it_for_every_jog_location() {
    // AutoCAD LT 2025 (/tmp/cad/elev-oracle16): circle of radius 10 at the origin, location towards 45 degrees, override centre (40,15). The
    // jog is vertical at x = the number.
    let cases = [
        ((12.0, 12.0), 17.370),
        ((15.0, 10.0), 18.422),
        ((20.0, 12.0), 22.108),
        ((25.0, 12.0), 24.748),
        ((25.0, 20.0), 28.141),
        ((30.0, 18.0), 30.247),
        ((30.0, 10.0), 25.748),
        ((35.0, 15.0), 30.480),
        ((18.0, 18.0), 23.055),
        ((8.0, 20.0), 16.759),
    ];
    let u = Vec2::from_angle(std::f64::consts::FRAC_PI_4);
    let st = st_with(|s| {
        s.text_above = 1;
        s.text_height = 2.5;
        s.arrow_size = 2.5;
        s.text_gap = 0.625;
    });
    for ((jx, jy), x) in cases {
        let mut d = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
        d.kind = DimKind::Radius;
        d.p15 = Vec3::new(10.0 * u.x, 10.0 * u.y, 0.0);
        d.user_text_pos = true;
        d.text_mid = Vec3::new(16.0, 16.0, 0.0);
        d.marks.jogged = Some(cadcraft_doc::JoggedRadius { center: Vec3::new(40.0, 15.0, 0.0), jog: Vec3::new(jx, jy, 0.0) });
        let g = dimension_geometry(&d, &st, 1.0);
        let path = &g.lines[0];
        assert_eq!(path.len(), 4, "{path:?}");
        assert!((path[1].x - x).abs() < 0.02 && (path[1].y - path[1].x).abs() < 1e-9, "jog at ({jx},{jy}): {:?} against x {x}", path[1]);
    }
    // Past Q the jog is in the middle: the centre of the radius and of the override centre.
    let mut d = lin((0.0, 0.0), (0.0, 0.0), (0.0, 0.0), 0.0);
    d.kind = DimKind::Radius;
    d.p15 = Vec3::new(10.0 * u.x, 10.0 * u.y, 0.0);
    d.user_text_pos = true;
    d.text_mid = Vec3::new(16.0, 16.0, 0.0);
    d.marks.jogged = Some(cadcraft_doc::JoggedRadius { center: Vec3::new(40.0, 15.0, 0.0), jog: Vec3::new(60.0, 60.0, 0.0) });
    let g = dimension_geometry(&d, &st, 1.0);
    let default = (38.89087296526011 + 10.0) / 2.0 + 17.67766952966369 / 2.0;
    assert!((g.lines[0][1].x * std::f64::consts::SQRT_2 - default).abs() < 0.02, "{:?}", g.lines[0]);
}

#[test]
fn narrow_text_above_a_short_line_stays_between_the_extension_lines_with_the_arrowheads_outside() {
    // AutoCAD LT 2025, ISO-25 sizes (arrow 2.5, gap 0.625, text 2.5), DIMTAD 1, span 4.9, text "10": the text stays centred between the
    // extension lines (it fits with a gap on each side) and the arrowheads go outside, their bases at -2.5 and 7.4
    // (/tmp/cad/elev-oracle10/dim1-elev10.dxf, *D26).
    let st = st_with(|s| {
        s.text_height = 2.5;
        s.text_gap = 0.625;
        s.arrow_size = 2.5;
        s.text_above = 1;
    });
    let mut d = lin((0.0, 0.0), (4.9, 0.0), (0.0, 5.0), 0.0);
    d.text = "10".into();
    let g = dimension_geometry_with(&d, &st, 1.0, &DimText { width_factor: 0.5, ..Default::default() });
    assert!((g.text_pos.x - 2.45).abs() < 1e-9, "the text stays centred: {:?}", g.text_pos);
    assert_eq!(g.fills.len(), 2, "{:?}", g.fills);
    for fill in &g.fills {
        let (low, high) = fill.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p.x), h.max(p.x)));
        assert!(high <= 1e-9 || low >= 4.9 - 1e-9, "an arrowhead lies outside the extension lines: x from {low} to {high}");
    }
}

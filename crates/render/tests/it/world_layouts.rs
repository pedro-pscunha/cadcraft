//! The layouts the world builders take (a table and a tolerance in the local coordinates of their frame) are what the
//! renderer lays out today, moved to the origin: the same lines, fills and texts, and the same world primitives for a
//! +Z table and tolerance.

use cadcraft_doc::{
    BreakDirection, CellStyle, Drawing, GdtPrim, Table, TableBreak, TableCell, Tolerance, WorldPrimitive, table_primitives, tolerance_primitives,
};
use cadcraft_geom::{Mat4, Vec2, Vec3};
use cadcraft_render::{table_geometry, table_layout, tolerance_in, tolerance_layout};

fn near2(a: Vec2, b: Vec2) -> bool {
    (a - b).len() < 1e-9
}

fn near3(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

/// A drawing whose table style fills the header cells.
fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    let mut header = CellStyle { name: CellStyle::HEADER.into(), ..CellStyle::default() };
    header.fill = Some(cadcraft_color::Color::Index(3));
    if let Some(style) = d.table_styles.first_mut() {
        style.cell_styles.push(header);
    }
    d
}

fn table(breaks: Option<TableBreak>) -> Table {
    let cell = |s: &str| TableCell { text: s.into(), ..TableCell::default() };
    let mut cells = vec![vec![cell("Title"), cell("")], vec![cell("A"), cell("B")]];
    cells.extend((0..4).map(|i| vec![cell(&format!("r{i}")), TableCell { text: format!("{i}.5"), text_height: Some(0.4), ..TableCell::default() }]));
    cells[0][0].merged = Some((1, 2));
    Table {
        insert: Vec3::new(5.0, 7.0, 3.0),
        col_widths: vec![3.0, 2.0],
        row_heights: vec![1.0; 6],
        cells,
        title: true,
        header: true,
        text_height: 0.3,
        breaks,
        ..Table::default()
    }
}

fn broken() -> TableBreak {
    TableBreak { enabled: true, direction: BreakDirection::Right, height: 3.5, spacing: 1.0, ..TableBreak::default() }
}

#[test]
fn the_table_layout_is_the_rendered_geometry_at_the_origin() {
    let d = drawing();
    for t in [table(None), table(Some(broken()))] {
        let (drawn, layout) = (table_geometry(&d, &t), table_layout(&d, &t));
        let shift = t.insert.xy();
        assert!(!drawn.lines.is_empty() && !drawn.texts.is_empty() && !drawn.fills.is_empty());
        assert_eq!((layout.lines.len(), layout.fills.len(), layout.texts.len()), (drawn.lines.len(), drawn.fills.len(), drawn.texts.len()));
        for (l, w) in layout.lines.iter().zip(&drawn.lines) {
            assert!(near2(l[0] + shift, w.a) && near2(l[1] + shift, w.b), "line {l:?} vs {w:?}");
        }
        for (f, w) in layout.fills.iter().zip(&drawn.fills) {
            assert!(f.iter().zip(&w.corners).all(|(a, b)| near2(*a + shift, *b)), "fill {f:?} vs {w:?}");
        }
        for (x, w) in layout.texts.iter().zip(&drawn.texts) {
            assert!(near2(x.at + shift, w.at), "text {x:?} vs {w:?}");
            assert_eq!((x.attach, x.height, x.style.as_str(), x.text.as_str()), (w.attach, w.height, w.style.as_str(), w.text.as_str()));
        }
        // The top left corner of the grid is the origin.
        let top = layout.lines.iter().flat_map(|l| l.iter()).map(|p| p.y).fold(f64::MIN, f64::max);
        assert!(top.abs() < 1e-9, "the first row starts at y = 0, not {top}");
    }
}

#[test]
fn a_plus_z_table_gives_the_same_world_lines_texts_and_fills_as_it_draws() {
    let d = drawing();
    let t = table(None);
    let (drawn, layout) = (table_geometry(&d, &t), table_layout(&d, &t));
    let prims = table_primitives(&t, Vec3::Z, &layout, &Mat4::IDENTITY).unwrap();
    let z = t.insert.z;
    let lines: Vec<(Vec3, Vec3)> = prims.iter().filter_map(|p| if let WorldPrimitive::Line { a, b } = p { Some((*a, *b)) } else { None }).collect();
    assert_eq!(lines.len(), drawn.lines.len());
    for ((a, b), w) in lines.iter().zip(&drawn.lines) {
        assert!(near3(*a, w.a.to3(z)) && near3(*b, w.b.to3(z)));
    }
    let texts: Vec<_> = prims.iter().filter_map(|p| if let WorldPrimitive::Text(t) = p { Some(t) } else { None }).collect();
    assert_eq!(texts.len(), drawn.texts.len());
    for (x, w) in texts.iter().zip(&drawn.texts) {
        assert!(near3(x.frame.origin, w.at.to3(z)), "{:?} vs {:?}", x.frame.origin, w.at);
    }
    let fills = prims.iter().filter(|p| matches!(p, WorldPrimitive::Fill(_))).count();
    assert_eq!(fills, drawn.fills.len());
}

fn tolerance() -> Tolerance {
    Tolerance {
        insert: Vec3::new(5.0, 7.0, 3.0),
        dir: Vec3::new(0.3f64.cos(), 0.3f64.sin(), 0.0),
        style: "Standard".into(),
        text: "{\\Fgdt;j}%%v0.1%%vA%%v%%v{\\Fgdt;m}".into(),
    }
}

/// The primitive with every point mapped by `f`.
fn mapped(p: &GdtPrim, f: &dyn Fn(Vec2) -> Vec2) -> GdtPrim {
    match p {
        GdtPrim::Line(v) => GdtPrim::Line(v.iter().map(|q| f(*q)).collect()),
        GdtPrim::Circle { center, radius } => GdtPrim::Circle { center: f(*center), radius: *radius },
        GdtPrim::Arc { center, radius, start, end } => GdtPrim::Arc { center: f(*center), radius: *radius, start: start + 0.3, end: end + 0.3 },
        GdtPrim::Fill([a, b, c]) => GdtPrim::Fill([f(*a), f(*b), f(*c)]),
    }
}

fn same(a: &GdtPrim, b: &GdtPrim) -> bool {
    let n = |x: f64, y: f64| (x - y).abs() < 1e-9;
    match (a, b) {
        (GdtPrim::Line(p), GdtPrim::Line(q)) => p.len() == q.len() && p.iter().zip(q).all(|(a, b)| near2(*a, *b)),
        (GdtPrim::Circle { center: c, radius: r }, GdtPrim::Circle { center: d, radius: s }) => near2(*c, *d) && n(*r, *s),
        (GdtPrim::Arc { center: c, radius: r, start: a0, end: a1 }, GdtPrim::Arc { center: d, radius: s, start: b0, end: b1 }) => {
            near2(*c, *d)
                && n(*r, *s)
                && n(a0.rem_euclid(std::f64::consts::TAU), b0.rem_euclid(std::f64::consts::TAU))
                && n(a1.rem_euclid(std::f64::consts::TAU), b1.rem_euclid(std::f64::consts::TAU))
        }
        (GdtPrim::Fill(p), GdtPrim::Fill(q)) => p.iter().zip(q).all(|(a, b)| near2(*a, *b)),
        _ => false,
    }
}

#[test]
fn the_tolerance_layout_is_the_rendered_layout_turned_back_to_the_origin() {
    let d = Drawing::new_metric();
    let t = tolerance();
    let (drawn, local) = (tolerance_in(&d, &t), tolerance_layout(&d, &t));
    assert!(!drawn.frame.is_empty() && !drawn.symbols.is_empty() && !drawn.texts.is_empty());
    let (rotation, insert) = (0.3f64, t.insert.xy());
    let to_world = |p: Vec2| insert + p.rotate(rotation);
    assert_eq!((local.frame.len(), local.symbols.len(), local.texts.len()), (drawn.frame.len(), drawn.symbols.len(), drawn.texts.len()));
    for (l, w) in local.frame.iter().chain(&local.symbols).zip(drawn.frame.iter().chain(&drawn.symbols)) {
        assert!(same(&mapped(l, &to_world), w), "{l:?} vs {w:?}");
    }
    for (l, w) in local.texts.iter().zip(&drawn.texts) {
        assert!(near2(to_world(l.pos), w.pos) && (l.height - w.height).abs() < 1e-9 && l.value == w.value);
        assert!(l.rotation.abs() < 1e-12, "the local text is not rotated");
    }
}

#[test]
fn a_plus_z_tolerance_gives_the_same_world_lines_as_it_draws() {
    let d = Drawing::new_metric();
    let mut t = tolerance();
    t.dir = Vec3::new(1.0, 0.0, 0.0);
    let (drawn, local) = (tolerance_in(&d, &t), tolerance_layout(&d, &t));
    let prims = tolerance_primitives(&t, Vec3::Z, &local, "Standard", &Mat4::IDENTITY).unwrap();
    let z = t.insert.z;
    let lines: Vec<(Vec3, Vec3)> = prims.iter().filter_map(|p| if let WorldPrimitive::Line { a, b } = p { Some((*a, *b)) } else { None }).collect();
    // The frame boxes and the symbol strokes drawn as polylines are consecutive segments in both.
    let expected: Vec<(Vec2, Vec2)> = drawn
        .frame
        .iter()
        .chain(&drawn.symbols)
        .filter_map(|p| if let GdtPrim::Line(v) = p { Some(v.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>()) } else { None })
        .flatten()
        .collect();
    assert_eq!(lines.len(), expected.len());
    for ((a, b), (ea, eb)) in lines.iter().zip(&expected) {
        assert!(near3(*a, ea.to3(z)) && near3(*b, eb.to3(z)));
    }
    let texts = prims.iter().filter(|p| matches!(p, WorldPrimitive::Text(_))).count();
    assert_eq!(texts, drawn.texts.len());
}

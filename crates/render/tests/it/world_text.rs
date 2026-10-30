//! A world text is shaped in local glyph coordinates and mapped once through its complete affine frame: at +Z it is the text
//! the renderer draws, the entity's generation flags join the style's, a shear or non-uniform scale in the parent maps the
//! layout without scaling any length twice, and a multiline text keeps its colour runs and columns.

use cadcraft_doc::{
    Drawing, HAlign, MText, MTextColumnType, MTextColumns, Text, VAlign, WorldPrimitive, WorldText, WorldTextContent, mtext_primitives,
    text_primitives,
};
use cadcraft_fonts::Shaped;
use cadcraft_geom::{Mat4, Vec2, Vec3};
use cadcraft_render::{WorldTextLayout, layout_mtext_entity, layout_world_text, place_text_entity};

fn text(halign: HAlign, valign: VAlign, align: bool) -> Text {
    Text {
        insert: Vec3::new(5.0, 7.0, 0.0),
        align_pt: align.then(|| Vec3::new(11.0, 9.0, 0.0)),
        height: 2.0,
        value: "Hello".into(),
        rotation: 0.3,
        width_factor: 0.8,
        oblique: 0.2,
        style: "Standard".into(),
        halign,
        valign,
        backward: false,
        upside_down: false,
    }
}

fn world_text(t: &Text, parent: &Mat4) -> WorldText {
    let primitives = text_primitives(t, Vec3::Z, parent).unwrap();
    let Some(WorldPrimitive::Text(w)) = primitives.into_iter().next() else { panic!("no text") };
    w
}

fn near2(a: Vec2, b: Vec2) -> bool {
    (a - b).len() < 1e-9
}

fn same_shape(a: &Shaped, b: &Shaped) -> bool {
    let strokes = a.strokes.len() == b.strokes.len()
        && a.strokes.iter().zip(&b.strokes).all(|(p, q)| p.len() == q.len() && p.iter().zip(q).all(|(x, y)| near2(*x, *y)));
    let glyphs = a.glyphs.len() == b.glyphs.len()
        && a.glyphs.iter().flatten().flatten().count() == b.glyphs.iter().flatten().flatten().count()
        && a.glyphs.iter().flatten().flatten().zip(b.glyphs.iter().flatten().flatten()).all(|(p, q)| near2(*p, *q))
        && a.text.len() == b.text.len()
        && a.text.iter().zip(&b.text).all(|(g, h)| near2(g.origin, h.origin) && near2(g.x, h.x) && near2(g.y, h.y));
    strokes && glyphs
}

fn single(layout: WorldTextLayout) -> cadcraft_render::WorldShaped {
    let WorldTextLayout::Text(s) = layout else { panic!("not a single-line layout") };
    s
}

#[test]
fn a_plus_z_text_is_the_text_the_renderer_draws_for_every_alignment() {
    let d = Drawing::new_metric();
    let cases = [
        (HAlign::Left, VAlign::Baseline, false),
        (HAlign::Center, VAlign::Middle, false),
        (HAlign::Right, VAlign::Top, false),
        (HAlign::Middle, VAlign::Bottom, false),
        (HAlign::Aligned, VAlign::Baseline, true),
        (HAlign::Fit, VAlign::Baseline, true),
    ];
    for (h, v, align) in cases {
        let t = text(h, v, align);
        let wt = world_text(&t, &Mat4::IDENTITY);
        let shown = single(layout_world_text(&d, &wt, &t.value).unwrap()).project_xy();
        let (drawn, _) = place_text_entity(&d, &t, &t.value);
        assert!(!drawn.strokes.is_empty() || !drawn.glyphs.is_empty(), "{h:?} {v:?}: something is drawn");
        assert!(same_shape(&shown, &drawn), "{h:?} {v:?} align {align}");
    }
}

#[test]
fn the_entity_generation_flags_mirror_like_the_style_flags() {
    let mut styled = Drawing::new_metric();
    let plain = Drawing::new_metric();
    for (backward, upside_down) in [(true, false), (false, true), (true, true)] {
        let t = Text { backward, upside_down, ..text(HAlign::Left, VAlign::Baseline, false) };
        let by_entity = single(layout_world_text(&plain, &world_text(&t, &Mat4::IDENTITY), &t.value).unwrap()).project_xy();
        styled.text_styles.iter_mut().for_each(|s| (s.backwards, s.upside_down) = (backward, upside_down));
        let flagless = Text { backward: false, upside_down: false, ..t.clone() };
        let by_style = single(layout_world_text(&styled, &world_text(&flagless, &Mat4::IDENTITY), &t.value).unwrap()).project_xy();
        assert!(same_shape(&by_entity, &by_style), "backward {backward} upside down {upside_down}");
        let unflipped = single(layout_world_text(&plain, &world_text(&flagless, &Mat4::IDENTITY), &t.value).unwrap()).project_xy();
        assert!(!same_shape(&by_entity, &unflipped), "the flags change the text");
    }
}

/// A rotation about a tilted axis with a non-uniform scale, a shear and a translation.
fn affine() -> Mat4 {
    let (s, c) = 0.7f64.sin_cos();
    let rotation = Mat4 { m: [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let tilt = Mat4 { m: [[1.0, 0.0, 0.0, 0.0], [0.0, 0.8, -0.6, 0.0], [0.0, 0.6, 0.8, 0.0], [0.0, 0.0, 0.0, 1.0]] };
    let stretch = Mat4 { m: [[2.0, 0.5, 0.0, 3.0], [0.0, 1.0, 0.0, -2.0], [0.0, 0.25, 3.0, 4.0], [0.0, 0.0, 0.0, 1.0]] };
    stretch.mul(&tilt.mul(&rotation))
}

#[test]
fn the_frame_maps_the_layout_once_whatever_the_parent_does() {
    let d = Drawing::new_metric();
    let t = text(HAlign::Center, VAlign::Middle, false);
    let parent = affine();
    let before = single(layout_world_text(&d, &world_text(&t, &Mat4::IDENTITY), &t.value).unwrap());
    let after = single(layout_world_text(&d, &world_text(&t, &parent), &t.value).unwrap());
    assert_eq!(before.width, after.width, "the local width is not scaled by the parent");
    assert!(!before.strokes.is_empty());
    for (a, b) in before.strokes.iter().flatten().zip(after.strokes.iter().flatten()) {
        assert!((parent.apply(*a) - *b).len() < 1e-8, "{a:?} -> {b:?}");
    }
    for (g, h) in before.text.iter().zip(&after.text) {
        assert!(
            (parent.apply(g.origin) - h.origin).len() < 1e-8
                && (parent.apply_vec(g.x) - h.x).len() < 1e-8
                && (parent.apply_vec(g.y) - h.y).len() < 1e-8
        );
    }
}

#[test]
fn a_justified_text_keeps_its_alignment_point_under_the_parent() {
    let d = Drawing::new_metric();
    let t = text(HAlign::Aligned, VAlign::Baseline, true);
    let parent = affine();
    let before = single(layout_world_text(&d, &world_text(&t, &Mat4::IDENTITY), &t.value).unwrap());
    let after = single(layout_world_text(&d, &world_text(&t, &parent), &t.value).unwrap());
    for (a, b) in before.strokes.iter().flatten().zip(after.strokes.iter().flatten()) {
        assert!((parent.apply(*a) - *b).len() < 1e-8, "the aligned text is stretched between the mapped points");
    }
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(5.0, 7.0, 0.0),
        height: 1.0,
        width: 12.0,
        attach: 5,
        rotation: 0.4,
        style: "Standard".into(),
        contents: "{\\C1;red words that wrap}\\Pplain second paragraph with more words to fill the columns".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: Some(MTextColumns { kind: MTextColumnType::Static, count: 2, gutter: 1.0, height: 3.0, ..MTextColumns::default() }),
        direction: Some(Vec3::new(0.4f64.cos(), 0.4f64.sin(), 0.0)),
    }
}

fn world_mtext(m: &MText, parent: &Mat4) -> WorldText {
    let Some(WorldPrimitive::Text(w)) = mtext_primitives(m, Vec3::Z, parent).unwrap().into_iter().next() else { panic!("no text") };
    w
}

#[test]
fn a_multiline_text_keeps_its_colour_runs_and_columns() {
    let d = Drawing::new_metric();
    let m = mtext();
    let WorldTextLayout::MText(pieces) = layout_world_text(&d, &world_mtext(&m, &Mat4::IDENTITY), "").unwrap() else { panic!("not multiline") };
    let drawn = layout_mtext_entity(&d, &m);
    assert!(pieces.len() >= 2, "a colour run and the plain text: {} pieces", pieces.len());
    assert_eq!(pieces.len(), drawn.pieces.len());
    for (w, p) in pieces.iter().zip(&drawn.pieces) {
        assert_eq!(w.color, p.color);
        assert!(same_shape(&w.shaped.project_xy(), &p.shaped), "the piece is where the renderer puts it, columns included");
    }
    assert!(pieces.iter().any(|p| p.color.is_some()) && pieces.iter().any(|p| p.color.is_none()));
    // Under an affine parent every point is the mapped one.
    let parent = affine();
    let WorldTextLayout::MText(moved) = layout_world_text(&d, &world_mtext(&m, &parent), "").unwrap() else { panic!("not multiline") };
    for (a, b) in pieces.iter().zip(&moved) {
        assert_eq!(a.color, b.color);
        for (p, q) in a.shaped.strokes.iter().flatten().zip(b.shaped.strokes.iter().flatten()) {
            assert!((parent.apply(*p) - *q).len() < 1e-8);
        }
    }
}

#[test]
fn a_frame_that_spans_no_plane_is_refused() {
    let d = Drawing::new_metric();
    let t = text(HAlign::Left, VAlign::Baseline, false);
    let mut wt = world_text(&t, &Mat4::IDENTITY);
    wt.frame.y_axis = wt.frame.x_axis * 2.0;
    assert!(layout_world_text(&d, &wt, "x").is_err());
    let mut nan = world_text(&t, &Mat4::IDENTITY);
    nan.frame.origin.x = f64::NAN;
    assert!(layout_world_text(&d, &nan, "x").is_err());
    let _ = WorldTextContent::Text(t);
}

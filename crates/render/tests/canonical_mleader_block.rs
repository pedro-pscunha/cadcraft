//! A canonical multileader with block content paints like a block reference of its own: its leader lines and text, then the block
//! placed by the block matrix with the multileader's colour for ByBlock, then its attributes (kept in block coordinates). Checked
//! on a tilted plane with XY values worked out independently, with a ByBlock colour, and against the legacy painter in the XY plane.

use std::sync::Arc;

use cadcraft_color::Color;
use cadcraft_doc::{
    Attrib, Block, Drawing, Entity, EntityKind, GeometryRevision, HAlign, Handle, Insert, Line, MLeader, MLeaderContext, Space, Text, VAlign,
};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_render::{DisplayList, Kind, Options, build};

fn text() -> Text {
    Text {
        insert: Vec3::new(0.5, 0.5, 0.0),
        align_pt: None,
        height: 0.4,
        value: "7".into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        backward: false,
        upside_down: false,
    }
}

/// A multileader at the world point (10, 2, 3) with block "B" (a line from the origin to (1, 0, 0) with ByBlock colour and an
/// attribute definition TAG), scale 2, in the plane of `normal`.
fn drawing(canonical: bool, normal: Vec3, rotation: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    let mut block = Block::new("B");
    let mut line = Entity::new(Handle(0x500), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) }));
    line.common.color = Color::ByBlock;
    let mut definition = Attrib::new("TAG", text());
    definition.invisible = false;
    let definition = Entity::new(Handle(0x501), EntityKind::AttDef(definition));
    if canonical {
        line.geometry_revision = GeometryRevision::CanonicalV2;
    }
    block.entities.push(line);
    block.entities.push(definition);
    d.blocks.insert("B".into(), Arc::new(block));
    let mut attrib = Attrib::new("TAG", text());
    if canonical {
        attrib.geometry_revision = GeometryRevision::CanonicalV2;
    } else {
        // Placed like its definition by the block reference: scale 2, rotation 0 here.
        attrib.text.insert = Vec3::new(10.0 + 1.0, 2.0 + 1.0, 0.0);
        attrib.text.height = 0.8;
    }
    let content = Insert {
        block: "B".into(),
        insert: Vec3::new(10.0, 2.0, if canonical { 3.0 } else { 0.0 }),
        scale: Vec3::new(2.0, 2.0, 2.0),
        rotation,
        attribs: vec![attrib],
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let mut m = MLeader::single(
        Vec3::new(8.0, 5.0, if canonical { 3.0 } else { 0.0 }),
        1.0,
        true,
        vec![vec![Vec3::new(2.0, 8.0, if canonical { 3.0 } else { 0.0 }), Vec3::new(6.0, 6.0, if canonical { 3.0 } else { 0.0 })]],
        None,
    );
    m.block = Some(content);
    if canonical {
        m.context = MLeaderContext {
            origin: Vec3::new(0.0, 0.0, 3.0),
            x_axis: Vec3::new(1.0, 0.0, 0.0),
            y_axis: Vec3::new(0.0, 1.0, 0.0),
            block_normal: normal,
            geometry_revision: GeometryRevision::CanonicalV2,
            ..MLeaderContext::default()
        };
    }
    let mut e = Entity::new(Handle(0x502), EntityKind::MLeader(m));
    e.common.color = Color::Index(1);
    if canonical {
        e.geometry_revision = GeometryRevision::CanonicalV2;
    }
    d.model.push(e);
    d
}

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).len() < 1e-9
}

fn polylines(list: &DisplayList) -> Vec<(Vec<Vec2>, cadcraft_color::Rgb)> {
    list.prims.iter().filter(|p| p.kind == Kind::Polyline).map(|p| (list.points(p).to_vec(), p.color)).collect()
}

#[test]
fn the_block_of_a_canonical_multileader_is_placed_by_its_block_matrix_on_a_tilted_plane() {
    // Block normal (0, 3, 4): OCS X is world -X and OCS Y is (0, -0.8, 0.6). With rotation 0.5 the block X axis is
    // 2 * (-cos, -0.8 sin, 0.6 sin): the child line runs from the insertion point (10, 2) to (10, 2) + (-2 cos 0.5, -1.6 sin 0.5).
    let list = build(&drawing(true, Vec3::new(0.0, 3.0, 4.0), 0.5), &Space::Model, &Options::default());
    let lines = polylines(&list);
    let end = Vec2::new(10.0 - 2.0 * 0.5f64.cos(), 2.0 - 1.6 * 0.5f64.sin());
    assert!(lines.iter().any(|(l, _)| l.len() == 2 && near(l[0], Vec2::new(10.0, 2.0)) && near(l[1], end)), "{lines:?}");
}

#[test]
fn byblock_takes_the_colour_of_the_multileader() {
    let list = build(&drawing(true, Vec3::new(0.0, 3.0, 4.0), 0.5), &Space::Model, &Options::default());
    let red = cadcraft_color::Color::Index(1).resolve(Color::Index(7), Color::Index(7));
    let lines = polylines(&list);
    // The leader lines, the dogleg and the block line (ByBlock) are the multileader's colour; the attribute takes its definition's.
    let (strokes, others): (Vec<_>, Vec<_>) = lines.iter().partition(|(l, _)| l.len() == 2);
    assert!(strokes.len() >= 4 && strokes.iter().all(|(_, c)| *c == red), "{lines:?}");
    assert!(others.iter().all(|(_, c)| *c != red), "the attribute follows its definition: {lines:?}");
}

/// The segments of every polyline with its colour, rounded and sorted: a leader drawn as one polyline or as one per segment is the
/// same picture.
fn segments(list: &DisplayList) -> Vec<(i64, i64, i64, i64, [u8; 3])> {
    let key = |v: f64| (v * 1e9).round() as i64;
    let mut out: Vec<_> = polylines(list)
        .into_iter()
        .flat_map(|(points, color)| {
            points
                .windows(2)
                .map(|w| {
                    let (a, b) = if (key(w[0].x), key(w[0].y)) <= (key(w[1].x), key(w[1].y)) { (w[0], w[1]) } else { (w[1], w[0]) };
                    (key(a.x), key(a.y), key(b.x), key(b.y), [color.0, color.1, color.2])
                })
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn in_the_xy_plane_the_canonical_painting_is_the_legacy_one() {
    let legacy = build(&drawing(false, Vec3::Z, 0.0), &Space::Model, &Options::default());
    let canonical = build(&drawing(true, Vec3::Z, 0.0), &Space::Model, &Options::default());
    let (a, b) = (segments(&legacy), segments(&canonical));
    assert!(a.len() > 4, "leader lines, dogleg, block line and the attribute strokes: {a:?}");
    assert_eq!(a, b);
}

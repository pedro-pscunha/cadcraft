//! A multileader with two roots keeps them apart through the commands that edit its lines and its content: a
//! stretched vertex grip moves one vertex of one root, a landing grip moves that root's landing (the first root's
//! also moves the content), MLEADEREDIT adds lines to the first root and removes one by its place over the roots,
//! and a move takes every root with it.
//! Native grip editing of a multileader's branch is not measured in AutoCAD: these assert CADCraft's own contract.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle, MLeader, MLeaderBranch, MLeaderRoot, MText};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::grips::GripMode;
use serde_json::json;

fn v(x: f64, y: f64) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

/// Roots 7 at (10, 20) with one line and 9 at (-10, 18) with one line; text at the first root's dogleg end.
fn two_roots(s: &mut Session) -> Handle {
    let text = MText {
        insert: v(12.0, 20.0),
        height: 0.4,
        width: 0.0,
        attach: 4,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Two roots".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
    };
    let mut m = MLeader::single(v(10.0, 20.0), 2.0, false, vec![vec![v(0.0, 0.0), v(4.0, 6.0)]], Some(text));
    m.roots[0].id = 7;
    m.roots[0].branches[0].id = 11;
    m.roots.push(MLeaderRoot {
        id: 9,
        landing: v(-10.0, 18.0),
        dogleg_dir: v(-1.0, 0.0),
        dogleg: 3.0,
        branches: vec![MLeaderBranch { id: 21, vertices: vec![v(-2.0, 0.0), v(-6.0, 5.0)] }],
    });
    s.add_entity(EntityKind::MLeader(m)).unwrap()
}

fn get(s: &Session, h: Handle) -> MLeader {
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::MLeader(m) => m.clone(),
        k => panic!("not a multileader: {k:?}"),
    }
}

#[test]
fn the_grips_are_the_vertices_of_every_line_then_one_landing_per_root() {
    let mut s = Session::new();
    let h = two_roots(&mut s);
    let grips = s.grips_of(h).unwrap();
    let want = [v(0.0, 0.0), v(4.0, 6.0), v(-2.0, 0.0), v(-6.0, 5.0), v(10.0, 20.0), v(-10.0, 18.0)];
    assert_eq!(grips.len(), want.len(), "{grips:?}");
    for (g, w) in grips.iter().zip(want) {
        assert!(g.near(w.xy(), 1e-9), "{grips:?}");
    }
}

#[test]
fn a_vertex_grip_of_one_root_leaves_the_other_root_alone() {
    let mut s = Session::new();
    let h = two_roots(&mut s);
    let before = get(&s, h);
    s.grip_edit(h, 3, Vec2::new(-7.0, 6.0), GripMode::Stretch).unwrap();
    let m = get(&s, h);
    assert_eq!(m.roots[1].branches[0].vertices[1], v(-7.0, 6.0));
    assert_eq!(m.roots[0], before.roots[0], "the first root is as it was");
    assert_eq!((m.roots[1].id, m.roots[1].landing, m.roots[1].dogleg, m.roots[1].dogleg_dir), (9, v(-10.0, 18.0), 3.0, v(-1.0, 0.0)));
    assert_eq!(m.text, before.text);
}

#[test]
fn a_landing_grip_moves_its_root_and_only_the_first_root_moves_the_content() {
    let mut s = Session::new();
    let h = two_roots(&mut s);
    let before = get(&s, h);
    // The second root's landing.
    s.grip_edit(h, 5, Vec2::new(-12.0, 19.0), GripMode::Stretch).unwrap();
    let m = get(&s, h);
    assert!(m.roots[1].landing.xy().near(Vec2::new(-12.0, 19.0), 1e-9));
    assert_eq!((m.roots[0].landing, m.text.clone()), (before.roots[0].landing, before.text.clone()), "the content stays");
    assert_eq!(m.roots[1].branches, before.roots[1].branches);
    // The first root's landing takes the content with it.
    s.grip_edit(h, 4, Vec2::new(11.0, 22.0), GripMode::Stretch).unwrap();
    let m = get(&s, h);
    assert!(m.roots[0].landing.xy().near(Vec2::new(11.0, 22.0), 1e-9));
    assert!(m.text.unwrap().insert.xy().near(Vec2::new(13.0, 22.0), 1e-9));
}

#[test]
fn mleaderedit_counts_the_lines_over_the_roots() {
    let mut s = Session::new();
    let h = two_roots(&mut s);
    let handle = format!("{:X}", h.0);
    // A new line joins the first root with a new branch number.
    s.execute("mleaderedit", &json!({"handle": handle, "mode": "add", "points": [[1, 1]]})).unwrap();
    let m = get(&s, h);
    assert_eq!(m.roots[0].branches.len(), 2);
    assert!(m.roots[0].branches[1].id != 11 && m.roots[0].branches[1].id != 21, "{:?}", m.roots[0].branches);
    assert_eq!(m.roots[1].branches.len(), 1);
    // Line 2 over the roots is the second root's.
    s.execute("mleaderedit", &json!({"handle": handle, "mode": "remove", "index": 2})).unwrap();
    let m = get(&s, h);
    assert_eq!((m.roots[0].branches.len(), m.roots[1].branches.len()), (2, 0));
    assert_eq!(m.roots[1].landing, v(-10.0, 18.0));
}

#[test]
fn a_move_takes_every_root_with_it() {
    let mut s = Session::new();
    let h = two_roots(&mut s);
    s.execute("move", &json!({"handles": [format!("{:X}", h.0)], "delta": [5, -3]})).unwrap();
    let m = get(&s, h);
    assert_eq!((m.roots[0].landing, m.roots[1].landing), (v(15.0, 17.0), v(-5.0, 15.0)));
    assert_eq!(m.roots[1].branches[0].vertices[0], v(3.0, -3.0));
    assert_eq!((m.roots[0].dogleg, m.roots[1].dogleg, m.roots[1].dogleg_dir), (2.0, 3.0, v(-1.0, 0.0)));
}

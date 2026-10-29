//! A multileader keeps its leader roots apart: each root has its own number, landing, dogleg direction and length,
//! and its leader lines (branches) have numbers of their own. Editing one line leaves the sibling root as it was,
//! in a DXF, in a DWG and in the second save; every root and context field of the model reaches the output
//! (the source is not replayed over an edit), and a property edit keeps the source's groups.

use cadcraft_doc::{Drawing, EntityKind, Handle, MLeader};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, read_dxf, write_with};

/// The two-root multileader of the reference fixture mleader-two-roots-001: roots 7 and 9, landings (10, 20) and
/// (-10, 18), doglegs 2 and 3 to the right and left, branches 11 and 21.
const SOURCE: &str = "0 MULTILEADER\n5 A0\n8 0\n100 AcDbEntity\n100 AcDbMLeader\n270 2\n300 CONTEXT_DATA{\n40 1.0\n10 12.0\n20 20.0\n30 0.0\n\
    41 0.4\n140 0.18\n145 0.2\n174 2\n175 2\n176 0\n177 1\n290 1\n304 Two roots\n11 0.0\n21 0.0\n31 1.0\n12 0.0\n22 24.0\n32 0.0\n\
    13 1.0\n23 0.0\n33 0.0\n42 0.0\n43 10.0\n44 0.0\n45 1.0\n170 1\n171 1\n172 1\n296 0\n110 0.0\n120 0.0\n130 0.0\n111 1.0\n121 0.0\n131 0.0\n\
    112 0.0\n122 1.0\n132 0.0\n297 0\n\
    302 LEADER{\n290 1\n291 1\n10 10.0\n20 20.0\n30 0.0\n11 1.0\n21 0.0\n31 0.0\n90 7\n40 2.0\n304 LEADER_LINE{\n10 0.0\n20 0.0\n30 0.0\n10 4.0\n20 6.0\n30 0.0\n91 11\n305 }\n271 0\n303 }\n\
    302 LEADER{\n290 1\n291 1\n10 -10.0\n20 18.0\n30 0.0\n11 -1.0\n21 0.0\n31 0.0\n90 9\n40 3.0\n304 LEADER_LINE{\n10 -2.0\n20 0.0\n30 0.0\n10 -6.0\n20 5.0\n30 0.0\n91 21\n305 }\n271 0\n303 }\n\
    272 9\n273 9\n301 }\n170 1\n291 1\n41 2.0\n42 0.18\n172 2\n";

fn source() -> String {
    let mut text = String::from("0\nSECTION\n2\nENTITIES\n");
    for line in SOURCE.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        text.push_str(&format!("{code}\n{value}\n"));
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

fn drawing() -> Drawing {
    read_dxf(source().as_bytes()).unwrap()
}

fn handle(d: &Drawing) -> Handle {
    d.model.iter().find(|e| matches!(e.kind, EntityKind::MLeader(_))).unwrap().handle
}

fn mleader(d: &Drawing) -> &MLeader {
    d.model.iter().find_map(|e| if let EntityKind::MLeader(m) = &e.kind { Some(m) } else { None }).expect("a multileader")
}

fn save(d: &Drawing) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap()
}

fn edit(d: &mut Drawing, f: impl FnOnce(&mut MLeader)) {
    let h = handle(d);
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            f(m);
        }
    })
    .unwrap();
}

/// The groups of the MULTILEADER record in `text` after its `AcDbMLeader` marker.
fn groups(text: &str) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let record: Vec<Tag> = records(&section.tags).into_iter().find(|(k, _)| k == "MULTILEADER").unwrap().1;
    let start = record.iter().position(|t| t.code == 100 && t.str() == "AcDbMLeader").unwrap();
    record[start + 1..].iter().map(|t| (t.code, t.str())).collect()
}

/// The groups of leader root `n` (from its `LEADER{` to its closing `}`).
fn root(g: &[(i32, String)], n: usize) -> Vec<(i32, String)> {
    let starts: Vec<usize> = (0..g.len()).filter(|i| g[*i].0 == 302).collect();
    let from = starts[n];
    let to = (from..g.len()).find(|i| g[*i].0 == 303).unwrap();
    g[from..=to].to_vec()
}

/// The groups of the content context before its first root.
fn context(g: &[(i32, String)]) -> Vec<(i32, String)> {
    let end = g.iter().position(|t| t.0 == 302).unwrap();
    g[..end].to_vec()
}

fn num(g: &[(i32, String)], code: i32, nth: usize) -> f64 {
    g.iter().filter(|t| t.0 == code).nth(nth).unwrap_or_else(|| panic!("no group {code} #{nth} in {g:?}")).1.trim().parse().unwrap()
}

#[test]
fn the_two_roots_of_the_reference_fixture_are_read_apart() {
    let d = drawing();
    let m = mleader(&d);
    assert_eq!(m.roots.len(), 2);
    let r = &m.roots;
    assert_eq!((r[0].id, r[0].landing, r[0].dogleg_dir, r[0].dogleg), (7, Vec3::new(10.0, 20.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 2.0));
    assert_eq!((r[1].id, r[1].landing, r[1].dogleg_dir, r[1].dogleg), (9, Vec3::new(-10.0, 18.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), 3.0));
    assert_eq!(r[0].branches.iter().map(|b| b.id).collect::<Vec<_>>(), [11]);
    assert_eq!(r[1].branches.iter().map(|b| b.id).collect::<Vec<_>>(), [21]);
    assert_eq!(r[0].branches[0].vertices, [Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 6.0, 0.0)]);
    assert_eq!(r[1].branches[0].vertices, [Vec3::new(-2.0, 0.0, 0.0), Vec3::new(-6.0, 5.0, 0.0)]);
    assert_eq!(m.context.text_normal, Vec3::new(0.0, 0.0, 1.0));
}

#[test]
fn a_vertex_edit_of_one_branch_keeps_the_sibling_root_in_every_output() {
    let want = root(&groups(&source()), 1);
    let mut d = drawing();
    edit(&mut d, |m| m.roots[0].branches[0].vertices[1] = Vec3::new(5.0, 7.0, 0.0));
    let first = save(&d);
    let g = groups(&first);
    // The edited root has the new vertex, the sibling root has its source groups.
    assert_eq!(root(&g, 0).iter().filter(|t| t.0 == 10).map(|t| t.1.parse::<f64>().unwrap()).collect::<Vec<_>>(), [10.0, 0.0, 5.0]);
    assert_eq!(root(&g, 1), want, "the sibling root of the DXF");
    // The second save is the same.
    let second = save(&read_dxf(first.as_bytes()).unwrap());
    assert_eq!(groups(&second), g, "the second save");
    // A DWG keeps both roots with their own landing, dogleg and numbers.
    let dwg = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    let m = mleader(&dwg);
    assert_eq!(m.roots.len(), 2);
    assert_eq!((m.roots[0].landing, m.roots[0].dogleg), (Vec3::new(10.0, 20.0, 0.0), 2.0));
    assert_eq!((m.roots[1].landing, m.roots[1].dogleg, m.roots[1].dogleg_dir), (Vec3::new(-10.0, 18.0, 0.0), 3.0, Vec3::new(-1.0, 0.0, 0.0)));
    assert_eq!(m.roots[0].branches[0].vertices[1], Vec3::new(5.0, 7.0, 0.0));
    assert_eq!(m.roots[1].branches[0].vertices, [Vec3::new(-2.0, 0.0, 0.0), Vec3::new(-6.0, 5.0, 0.0)]);
}

#[test]
fn a_property_edit_and_an_identity_keep_the_source_groups() {
    let want = groups(&source());
    let mut d = drawing();
    edit(&mut d, |m| m.arrow_size = 0.5);
    let g = groups(&save(&d));
    assert_eq!((root(&g, 0), root(&g, 1)), (root(&want, 0), root(&want, 1)));
    let (cx, wx) = (context(&g), context(&want));
    assert_eq!(
        cx.iter().filter(|t| matches!(t.0, 10 | 12 | 13 | 110 | 111 | 112)).collect::<Vec<_>>(),
        wx.iter().filter(|t| matches!(t.0, 10 | 12 | 13 | 110 | 111 | 112)).collect::<Vec<_>>()
    );
    let again = groups(&save(&read_dxf(save(&drawing()).as_bytes()).unwrap()));
    assert_eq!((root(&again, 0), root(&again, 1)), (root(&want, 0), root(&want, 1)));
}

type Mutation = fn(&mut MLeader);
type Check = fn(&[(i32, String)]) -> bool;

/// Each root and context field changed alone must reach the output, not the source's value.
#[test]
fn every_root_and_context_field_reaches_the_output_when_it_changes() {
    let r1 = |g: &[(i32, String)]| root(g, 1);
    let cases: Vec<(&str, Mutation, Check)> = vec![
        ("root number", |m| m.roots[1].id = 12, |g| num(&root(g, 1), 90, 0) == 12.0),
        ("landing x", |m| m.roots[1].landing.x = -11.5, |g| num(&root(g, 1), 10, 0) == -11.5),
        ("landing y", |m| m.roots[1].landing.y = 17.25, |g| num(&root(g, 1), 20, 0) == 17.25),
        ("landing z", |m| m.roots[1].landing.z = 0.75, |g| num(&root(g, 1), 30, 0) == 0.75),
        ("dogleg direction x", |m| m.roots[1].dogleg_dir.x = -0.6, |g| num(&root(g, 1), 11, 0) == -0.6),
        ("dogleg direction y", |m| m.roots[1].dogleg_dir.y = 0.8, |g| num(&root(g, 1), 21, 0) == 0.8),
        ("dogleg direction z", |m| m.roots[1].dogleg_dir.z = 0.25, |g| num(&root(g, 1), 31, 0) == 0.25),
        ("dogleg length", |m| m.roots[1].dogleg = 3.5, |g| num(&root(g, 1), 40, 0) == 3.5),
        ("branch number", |m| m.roots[1].branches[0].id = 22, |g| num(&root(g, 1), 91, 0) == 22.0),
        ("vertex x", |m| m.roots[1].branches[0].vertices[1].x = -6.5, |g| num(&root(g, 1), 10, 2) == -6.5),
        ("vertex y", |m| m.roots[1].branches[0].vertices[1].y = 5.5, |g| num(&root(g, 1), 20, 2) == 5.5),
        ("vertex z", |m| m.roots[1].branches[0].vertices[1].z = 0.5, |g| num(&root(g, 1), 30, 2) == 0.5),
        ("context origin x", |m| m.context.origin.x = 1.5, |g| num(&context(g), 110, 0) == 1.5),
        ("context origin y", |m| m.context.origin.y = 2.5, |g| num(&context(g), 120, 0) == 2.5),
        ("context origin z", |m| m.context.origin.z = 3.5, |g| num(&context(g), 130, 0) == 3.5),
        ("context x axis", |m| m.context.x_axis = Vec3::new(0.0, 1.0, 0.0), |g| num(&context(g), 121, 0) == 1.0 && num(&context(g), 111, 0) == 0.0),
        ("context y axis", |m| m.context.y_axis = Vec3::new(-1.0, 0.0, 0.0), |g| num(&context(g), 112, 0) == -1.0 && num(&context(g), 122, 0) == 0.0),
        ("text plane", |m| m.context.text_normal = Vec3::new(0.0, 0.6, 0.8), |g| num(&context(g), 21, 0) == 0.6 && num(&context(g), 31, 0) == 0.8),
        ("text direction z", |m| m.context.text_dir = Vec3::new(1.0, 0.0, 0.5), |g| num(&context(g), 33, 0) == 0.5),
    ];
    for (name, mutate, check) in cases {
        let mut d = drawing();
        edit(&mut d, mutate);
        let g = groups(&save(&d));
        assert!(check(&g), "{name}: {g:?}");
        // The sibling root and the lines that were not touched keep their source groups.
        if name.starts_with("landing")
            || name.starts_with("dogleg")
            || name.starts_with("vertex")
            || name.starts_with("branch")
            || name == "root number"
        {
            assert_eq!(root(&g, 0), root(&groups(&source()), 0), "{name}: root 0 stays");
        }
        let _ = r1;
        // The DWG gets the same change (numbers of roots and branches and the context frame are the bridge's own).
        if name.starts_with("landing") || name.starts_with("dogleg length") || name.starts_with("vertex") {
            let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
            let want = mleader(&d);
            let got = mleader(&back);
            assert_eq!(got.roots[1].landing, want.roots[1].landing, "{name}: DWG landing");
            assert_eq!(got.roots[1].dogleg, want.roots[1].dogleg, "{name}: DWG dogleg");
            assert_eq!(got.roots[1].branches[0].vertices, want.roots[1].branches[0].vertices, "{name}: DWG vertices");
        }
    }
}

#[test]
fn the_text_direction_follows_a_rotation_of_the_text() {
    // The source's direction keeps its Z; a changed rotation writes the direction it gives.
    let mut d = drawing();
    edit(&mut d, |m| {
        if let Some(t) = &mut m.text {
            t.rotation = std::f64::consts::FRAC_PI_2;
        }
    });
    let g = context(&groups(&save(&d)));
    assert!(num(&g, 13, 0).abs() < 1e-12 && (num(&g, 23, 0) - 1.0).abs() < 1e-12, "{g:?}");
}

#[test]
fn a_payload_saved_before_roots_loads_as_one_root() {
    let json = r#"{"leaders":[[{"x":0.0,"y":0.0,"z":0.0},{"x":2.0,"y":3.0,"z":0.0}],[{"x":1.0,"y":-2.0,"z":0.0}]],
        "landing":{"x":5.0,"y":4.0,"z":0.0},"dogleg":0.75,"text":null,"style":"Standard","arrowSize":0.18,"spline":false}"#;
    let m: MLeader = serde_json::from_str(json).unwrap();
    assert_eq!(m.roots.len(), 1);
    assert_eq!((m.roots[0].landing, m.roots[0].dogleg), (Vec3::new(5.0, 4.0, 0.0), 0.75));
    assert_eq!(m.roots[0].branches.len(), 2);
    assert_eq!(m.roots[0].branches[1].vertices, [Vec3::new(1.0, -2.0, 0.0)]);
    // The new layout round-trips.
    let back: MLeader = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(back, m);
}

#[test]
fn a_whole_entity_transform_turns_every_root() {
    let mut d = drawing();
    let h = handle(&d);
    let quarter = cadcraft_geom::Mat4::from_mat3(&cadcraft_geom::Mat3::rotate(std::f64::consts::FRAC_PI_2));
    d.try_modify_entity(h, |e| e.try_transform(&quarter)).unwrap();
    let m = mleader(&d);
    let near = |a: Vec3, b: Vec3| (a - b).len() < 1e-9;
    assert!(near(m.roots[0].landing, Vec3::new(-20.0, 10.0, 0.0)) && near(m.roots[0].dogleg_dir, Vec3::new(0.0, 1.0, 0.0)));
    assert!(near(m.roots[1].landing, Vec3::new(-18.0, -10.0, 0.0)) && near(m.roots[1].dogleg_dir, Vec3::new(0.0, -1.0, 0.0)));
    assert_eq!((m.roots[0].dogleg, m.roots[1].dogleg), (2.0, 3.0));
    assert_eq!((m.roots[0].branches[0].id, m.roots[1].id), (11, 9));
}

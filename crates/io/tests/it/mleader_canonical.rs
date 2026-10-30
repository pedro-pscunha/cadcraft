//! A canonical MULTILEADER (entity and context `CanonicalV2`) is written with every WCS point and direction it holds: the
//! roots with their landings, doglegs and lines, the context's frame, the text with its full direction and the block with its
//! complete WCS matrix. Each field changed alone reaches the user DXF and the DWG; the canonical reader returns what the
//! writer wrote; a motion of the entity moves every root; editing one line leaves the sibling root's groups as they were.

use acadrust::DwgReader;
use acadrust::entities::EntityType;
use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, GeometryRevision, Handle, Insert, Line, MLeader, MLeaderContext, MLeaderRoot, MText};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_geom::{Mat4, OcsFrame, Vec3};
use cadcraft_io::{SaveOptions, write_with};

const TEXT_NORMAL: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 2.0 };
const BLOCK_NORMAL: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 2.0 };

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(12.0, 20.0, 3.0),
        height: 0.4,
        width: 10.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Two roots".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(0.6, 0.8, 0.0)),
    }
}

/// Two roots, a text in the XY plane turned by its full direction, and a block content in a tilted plane.
fn leader(block: bool) -> MLeader {
    let mut m =
        MLeader::single(Vec3::new(10.0, 20.0, 3.0), 2.0, false, vec![vec![Vec3::new(0.0, 0.0, 3.0), Vec3::new(4.0, 6.0, 3.5)]], (!block).then(mtext));
    m.roots[0].id = 7;
    m.roots[0].branches[0].id = 11;
    let mut second = MLeaderRoot::new(Vec3::new(-10.0, 18.0, 3.0), 3.0, true, vec![vec![Vec3::new(-2.0, 0.0, 3.0), Vec3::new(-6.0, 5.0, 3.0)]]);
    second.id = 9;
    second.branches[0].id = 21;
    m.roots.push(second);
    m.arrow_size = 0.18;
    // A record holds the text's frame only with a text and the block's normal only with a block.
    m.context = MLeaderContext {
        origin: Vec3::new(1.0, 2.0, 3.0),
        x_axis: Vec3::new(0.0, 1.0, 0.0),
        y_axis: Vec3::new(0.0, 0.0, 1.0),
        text_normal: if block { Vec3::Z } else { TEXT_NORMAL },
        text_dir: if block { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.6, 0.8, 0.0) },
        block_normal: if block { BLOCK_NORMAL } else { Vec3::Z },
        geometry_revision: GeometryRevision::CanonicalV2,
    };
    if block {
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::new(13.0, 20.0, 4.0),
            scale: Vec3::new(1.5, 2.0, 0.5),
            rotation: 0.4,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
    }
    m
}

fn drawing(m: &MLeader) -> Drawing {
    let mut d = Drawing::new_metric();
    let mut block = Block::new("B");
    block.base = Vec3::new(1.0, 2.0, 3.0);
    block.entities.push(Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })));
    d.blocks.insert("B".into(), std::sync::Arc::new(block));
    let handle = d.new_handle();
    let mut e = Entity::new(handle, EntityKind::MLeader(m.clone()));
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common::default();
    d.model.push(e);
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap()
}

fn record(text: &str) -> Vec<Tag> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags).into_iter().find(|(k, _)| k == "MULTILEADER").expect("a MULTILEADER").1
}

/// The groups of the record after its `AcDbMLeader` marker.
fn groups(text: &str) -> Vec<(i32, String)> {
    let tags = record(text);
    let start = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbMLeader").unwrap();
    tags[start + 1..].iter().map(|t| (t.code, t.str())).collect()
}

fn root(g: &[(i32, String)], n: usize) -> Vec<(i32, String)> {
    let starts: Vec<usize> = (0..g.len()).filter(|i| g[*i].0 == 302).collect();
    let from = starts[n];
    let to = (from..g.len()).find(|i| g[*i].0 == 303).unwrap();
    g[from..=to].to_vec()
}

fn context(g: &[(i32, String)]) -> Vec<(i32, String)> {
    g[..g.iter().position(|t| t.0 == 302).unwrap()].to_vec()
}

fn num(g: &[(i32, String)], code: i32, nth: usize) -> f64 {
    g.iter().filter(|t| t.0 == code).nth(nth).unwrap_or_else(|| panic!("no group {code} #{nth} in {g:?}")).1.trim().parse().unwrap()
}

fn point(g: &[(i32, String)], code: i32, nth: usize) -> Vec3 {
    Vec3::new(num(g, code, nth), num(g, code + 10, nth), num(g, code + 20, nth))
}

fn read_back(text: &str) -> MLeader {
    let (_, kind) = cadcraft_io::canonical_mleader_from_groups(&record(text)).expect("a canonical multileader");
    let EntityKind::MLeader(m) = kind else { panic!("not a multileader") };
    m
}

fn dwg_leader(d: &Drawing) -> acadrust::entities::MultiLeader {
    let stored = DwgReader::from_stream(std::io::Cursor::new(cadcraft_io::write(d, "x.dwg").unwrap())).read().unwrap();
    stored.entities().find_map(|e| if let EntityType::MultiLeader(m) = e { Some((**m).clone()) } else { None }).expect("a MULTILEADER in the DWG")
}

type Mutation = fn(&mut MLeader);

/// Which content a mutation applies to.
enum Only {
    Both,
    Text,
    Block,
}

/// Each field of the model changed alone, with the content it applies to.
fn mutations(block: bool) -> Vec<(&'static str, Mutation)> {
    let all: Vec<(&'static str, Only, Mutation)> = vec![
        ("root 0 landing x", Only::Both, |m| m.roots[0].landing.x = 10.5),
        ("root 0 landing z", Only::Both, |m| m.roots[0].landing.z = 3.25),
        ("root 0 dogleg direction y", Only::Both, |m| m.roots[0].dogleg_dir = Vec3::new(0.6, 0.8, 0.0)),
        ("root 0 dogleg direction z", Only::Both, |m| m.roots[0].dogleg_dir = Vec3::new(0.6, 0.0, 0.8)),
        ("root 0 dogleg length", Only::Both, |m| m.roots[0].dogleg = 2.5),
        ("root 0 vertex z", Only::Both, |m| m.roots[0].branches[0].vertices[1].z = 3.75),
        ("root 1 landing z", Only::Both, |m| m.roots[1].landing.z = 3.5),
        ("root 1 vertex y", Only::Both, |m| m.roots[1].branches[0].vertices[1].y = 5.5),
        ("root 1 dogleg length", Only::Both, |m| m.roots[1].dogleg = 3.5),
        ("arrow size", Only::Both, |m| m.arrow_size = 0.25),
        ("context origin z", Only::Both, |m| m.context.origin.z = 4.0),
        ("context x axis", Only::Both, |m| m.context.x_axis = Vec3::new(0.0, 0.0, 1.0)),
        ("context y axis", Only::Both, |m| m.context.y_axis = Vec3::new(1.0, 0.0, 0.0)),
        ("text normal", Only::Text, |m| m.context.text_normal = Vec3::new(0.0, 0.0, 3.0)),
        ("text location z", Only::Text, |m| m.text.as_mut().unwrap().insert.z = 3.5),
        ("text direction", Only::Text, |m| {
            let t = m.text.as_mut().unwrap();
            t.direction = Some(Vec3::new(0.8, 0.6, 0.0));
            m.context.text_dir = Vec3::new(0.8, 0.6, 0.0);
        }),
        ("text height", Only::Text, |m| m.text.as_mut().unwrap().height = 0.5),
        ("block location z", Only::Block, |m| m.block.as_mut().unwrap().insert.z = 4.5),
        ("block normal", Only::Block, |m| m.context.block_normal = Vec3::new(0.0, 1.0, 2.0)),
        ("block scale z", Only::Block, |m| m.block.as_mut().unwrap().scale.z = 0.75),
        ("block scale y sign", Only::Block, |m| m.block.as_mut().unwrap().scale.y = -2.0),
        ("block rotation", Only::Block, |m| m.block.as_mut().unwrap().rotation = 0.5),
    ];
    all.into_iter()
        .filter(|(_, only, _)| matches!((only, block), (Only::Both, _) | (Only::Text, false) | (Only::Block, true)))
        .map(|(n, _, f)| (n, f))
        .collect()
}

#[test]
fn the_text_content_reaches_the_dxf_in_wcs() {
    let m = leader(false);
    let g = groups(&dxf(&drawing(&m)));
    let c = context(&g);
    assert!(near(point(&c, 11, 0), TEXT_NORMAL), "text normal");
    assert!(near(point(&c, 12, 0), Vec3::new(12.0, 20.0, 3.0)), "text location");
    assert!(near(point(&c, 13, 0), Vec3::new(0.6, 0.8, 0.0)), "text direction");
    assert!(near(point(&c, 110, 0), m.context.origin) && near(point(&c, 111, 0), m.context.x_axis) && near(point(&c, 112, 0), m.context.y_axis));
    // Root 7 and root 9, in file order, with their own landing, direction, length and line numbers.
    let first = root(&g, 0);
    assert_eq!((num(&first, 90, 0), num(&first, 91, 0), num(&first, 40, 0)), (7.0, 11.0, 2.0));
    assert!(near(point(&first, 10, 0), Vec3::new(10.0, 20.0, 3.0)) && near(point(&first, 11, 0), Vec3::new(1.0, 0.0, 0.0)));
    assert!(near(point(&first, 10, 2), Vec3::new(4.0, 6.0, 3.5)), "the second vertex keeps its Z");
    let second = root(&g, 1);
    assert_eq!((num(&second, 90, 0), num(&second, 91, 0), num(&second, 40, 0)), (9.0, 21.0, 3.0));
    assert!(near(point(&second, 10, 0), Vec3::new(-10.0, 18.0, 3.0)) && near(point(&second, 11, 0), Vec3::new(-1.0, 0.0, 0.0)));
}

#[test]
fn the_block_content_reaches_the_dxf_with_its_complete_wcs_matrix() {
    let m = leader(true);
    let d = drawing(&m);
    let g = groups(&dxf(&d));
    let c = context(&g);
    assert!(near(point(&c, 14, 0), BLOCK_NORMAL) && near(point(&c, 15, 0), Vec3::new(13.0, 20.0, 4.0)), "block normal and location");
    assert!(near(point(&c, 16, 0), Vec3::new(1.5, 2.0, 0.5)) && (num(&c, 46, 0) - 0.4).abs() < 1e-12, "block scale and rotation");
    // The matrix: the arbitrary-axis frame of the block normal, the scale, the rotation and the definition's base point, built
    // here from the frame's own axes (independent of the document's matrix routine).
    let frame = OcsFrame::new(BLOCK_NORMAL).unwrap();
    let (s, co) = 0.4f64.sin_cos();
    let x = frame.to_world(Vec3::new(co, s, 0.0)) * 1.5;
    let y = frame.to_world(Vec3::new(-s, co, 0.0)) * 2.0;
    let z = frame.to_world(Vec3::Z) * 0.5;
    let origin = Vec3::new(13.0, 20.0, 4.0) - x * 1.0 - y * 2.0 - z * 3.0;
    let want = [x.x, y.x, z.x, origin.x, x.y, y.y, z.y, origin.y, x.z, y.z, z.z, origin.z, 0.0, 0.0, 0.0, 1.0];
    let got: Vec<f64> = c.iter().filter(|t| t.0 == 47).map(|t| t.1.trim().parse().unwrap()).collect();
    assert_eq!(got.len(), 16);
    for (i, (a, b)) in got.iter().zip(want).enumerate() {
        assert!((a - b).abs() < 1e-9, "matrix element {i}: {a} vs {b}");
    }
}

#[test]
fn each_field_alone_reaches_the_dxf_and_the_reader_returns_it() {
    for block in [false, true] {
        let base = groups(&dxf(&drawing(&leader(block))));
        for (name, mutate) in mutations(block) {
            let mut m = leader(block);
            mutate(&mut m);
            let out = dxf(&drawing(&m));
            assert_ne!(groups(&out), base, "{name}: the field reaches the DXF");
            let back = read_back(&out);
            // The names come from the file's objects; the geometry is the model's.
            assert_eq!(back.roots, m.roots, "{name}: roots");
            assert_eq!(back.context, m.context, "{name}: context");
            assert_eq!(back.arrow_size, m.arrow_size, "{name}: arrow size");
            let (a, b) = (back.text.as_ref(), m.text.as_ref());
            assert_eq!(
                a.map(|t| (t.insert, t.height, t.width, t.direction, t.attach)),
                b.map(|t| (t.insert, t.height, t.width, t.direction, t.attach)),
                "{name}: text"
            );
            if let Some(t) = a {
                let in_plane = OcsFrame::new(m.context.text_normal).unwrap().to_ocs(t.direction.unwrap());
                assert!((t.rotation - in_plane.y.atan2(in_plane.x)).abs() < 1e-12, "{name}: the rotation is the direction's angle in the text plane");
            }
            assert_eq!(back.block.map(|i| (i.insert, i.scale, i.rotation)), m.block.map(|i| (i.insert, i.scale, i.rotation)), "{name}: block");
        }
    }
}

#[test]
fn each_field_alone_reaches_the_dwg() {
    for block in [false, true] {
        for (name, mutate) in mutations(block) {
            let mut m = leader(block);
            mutate(&mut m);
            let s = dwg_leader(&drawing(&m));
            let c = &s.context;
            let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
            assert_eq!(s.context.leader_roots.len(), 2, "{name}");
            for (i, root) in m.roots.iter().enumerate() {
                let r = &c.leader_roots[i];
                assert!(near(v(r.connection_point), root.landing), "{name}: DWG root {i} landing {:?}", r.connection_point);
                assert!(near(v(r.direction), root.dogleg_dir), "{name}: DWG root {i} direction {:?}", r.direction);
                assert!((r.landing_distance - root.dogleg).abs() < 1e-12, "{name}: DWG root {i} dogleg {}", r.landing_distance);
                let points: Vec<Vec3> = r.lines.iter().flat_map(|l| l.points.iter().map(|p| v(*p))).collect();
                let want: Vec<Vec3> = root.branches.iter().flat_map(|b| b.vertices.iter().copied()).collect();
                assert_eq!(points.len(), want.len(), "{name}");
                assert!(points.iter().zip(&want).all(|(a, b)| near(*a, *b)), "{name}: DWG root {i} vertices {points:?}");
            }
            if let Some(t) = &m.text {
                assert!(
                    near(v(c.text_location), t.insert) && near(v(c.text_normal), m.context.text_normal),
                    "{name}: DWG text {:?}",
                    c.text_location
                );
                assert!(near(v(c.text_direction), t.direction.unwrap()), "{name}: DWG text direction {:?}", c.text_direction);
            }
            if let Some(b) = &m.block {
                assert!(near(v(c.block_content_location), b.insert) && near(v(c.block_content_normal), m.context.block_normal), "{name}: DWG block");
                assert!(
                    near(v(c.block_content_scale), b.scale) && (c.block_rotation - b.rotation).abs() < 1e-12,
                    "{name}: DWG block scale {:?}",
                    c.block_content_scale
                );
            }
        }
    }
}

#[test]
fn a_motion_of_the_entity_moves_every_root_and_the_second_save_is_the_same() {
    let mut d = drawing(&leader(false));
    let handle = d.model.iter().next().unwrap().handle;
    let (s, c) = 0.6f64.sin_cos();
    let turn = Mat4 { m: [[1.0, 0.0, 0.0, 5.0], [0.0, c, -s, -2.0], [0.0, s, c, 1.0], [0.0, 0.0, 0.0, 1.0]] };
    d.transform_entity(handle, &turn).unwrap();
    let first = dxf(&d);
    let g = groups(&first);
    for (n, (landing, vertex)) in
        [(Vec3::new(10.0, 20.0, 3.0), Vec3::new(4.0, 6.0, 3.5)), (Vec3::new(-10.0, 18.0, 3.0), Vec3::new(-6.0, 5.0, 3.0))].into_iter().enumerate()
    {
        let r = root(&g, n);
        assert!(near(point(&r, 10, 0), turn.apply(landing)), "root {n} landing");
        assert!(near(point(&r, 10, 2), turn.apply(vertex)), "root {n} vertex");
    }
    let c = context(&g);
    assert!(near(point(&c, 11, 0), turn.apply_vec(TEXT_NORMAL)), "text normal");
    assert!(near(point(&c, 13, 0), turn.apply_vec(Vec3::new(0.6, 0.8, 0.0))), "text direction");
    // The second save of what the canonical reader read is the same record.
    let again = drawing(&read_back(&first));
    assert_eq!(groups(&dxf(&again)), g, "the second save");
}

#[test]
fn editing_one_line_keeps_the_sibling_root_in_every_output() {
    let want = root(&groups(&dxf(&drawing(&leader(false)))), 1);
    let mut m = leader(false);
    m.roots[0].branches[0].vertices[1] = Vec3::new(5.0, 7.0, 4.0);
    let d = drawing(&m);
    let first = dxf(&d);
    let g = groups(&first);
    assert_eq!(root(&g, 1), want, "the sibling root of the DXF");
    assert!(near(point(&root(&g, 0), 10, 2), Vec3::new(5.0, 7.0, 4.0)));
    let read = read_back(&first);
    assert_eq!(groups(&dxf(&drawing(&read))), g, "the second save");
    // The DWG keeps both roots with their own landing, dogleg and vertices.
    let s = dwg_leader(&d);
    let roots = &s.context.leader_roots;
    assert_eq!(roots.len(), 2);
    let v = |p: acadrust::types::Vector3| Vec3::new(p.x, p.y, p.z);
    assert!(near(v(roots[1].connection_point), Vec3::new(-10.0, 18.0, 3.0)) && (roots[1].landing_distance - 3.0).abs() < 1e-12);
    assert!(near(v(roots[0].lines[0].points[1]), Vec3::new(5.0, 7.0, 4.0)));
    assert!(near(v(roots[1].lines[0].points[1]), Vec3::new(-6.0, 5.0, 3.0)));
}

#[test]
fn the_canonical_reader_refuses_a_text_without_a_plane_or_a_direction() {
    let out = dxf(&drawing(&leader(false)));
    let mut tags = record(&out);
    // Zero the text normal (the first group 31 after the first group 11 of the context).
    let n = tags.iter().position(|t| t.code == 31).unwrap();
    tags[n] = Tag::f(31, 0.0);
    let n = tags.iter().position(|t| t.code == 21).unwrap();
    tags[n] = Tag::f(21, 0.0);
    let n = tags.iter().position(|t| t.code == 11).unwrap();
    tags[n] = Tag::f(11, 0.0);
    assert!(cadcraft_io::canonical_mleader_from_groups(&tags).is_none());
}

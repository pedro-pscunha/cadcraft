//! A canonical MULTILEADER saved for a release before R2007 (no MULTILEADER entity) becomes LEADER, MTEXT and INSERT
//! parts that each have their own plane (group 210), 3D vertices and the MTEXT's full direction. Read back with the canonical
//! readers, the parts have the world geometry of the multileader.

use cadcraft_doc::{
    Attrib, Block, Common, Drawing, Entity, EntityKind, GeometryRevision, HAlign, Handle, Insert, LeaderArrowLayout, Line, MLeader, MLeaderContext,
    MLeaderRoot, MText, Text, VAlign, WorldPrimitive, attrib_primitives, leader_primitives, mleader_primitives, mtext_primitives,
};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_geom::{Mat4, Vec3};
use cadcraft_io::{SaveOptions, write_with, write_with_noted};

/// The plane of every point: X along the world X axis, Y up (0, 0.6, 0.8); its normal is X cross Y.
const NORMAL: Vec3 = Vec3 { x: 0.0, y: -0.8, z: 0.6 };
const BASE: Vec3 = Vec3 { x: 1.0, y: 2.0, z: 3.0 };

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
}

fn in_plane(a: f64, b: f64) -> Vec3 {
    Vec3::new(a, 0.6 * b, 0.8 * b)
}

fn text() -> Text {
    Text {
        insert: Vec3::new(0.5, 1.0, 0.0),
        align_pt: None,
        height: 0.25,
        value: "TAG".into(),
        rotation: 0.2,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        backward: false,
        upside_down: false,
    }
}

fn leader(block: Option<Vec3>) -> MLeader {
    let mtext = MText {
        insert: in_plane(12.0, 6.0),
        height: 0.4,
        width: 10.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "Tilted".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(1.0, 0.0, 0.0)),
        drawing_direction: None,
    };
    let mut m = MLeader::single(
        in_plane(10.0, 5.0),
        2.0,
        false,
        vec![vec![in_plane(0.0, 0.0), in_plane(4.0, 2.5)], vec![in_plane(1.0, 6.0)]],
        block.is_none().then_some(mtext),
    );
    m.roots.push(MLeaderRoot::new(in_plane(-10.0, 3.0), 3.0, true, vec![vec![in_plane(-2.0, 0.0)]]));
    m.roots[1].id = 1;
    m.context = MLeaderContext {
        origin: in_plane(0.0, 0.0),
        x_axis: Vec3::new(1.0, 0.0, 0.0),
        y_axis: Vec3::new(0.0, 0.6, 0.8),
        text_normal: NORMAL,
        text_dir: Vec3::new(1.0, 0.0, 0.0),
        block_normal: NORMAL,
        geometry_revision: GeometryRevision::CanonicalV2,
        codes: Default::default(),
    };
    if let Some(scale) = block {
        let mut attrib = Attrib::new("TAG", text());
        attrib.geometry_revision = GeometryRevision::CanonicalV2;
        m.block = Some(Insert {
            block: "B".into(),
            insert: in_plane(13.0, 6.0),
            scale,
            rotation: 0.4,
            attribs: vec![attrib],
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
    block.base = BASE;
    block.entities.push(Entity::new(Handle(0x7000), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })));
    d.blocks.insert("B".into(), std::sync::Arc::new(block));
    let handle = d.new_handle();
    let mut e = Entity::new(handle, EntityKind::MLeader(m.clone()));
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common::default();
    d.model.push(e);
    d
}

fn entities(d: &Drawing) -> Vec<(String, Vec<Tag>)> {
    let out = String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2000), binary: false }).unwrap()).unwrap();
    let tags = cadcraft_dxf::parse(out.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags)
}

fn same(a: &WorldPrimitive, b: &WorldPrimitive) -> bool {
    match (a, b) {
        (WorldPrimitive::Line { a: a0, b: b0 }, WorldPrimitive::Line { a: a1, b: b1 }) => near(*a0, *a1) && near(*b0, *b1),
        (WorldPrimitive::Text(t0), WorldPrimitive::Text(t1)) => {
            // The same placed text: the frame axes of a text under a block matrix carry its scale, a placed attribute has it in
            // its height, so compare the directions and the size in the world.
            let size = |t: &cadcraft_doc::WorldText| {
                let height = match &t.content {
                    cadcraft_doc::WorldTextContent::Text(x) => x.height,
                    cadcraft_doc::WorldTextContent::MText(x) => x.height,
                };
                height * t.frame.y_axis.len()
            };
            near(t0.frame.origin, t1.frame.origin)
                && near(t0.frame.x_axis.normalized(), t1.frame.x_axis.normalized())
                && near(t0.frame.y_axis.normalized(), t1.frame.y_axis.normalized())
                && near(t0.frame.normal, t1.frame.normal)
                && (size(t0) - size(t1)).abs() < 1e-9
        }
        _ => false,
    }
}

fn number(tags: &[Tag], code: i32) -> f64 {
    tags.iter().find(|t| t.code == code).unwrap_or_else(|| panic!("no group {code}")).str().trim().parse().unwrap()
}

fn point(tags: &[Tag], code: i32) -> Vec3 {
    Vec3::new(number(tags, code), number(tags, code + 10), number(tags, code + 20))
}

/// The world primitives and the block matrix of the parts a pre-2007 file holds.
fn world_of_parts(d: &Drawing) -> (Vec<WorldPrimitive>, Option<Mat4>) {
    let arrow = LeaderArrowLayout::default();
    let id = Mat4::IDENTITY;
    let mut out = Vec::new();
    let mut matrix = None;
    for (kind, tags) in entities(d) {
        match kind.as_str() {
            "LEADER" => {
                let (c, EntityKind::Leader(l)) = cadcraft_io::canonical_curve_from_groups("LEADER", &tags).expect("a canonical LEADER") else {
                    panic!("not a leader")
                };
                out.extend(leader_primitives(&l, c.extrusion, &arrow, &id).unwrap());
            }
            "MTEXT" => {
                let (c, EntityKind::MText(t)) = cadcraft_io::canonical_text_from_groups("MTEXT", &tags).expect("a canonical MTEXT") else {
                    panic!("not an mtext")
                };
                out.extend(mtext_primitives(&t, c.extrusion, &id).unwrap());
            }
            "INSERT" => {
                let insert = Insert {
                    block: "B".into(),
                    insert: point(&tags, 10),
                    scale: Vec3::new(number(&tags, 41), number(&tags, 42), number(&tags, 43)),
                    rotation: number(&tags, 50).to_radians(),
                    attribs: Vec::new(),
                    cols: 1,
                    rows: 1,
                    col_spacing: 0.0,
                    row_spacing: 0.0,
                };
                matrix = Some(insert.world_transform(point(&tags, 210), BASE, 0, 0).unwrap());
            }
            "ATTRIB" => {
                let a = cadcraft_io::canonical_attrib_from_groups(&tags, None).expect("a canonical ATTRIB");
                out.extend(attrib_primitives(&a, &id).unwrap());
            }
            _ => {}
        }
    }
    (out, matrix)
}

fn assert_world(m: &MLeader, with_attribs: bool) {
    let d = drawing(m);
    let want = mleader_primitives(m, BASE, &LeaderArrowLayout::default(), &Mat4::IDENTITY).unwrap();
    let (got, matrix) = world_of_parts(&d);
    // The leader lines, the doglegs and the text come in the multileader's own order; the attribute texts come last.
    let mut expected: Vec<&WorldPrimitive> = want.primitives.iter().collect();
    if !with_attribs {
        expected.retain(
            |p| !matches!(p, WorldPrimitive::Text(t) if t.frame.origin != m.text.as_ref().map_or(Vec3::ZERO, |x| x.insert) && m.text.is_some()),
        );
        if m.text.is_none() {
            expected.retain(|p| !matches!(p, WorldPrimitive::Text(_)));
        }
    }
    assert_eq!(got.len(), expected.len(), "{got:?}");
    for (i, (g, w)) in got.iter().zip(&expected).enumerate() {
        assert!(same(g, w), "primitive {i}: {g:?} vs {w:?}");
    }
    match (matrix, want.block) {
        (Some(g), Some(w)) => {
            for (row_g, row_w) in g.m.iter().zip(&w.m) {
                assert!(row_g.iter().zip(row_w).all(|(a, b)| (a - b).abs() < 1e-9), "{g:?} vs {w:?}");
            }
        }
        (None, None) => {}
        (g, w) => panic!("block matrix {g:?} vs {w:?}"),
    }
}

#[test]
fn a_tilted_text_multileader_becomes_leaders_in_its_plane_and_an_mtext_with_its_direction() {
    let m = leader(None);
    let parts = entities(&drawing(&m));
    assert!(parts.iter().all(|(k, _)| k != "MULTILEADER"));
    let leaders: Vec<&Vec<Tag>> = parts.iter().filter(|(k, _)| k == "LEADER").map(|(_, t)| t).collect();
    // Three lines (two on root 0) and two doglegs.
    assert_eq!(leaders.len(), 5);
    for l in &leaders {
        assert!(near(point(l, 210), NORMAL), "the plane of the leader: {:?}", point(l, 210));
        assert!(near(point(l, 211), Vec3::new(1.0, 0.0, 0.0)));
    }
    let (_, mtext) = parts.iter().find(|(k, _)| k == "MTEXT").unwrap();
    assert!(near(point(mtext, 210), NORMAL) && near(point(mtext, 11), Vec3::new(1.0, 0.0, 0.0)) && near(point(mtext, 10), in_plane(12.0, 6.0)));
    assert_world(&m, false);
}

#[test]
fn a_tilted_block_multileader_becomes_an_insert_in_its_plane_with_its_attributes_placed() {
    let m = leader(Some(Vec3::new(2.0, 2.0, 2.0)));
    let parts = entities(&drawing(&m));
    let (_, insert) = parts.iter().find(|(k, _)| k == "INSERT").unwrap();
    assert!(near(point(insert, 210), NORMAL));
    // The insertion point is an OCS value of the block normal: its WCS point is the multileader's.
    let ocs = cadcraft_geom::OcsFrame::new(NORMAL).unwrap();
    assert!(near(ocs.to_world(point(insert, 10)), in_plane(13.0, 6.0)), "{:?}", point(insert, 10));
    assert!((number(insert, 50) - 0.4f64.to_degrees()).abs() < 1e-9);
    assert_eq!(parts.iter().filter(|(k, _)| k == "ATTRIB").count(), 1);
    assert_world(&m, true);
}

#[test]
fn a_non_uniform_block_scale_leaves_the_attribute_out_and_keeps_the_block() {
    let m = leader(Some(Vec3::new(1.5, 2.0, 0.5)));
    let parts = entities(&drawing(&m));
    assert!(parts.iter().any(|(k, _)| k == "INSERT"));
    assert_eq!(parts.iter().filter(|(k, _)| k == "ATTRIB").count(), 0, "a text has no non-uniform form");
    assert_world(&m, false);
}

#[test]
fn a_legacy_multileader_splits_as_before() {
    let mut m = leader(None);
    m.context.geometry_revision = GeometryRevision::LegacyProjectedV1;
    let mut d = Drawing::new_metric();
    let h = d.new_handle();
    d.model.push(Entity::new(h, EntityKind::MLeader(m)));
    let parts = entities(&d);
    assert_eq!(parts.iter().filter(|(k, _)| k == "LEADER").count(), 3, "no dogleg part for a legacy payload");
    assert!(parts.iter().all(|(_, t)| t.iter().all(|g| g.code != 210)), "a legacy part has no plane");
}

#[test]
fn the_attributes_left_out_by_a_non_uniform_scale_are_a_note_of_the_save() {
    let m = leader(Some(Vec3::new(1.5, 2.0, 0.5)));
    let d = drawing(&m);
    let handle = d.model.iter().next().unwrap().handle;
    let note = format!("MULTILEADER {}: 1 block attribute(s) left out", handle.hex());
    // A DXF and a DWG before R2007 report it; a uniform scale and a file from R2007 on do not.
    for name in ["x.dxf", "x.dwg"] {
        let (_, notes) = write_with_noted(&d, name, &SaveOptions { version: Some(Version::R2004), binary: false }).unwrap();
        assert!(notes.iter().any(|n| n.starts_with(&note)), "{name}: {notes:?}");
        let (_, notes) = write_with_noted(&d, name, &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
        assert!(notes.iter().all(|n| !n.contains("MULTILEADER")), "{name}: {notes:?}");
    }
    let (_, notes) =
        write_with_noted(&drawing(&leader(Some(Vec3::new(2.0, 2.0, 2.0)))), "x.dxf", &SaveOptions { version: Some(Version::R2004), binary: false })
            .unwrap();
    // The drawing has no saved extents, so the save may note the extents it omits; it adds no multileader note.
    assert!(notes.iter().all(|n| !n.contains("MULTILEADER")), "{notes:?}");
}

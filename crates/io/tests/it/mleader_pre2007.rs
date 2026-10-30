//! Old targets retain native multileaders, including their WCS geometry and block attributes.

use cadcraft_doc::{
    Attrib, Block, Common, Drawing, Entity, EntityKind, GeometryRevision, HAlign, Handle, Insert, LeaderArrowLayout, Line, MLeader, MLeaderContext,
    MLeaderRoot, MText, Text, VAlign, WorldPrimitive, mleader_primitives,
};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_geom::{Mat4, Vec3};
use cadcraft_io::{SaveOptions, write_with_noted};

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
    if let Some(attrib) = m.block.as_ref().and_then(|b| b.attribs.first()) {
        let mut definition = Entity::new(Handle(0x7001), EntityKind::AttDef(attrib.clone()));
        definition.geometry_revision = GeometryRevision::CanonicalV2;
        block.entities.push(definition);
    }
    d.blocks.insert("B".into(), std::sync::Arc::new(block));
    let handle = d.new_handle();
    let mut e = Entity::new(handle, EntityKind::MLeader(m.clone()));
    e.geometry_revision = GeometryRevision::CanonicalV2;
    e.common = Common::default();
    d.model.push(e);
    d.bump_handseed(Handle(0x7002));
    d
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

fn native(d: &Drawing, version: Version) -> (Vec<Tag>, Drawing, Vec<String>) {
    let (bytes, notes) = write_with_noted(d, "old.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let records = records(&section.tags);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0, "MULTILEADER");
    (records[0].1.clone(), cadcraft_io::read_dxf(&bytes).unwrap(), notes)
}
#[test]
fn tilted_native_text_retains_its_world_primitives_before_2007() {
    let original = leader(None);
    let d = drawing(&original);
    for version in [Version::R2000, Version::R2004] {
        let (tags, _, _) = native(&d, version);
        let (_, EntityKind::MLeader(parsed)) = cadcraft_io::canonical_mleader_from_groups(&tags).unwrap() else { panic!("multileader") };
        let arrow = LeaderArrowLayout::default();
        let matrix = Mat4::IDENTITY;
        let want = mleader_primitives(&original, BASE, &arrow, &matrix).unwrap();
        let got = mleader_primitives(&parsed, BASE, &arrow, &matrix).unwrap();
        assert_eq!(got.primitives.len(), want.primitives.len());
        for (actual, expected) in got.primitives.iter().zip(&want.primitives) {
            assert!(same(actual, expected), "{actual:?} vs {expected:?}");
        }
    }
}
#[test]
fn native_old_target_block_content_keeps_non_uniform_scale_and_all_attributes() {
    for scale in [Vec3::new(2.0, 2.0, 2.0), Vec3::new(1.5, 2.0, 0.5)] {
        let original = leader(Some(scale));
        let d = drawing(&original);
        for version in [Version::R2000, Version::R2004] {
            let (tags, back, notes) = native(&d, version);
            let (_, EntityKind::MLeader(parsed)) = cadcraft_io::canonical_mleader_from_groups(&tags).unwrap() else { panic!("multileader") };
            let actual = parsed.block_world_transform(BASE).unwrap().unwrap();
            let expected = original.block_world_transform(BASE).unwrap().unwrap();
            for (a, b) in actual.m.iter().zip(&expected.m) {
                assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-9));
            }
            let EntityKind::MLeader(saved) = &back.model.iter().next().unwrap().kind else { panic!("multileader") };
            let attributes = &saved.block.as_ref().unwrap().attribs;
            assert_eq!(attributes.len(), 1);
            assert_eq!(attributes[0].text.value, "TAG");
            assert!(notes.iter().all(|note| !note.contains("block attribute(s) left out")), "{notes:?}");
            let bytes = cadcraft_io::write_with(&d, "old.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
            let back = cadcraft_io::read(&bytes, "old.dwg").unwrap();
            let EntityKind::MLeader(saved) = &back.model.iter().next().unwrap().kind else { panic!("native DWG multileader") };
            let attributes = &saved.block.as_ref().unwrap().attribs;
            assert_eq!(attributes.len(), 1, "{version:?}");
            assert_eq!(attributes[0].text.value, "TAG");
        }
    }
}

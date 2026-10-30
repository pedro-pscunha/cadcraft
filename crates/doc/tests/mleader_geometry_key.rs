//! Every canonical MULTILEADER field is part of the replay key, so changing it alone retires the source snapshot; the
//! text, its style and the arrowhead name keep it. A legacy key is what it was.

use cadcraft_doc::{Common, EntityKind, GeometryRevision, Insert, MLeader, MLeaderContext, MLeaderRoot, MText, geometry_key};
use cadcraft_geom::Vec3;

fn leader() -> MLeader {
    let mut m = MLeader::single(
        Vec3::new(10.0, 20.0, 3.0),
        2.0,
        false,
        vec![vec![Vec3::new(0.0, 0.0, 3.0), Vec3::new(4.0, 6.0, 3.0)]],
        Some(MText {
            insert: Vec3::new(12.0, 20.0, 3.0),
            height: 0.4,
            width: 10.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "T".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
            direction: Some(Vec3::new(1.0, 0.0, 0.0)),
        }),
    );
    m.roots.push(MLeaderRoot::new(Vec3::new(-10.0, 18.0, 3.0), 3.0, true, vec![vec![Vec3::new(-2.0, 0.0, 3.0)]]));
    m.context = MLeaderContext { geometry_revision: GeometryRevision::CanonicalV2, ..m.context };
    m
}

fn key(m: &MLeader) -> String {
    geometry_key(&EntityKind::MLeader(m.clone()), &Common::default()).unwrap()
}

#[test]
fn each_canonical_field_alone_changes_the_key() {
    let base = key(&leader());
    type Mutation = fn(&mut MLeader);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("root landing z", |m| m.roots[1].landing.z += 0.5),
        ("dogleg direction z", |m| m.roots[0].dogleg_dir.z = 0.5),
        ("vertex z", |m| m.roots[0].branches[0].vertices[1].z += 0.5),
        ("arrow size", |m| m.arrow_size += 0.5),
        ("context x axis", |m| m.context.x_axis.z = 0.5),
        ("text normal", |m| m.context.text_normal.x = 0.5),
        ("text direction", |m| m.text.as_mut().unwrap().direction = Some(Vec3::new(0.0, 1.0, 0.0))),
        ("text location z", |m| m.text.as_mut().unwrap().insert.z += 0.5),
        ("block normal", |m| m.context.block_normal.x = 0.5),
        ("block", |m| {
            m.block = Some(Insert {
                block: "B".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            })
        }),
    ];
    let mut keys = vec![base.clone()];
    for (name, mutate) in mutations {
        let mut m = leader();
        mutate(&mut m);
        keys.push(key(&m));
        assert_ne!(base, *keys.last().unwrap(), "{name}");
    }
    for (i, a) in keys.iter().enumerate() {
        assert!(keys[i + 1..].iter().all(|b| a != b), "keys {i} repeat");
    }
}

#[test]
fn the_text_and_its_style_keep_the_key() {
    let mut m = leader();
    let base = key(&m);
    m.text.as_mut().unwrap().contents = "other".into();
    m.text.as_mut().unwrap().style = "Other".into();
    m.arrow_block = "_DOT".into();
    assert_eq!(key(&m), base);
}

#[test]
fn a_legacy_key_does_not_change() {
    let mut m = leader();
    m.context.geometry_revision = GeometryRevision::LegacyProjectedV1;
    m.text.as_mut().unwrap().direction = None;
    let before = key(&m);
    m.arrow_size += 1.0;
    assert_eq!(key(&m), before, "the legacy key never had the arrowhead size");
}

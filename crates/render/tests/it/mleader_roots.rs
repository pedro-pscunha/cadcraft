//! Each root of a multileader draws its own leader lines to its own landing and its own dogleg along its own
//! direction.

use cadcraft_doc::{Common, Drawing, EntityKind, MLeader, MLeaderBranch, MLeaderRoot, Space};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_render::{Kind, Options, build};

fn v(x: f64, y: f64) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

#[test]
fn every_root_draws_its_lines_and_its_dogleg_along_its_own_direction() {
    let mut m = MLeader::single(v(10.0, 20.0), 2.0, false, vec![vec![v(0.0, 0.0), v(4.0, 6.0)]], None);
    m.roots.push(MLeaderRoot {
        id: 9,
        landing: v(-10.0, 18.0),
        dogleg_dir: v(-1.0, 0.0),
        dogleg: 3.0,
        branches: vec![MLeaderBranch { id: 21, vertices: vec![v(-2.0, 0.0), v(-6.0, 5.0)] }],
    });
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::MLeader(m)).unwrap();
    let list = build(&d, &Space::Model, &Options::default());
    let lines: Vec<Vec<Vec2>> = list.prims.iter().filter(|p| p.kind == Kind::Polyline).map(|p| list.points(p).to_vec()).collect();
    let has = |want: &[Vec2]| lines.iter().any(|l| l.len() == want.len() && l.iter().zip(want).all(|(a, b)| a.near(*b, 1e-9)));
    // The leader lines end at their own landing.
    assert!(has(&[Vec2::new(0.0, 0.0), Vec2::new(4.0, 6.0), Vec2::new(10.0, 20.0)]), "{lines:?}");
    assert!(has(&[Vec2::new(-2.0, 0.0), Vec2::new(-6.0, 5.0), Vec2::new(-10.0, 18.0)]), "{lines:?}");
    // And each root has its dogleg along its direction, with its length.
    assert!(has(&[Vec2::new(10.0, 20.0), Vec2::new(12.0, 20.0)]), "{lines:?}");
    assert!(has(&[Vec2::new(-10.0, 18.0), Vec2::new(-13.0, 18.0)]), "{lines:?}");
}

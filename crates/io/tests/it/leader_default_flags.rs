//! From AutoCAD 2010 on a LEADER has no group 71 (arrowhead flag) while the arrowhead is on and no group 72 (path type) while
//! the path is straight, as AutoCAD writes it; earlier versions and other values keep the groups. Every case reads back the same.

use cadcraft_doc::{Common, EntityKind, Leader, LeaderLinks, Space};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn drawing(arrow: bool, spline: bool) -> cadcraft_doc::Drawing {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let leader = Leader {
        vertices: vec![Vec3::ZERO, Vec3::new(5.0, 5.0, 0.0), Vec3::new(10.0, 5.0, 0.0)],
        arrow,
        spline,
        style: "Standard".into(),
        frame: None,
        links: LeaderLinks::default(),
    };
    d.add(&Space::Model, Common::default(), EntityKind::Leader(leader)).unwrap();
    d
}

/// The groups 71 and 72 of the LEADER in a DXF text.
fn flags(d: &cadcraft_doc::Drawing, version: Version) -> (Option<String>, Option<String>) {
    let bytes = cadcraft_io::write_with(d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
    let tags = cadcraft_dxf::parse(&bytes).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.str() == "LEADER").unwrap();
    let body: Vec<_> = tags[start + 1..].iter().take_while(|t| t.code != 0).collect();
    let find = |c| body.iter().find(|t| t.code == c).map(|t| t.str());
    (find(71), find(72))
}

#[test]
fn the_default_flags_are_left_out_from_2010_on() {
    let d = drawing(true, false);
    assert_eq!(flags(&d, Version::R2010), (None, None));
    assert_eq!(flags(&d, Version::R2018), (None, None));
    assert_eq!(flags(&d, Version::R2000), (Some("1".into()), Some("0".into())));
    assert_eq!(flags(&d, Version::R2007), (Some("1".into()), Some("0".into())));
}

#[test]
fn other_values_keep_their_groups_and_read_back() {
    let d = drawing(false, true);
    assert_eq!(flags(&d, Version::R2018), (Some("0".into()), Some("1".into())));
    for (arrow, spline) in [(true, false), (false, true), (true, true), (false, false)] {
        let d = drawing(arrow, spline);
        let bytes = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
        let back = cadcraft_io::read_dxf(&bytes).unwrap();
        let found = back.model.iter().find_map(|e| match &e.kind {
            EntityKind::Leader(l) => Some((l.arrow, l.spline)),
            _ => None,
        });
        assert_eq!(found, Some((arrow, spline)));
    }
}

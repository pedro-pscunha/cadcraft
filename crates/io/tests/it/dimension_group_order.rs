//! AutoCAD reads the groups of a DIMENSION's `AcDbDimension` subclass in a fixed order and refuses the file on another ("unexpected
//! DXF group code: 280"): the version group (280) comes first, and the presentation groups (71, 72, 41, 73, 74, 75) follow the type
//! (70) and precede the measurement (42)'s successors. A dimension read from AutoCAD's own order is written in it.

use cadcraft_io::{SaveOptions, Version};

use super::foreign_passthrough::dxf;

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "DIMENSION"),
        (5, "A8"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbDimension"),
        (280, "0"),
        (2, "*D1"),
        (10, "20"),
        (20, "10"),
        (30, "0"),
        (11, "15"),
        (21, "10"),
        (31, "0"),
        (70, "33"),
        (71, "5"),
        (72, "1"),
        (41, "1"),
        (42, "10"),
        (73, "0"),
        (74, "0"),
        (75, "0"),
        (3, "Standard"),
        (100, "AcDbAlignedDimension"),
        (13, "10"),
        (23, "0"),
        (33, "0"),
        (14, "20"),
        (24, "0"),
        (34, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

/// The group codes of the `AcDbDimension` subclass of the first DIMENSION of a DXF.
fn codes(bytes: &[u8]) -> Vec<i32> {
    let tags = cadcraft_dxf::parse(bytes).unwrap();
    let start = tags.iter().position(|t| t.code == 100 && t.str() == "AcDbDimension").unwrap() + 1;
    tags[start..].iter().take_while(|t| t.code != 100).map(|t| t.code).collect()
}

#[test]
fn the_version_group_follows_the_marker_and_the_others_keep_autocads_order() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for version in [Version::R2010, Version::R2018] {
        let bytes = cadcraft_io::write_with(&d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let written = codes(&bytes);
        assert_eq!(written.first(), Some(&280), "{version:?}: {written:?}");
        let rank = |c: i32| [280, 2, 10, 11, 70, 71, 72, 41, 42, 73, 74, 75, 1, 53, 51, 210, 3].iter().position(|x| *x == c);
        let ranks: Vec<usize> = written
            .iter()
            .filter_map(|c| match c {
                20 | 30 => None,
                21 | 31 => None,
                220 | 230 => None,
                c => rank(*c),
            })
            .collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{version:?}: groups out of order: {written:?}");
    }
}

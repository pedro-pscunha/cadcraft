//! A block reference with an array (MINSERT) has its own subclass marker, `AcDbMInsertBlock`. With the plain
//! `AcDbBlockReference` AutoCAD ignores the counts (70, 71) and the spacing (44, 45) and shows one instance.
//! The marker is written for every array, kept through the geometry snapshot, and the DWG keeps the array.

use cadcraft_doc::{Drawing, EntityKind, Insert};
use cadcraft_dxf::{Tag, Version, records, sections};
use cadcraft_io::{SaveOptions, read_dxf, write_with};

fn save(d: &Drawing, version: Version) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(version), binary: false }).unwrap()).unwrap()
}

/// The DXF text of an ENTITIES section from `code value` lines.
fn dxf(entities: &str) -> String {
    let mut text = String::from("0\nSECTION\n2\nENTITIES\n");
    for line in entities.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        text.push_str(&format!("{code}\n{value}\n"));
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

fn inserts(text: &str) -> Vec<Vec<Tag>> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    records(&section.tags).into_iter().filter(|(k, _)| k == "INSERT").map(|(_, t)| t).collect()
}

fn subclass(record: &[Tag]) -> Vec<String> {
    record.iter().filter(|t| t.code == 100).map(|t| t.str()).collect()
}

fn insert(d: &Drawing) -> Insert {
    d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i.clone()) } else { None }).expect("an insert")
}

const BITS: [i32; 3] = [210, 220, 230];

/// AutoCAD's own record of a 3 by 2 array in a tilted plane (the plane and the signs are as it wrote them).
const SOURCE: &str = "0 INSERT\n5 101\n100 AcDbEntity\n8 0\n100 AcDbMInsertBlock\n2 PointBlock\n10 10.0\n20 20.0\n30 7.0\n41 -2.0\n42 3.0\n43 0.5\n50 30.0\n\
    70 3\n71 2\n44 5.0\n45 -4.0\n210 0.0\n220 0.8\n230 0.6";

#[test]
fn an_array_is_written_with_the_minsert_marker_and_a_single_insert_without_it() {
    let mut d = read_dxf(dxf(SOURCE).as_bytes()).unwrap();
    assert_eq!((insert(&d).cols, insert(&d).rows, insert(&d).col_spacing, insert(&d).row_spacing), (3, 2, 5.0, -4.0));
    for version in Version::ALL {
        let text = save(&d, version);
        let record = &inserts(&text)[0];
        assert_eq!(subclass(record), ["AcDbEntity", "AcDbMInsertBlock"], "{version:?}");
        let get = |code: i32| record.iter().find(|t| t.code == code).map(|t| t.str());
        assert_eq!((get(70).as_deref(), get(71).as_deref()), (Some("3"), Some("2")), "{version:?}");
    }
    // One instance, or an edit that leaves one, is a plain block reference.
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            (i.cols, i.rows) = (1, 1);
        }
    })
    .unwrap();
    let record = &inserts(&save(&d, Version::R2018))[0];
    assert_eq!(subclass(record), ["AcDbEntity", "AcDbBlockReference"]);
    assert!(!record.iter().any(|t| matches!(t.code, 70 | 71 | 44 | 45)));
}

#[test]
fn an_array_keeps_its_plane_and_is_written_again_as_the_marker_says() {
    // The second one is what CADCraft wrote before: the array under the plain marker.
    let old = SOURCE.replace("AcDbMInsertBlock", "AcDbBlockReference");
    for source in [SOURCE.to_string(), old] {
        let d = read_dxf(dxf(&source).as_bytes()).unwrap();
        for version in [Version::R2000, Version::R2018] {
            let record = &inserts(&save(&d, version))[0];
            assert_eq!(subclass(record), ["AcDbEntity", "AcDbMInsertBlock"], "{version:?}");
            let plane: Vec<f64> = BITS.iter().map(|c| record.iter().find(|t| t.code == *c).unwrap().str().parse().unwrap()).collect();
            assert_eq!(plane, [0.0, 0.8, 0.6], "{version:?}: the plane of the source");
            let scale: Vec<f64> = [41, 42, 43].iter().map(|c| record.iter().find(|t| t.code == *c).unwrap().str().parse().unwrap()).collect();
            assert_eq!(scale, [-2.0, 3.0, 0.5], "{version:?}: the signs of the source");
        }
    }
}

#[test]
fn a_dwg_keeps_the_array() {
    let d = read_dxf(dxf(SOURCE).as_bytes()).unwrap();
    let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    let i = insert(&back);
    assert_eq!((i.cols, i.rows), (3, 2));
    assert_eq!((i.col_spacing, i.row_spacing), (5.0, -4.0));
}

//! A legacy MLINE keeps the direction (12) and miter (13) of its record through a whole-object edit and a save, though they are
//! not the ones its points give: MOVE, ROTATE, SCALE and MIRROR move them with the vertices, and a DXF or DWG save writes them.
//! Moving a vertex makes them stale and they are computed again.

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, MLine, MLineJust, MLineStyle, Space};
use cadcraft_dxf::Tag;
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

/// A multiline saved by CADCraft, with the miter at its first vertex tilted by the test (an AutoCAD file can have any).
fn source() -> Vec<u8> {
    let mut d = Drawing::new_metric();
    let m = MLine::new(&MLineStyle::default(), &[Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(20.0, 10.0)], false, MLineJust::Zero, 1.0);
    d.add(&Space::Model, Common::default(), EntityKind::MLine(m)).unwrap();
    let mut tags = cadcraft_dxf::parse(&cadcraft_io::write(&d, "x.dxf").unwrap()).unwrap();
    let start = tags.iter().position(|t| t.code == 0 && t.str() == "MLINE").expect("an MLINE");
    let at = start + tags[start..].iter().position(|t| t.code == 13).expect("a miter");
    tags[at] = Tag::f(13, 0.6);
    tags[at + 1] = Tag::f(23, 0.8);
    cadcraft_dxf::write_ascii(&tags).into_bytes()
}

fn mline(d: &Drawing) -> (Entity, MLine) {
    let e = d.model.iter().find(|e| matches!(e.kind, EntityKind::MLine(_))).expect("a multiline").as_ref().clone();
    let EntityKind::MLine(m) = e.kind.clone() else { unreachable!() };
    (e, m)
}

fn near(a: Vec2, b: Vec2) -> bool {
    a.dist(b) < 1e-9
}

fn maps() -> Vec<(&'static str, Mat3)> {
    vec![
        ("move", Mat3::translate(Vec2::new(10.0, 5.0))),
        ("rotate", Mat3::rotate_about(Vec2::new(1.0, 1.0), 0.6)),
        ("scale", Mat3::scale_about(Vec2::new(1.0, 1.0), 2.5)),
        ("mirror", Mat3::mirror(Vec2::new(0.0, 1.0), Vec2::new(3.0, 2.0))),
    ]
}

#[test]
fn the_record_vectors_are_read_and_follow_each_edit_through_the_dxf_and_the_dwg() {
    let d = cadcraft_io::read_dxf(&source()).unwrap();
    let (e, m0) = mline(&d);
    let v0 = m0.source_vectors().expect("the source vectors");
    assert!(near(v0[0].1, Vec2::new(0.6, 0.8)), "the tilted miter is read: {:?}", v0[0].1);
    assert!(!near(v0[0].1, m0.miters()[0].normalized()), "it is not the one the points give");
    for (name, m) in maps() {
        let moved = e.transformed(&Mat4::from_mat3(&m)).expect("the edit");
        let mut out = Drawing::new_metric();
        out.model.push(moved);
        for file in ["x.dxf", "x.dwg"] {
            let back = cadcraft_io::read(&cadcraft_io::write(&out, file).unwrap(), file).unwrap();
            let (_, mb) = mline(&back);
            let got = mb.source_vectors().unwrap_or_else(|| panic!("{name}, {file}: no vectors"));
            for (k, ((gd, gm), (sd, sm))) in got.iter().zip(&v0).enumerate() {
                let (wd, wm) = (m.apply_vec(*sd).normalized(), m.apply_vec(*sm).normalized());
                assert!(near(*gd, wd), "{name}, {file}: direction {k}: {gd:?} != {wd:?}");
                assert!(near(*gm, wm), "{name}, {file}: miter {k}: {gm:?} != {wm:?}");
            }
        }
    }
}

#[test]
fn a_moved_vertex_makes_the_source_vectors_stale() {
    let d = cadcraft_io::read_dxf(&source()).unwrap();
    let (mut e, _) = mline(&d);
    if let EntityKind::MLine(m) = &mut e.kind {
        m.vertices[1].p = m.vertices[1].p + Vec3::new(0.0, 3.0, 0.0);
    }
    let mut out = Drawing::new_metric();
    out.model.push(e);
    let back = cadcraft_io::read_dxf(&cadcraft_io::write(&out, "x.dxf").unwrap()).unwrap();
    let (_, m) = mline(&back);
    let v = m.source_vectors().expect("vectors of the saved file");
    // The first miter is the one the new points give: perpendicular to the first segment.
    assert!(near(v[0].1, m.miters()[0].normalized()), "{:?}", v[0].1);
}

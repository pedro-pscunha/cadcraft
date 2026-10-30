//! A legacy WIPEOUT keeps the image frame of its record (origin, pixel vectors, size, clip type and clip) through a whole-object
//! edit: MOVE, ROTATE, SCALE and MIRROR move the frame with the outline, and a DXF or DWG save writes it, not a synthetic
//! 1x1 frame over the boundary's extents. Reshaping the boundary writes the synthetic frame.

use cadcraft_doc::{Drawing, Entity, EntityKind, Wipeout, WipeoutFrame};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

fn source(clip_type: i64, clip: &[(f64, f64)]) -> String {
    let mut s = String::from(
        "0\nSECTION\n2\nENTITIES\n0\nWIPEOUT\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbWipeout\n90\n0\n10\n10\n20\n2\n30\n0\n11\n0.5\n21\n0.1\n31\n0\n12\n-0.1\n22\n0.5\n32\n0\n13\n20\n23\n10\n70\n13\n280\n1\n281\n60\n282\n40\n283\n10\n",
    );
    s.push_str(&format!("71\n{clip_type}\n91\n{}\n", clip.len()));
    for (x, y) in clip {
        s.push_str(&format!("14\n{x}\n24\n{y}\n"));
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

fn wipeout(d: &Drawing) -> (Entity, Wipeout) {
    let e = d.model.iter().find(|e| matches!(e.kind, EntityKind::Wipeout(_))).expect("a wipeout").as_ref().clone();
    let EntityKind::Wipeout(w) = e.kind.clone() else { unreachable!() };
    (e, w)
}

fn near(a: Vec3, b: Vec3) -> bool {
    (a - b).len() < 1e-9
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
fn the_image_frame_follows_each_edit_and_is_written_in_the_dxf_and_the_dwg() {
    for (clip_type, clip) in [(2, vec![(2.0, 2.0), (17.0, 2.0), (17.0, 8.0), (2.0, 8.0), (2.0, 2.0)]), (1, vec![(-0.5, -0.5), (19.5, 9.5)])] {
        let d = cadcraft_io::read_dxf(source(clip_type, &clip).as_bytes()).unwrap();
        let (e, w0) = wipeout(&d);
        let f0 = w0.image.clone().expect("the source frame");
        assert_eq!((f0.clip_type, f0.clip.len(), f0.size), (u8::try_from(clip_type).unwrap(), clip.len(), Vec2::new(20.0, 10.0)));
        for (name, m) in maps() {
            let moved = e.transformed(&Mat4::from_mat3(&m)).expect("the edit");
            let mut out = Drawing::new_metric();
            out.model.push(moved);
            for file in ["x.dxf", "x.dwg"] {
                let back = cadcraft_io::read(&cadcraft_io::write(&out, file).unwrap(), file).unwrap();
                let (_, w) = wipeout(&back);
                let f = w.image.as_ref().unwrap_or_else(|| panic!("{name}, {file}: no frame"));
                assert!(near(f.origin, m.apply(f0.origin.xy()).to3(0.0)), "{name}, {file}: origin {:?}", f.origin);
                assert!(near(f.u, m.apply_vec(f0.u.xy()).to3(0.0)) && near(f.v, m.apply_vec(f0.v.xy()).to3(0.0)), "{name}, {file}: pixel vectors");
                assert_eq!((f.size, f.clip_type), (f0.size, f0.clip_type), "{name}, {file}: size and clip type");
                assert!(
                    f.clip.iter().zip(&f0.clip).all(|(a, b)| a.dist(*b) < 1e-6) && f.clip.len() == f0.clip.len(),
                    "{name}, {file}: clip {:?}",
                    f.clip
                );
                assert_eq!(w.display, w0.display, "{name}, {file}: display settings");
            }
        }
    }
}

#[test]
fn a_reshaped_boundary_is_written_with_a_frame_of_its_own() {
    let d = cadcraft_io::read_dxf(source(2, &[(2.0, 2.0), (17.0, 2.0), (17.0, 8.0), (2.0, 8.0), (2.0, 2.0)]).as_bytes()).unwrap();
    let (mut e, _) = wipeout(&d);
    if let EntityKind::Wipeout(w) = &mut e.kind {
        w.boundary[0] += Vec2::new(1.0, 1.0);
    }
    let EntityKind::Wipeout(w) = &e.kind else { unreachable!() };
    assert!(w.source_image().is_none());
    let mut out = Drawing::new_metric();
    out.model.push(e);
    let back = cadcraft_io::read_dxf(&cadcraft_io::write(&out, "x.dxf").unwrap()).unwrap();
    let (_, w) = wipeout(&back);
    let f: WipeoutFrame = w.image.expect("a frame");
    assert_eq!(f.size, Vec2::new(1.0, 1.0), "the synthetic frame is 1x1");
}

//! A LEADER keeps what its record holds through a whole-object edit and a save: the annotation type (73), hookline direction and
//! flag (74, 75), annotation size (40, 41), horizontal direction (211), offsets (212, 213), plane (210) and the annotation it points
//! at (340). MOVE, ROTATE, SCALE and MIRROR move the vectors and scale the sizes.

use cadcraft_doc::{Drawing, Entity, EntityKind, Leader};
use cadcraft_geom::{Mat3, Mat4, Vec2, Vec3};

const SOURCE: &str = "0\nSECTION\n2\nENTITIES\n\
0\nMTEXT\n5\n41\n100\nAcDbEntity\n8\n0\n100\nAcDbMText\n10\n8\n20\n3\n30\n0\n40\n1\n41\n0\n71\n1\n1\nNOTE\n7\nStandard\n\
0\nLEADER\n5\n40\n100\nAcDbEntity\n8\n0\n100\nAcDbLeader\n3\nStandard\n71\n1\n72\n0\n73\n0\n74\n1\n75\n1\n40\n2.5\n41\n6\n76\n3\n\
10\n0\n20\n0\n30\n0\n10\n4\n20\n3\n30\n0\n10\n8\n20\n3\n30\n0\n340\n41\n210\n0\n220\n0\n230\n1\n\
211\n1\n221\n0\n231\n0\n212\n0.5\n222\n0.5\n232\n0\n213\n1\n223\n0.2\n233\n0\n0\nENDSEC\n0\nEOF\n";

fn leader_entity(d: &Drawing) -> Entity {
    d.model.iter().find(|e| matches!(e.kind, EntityKind::Leader(_))).expect("a leader").as_ref().clone()
}

fn leader(e: &Entity) -> Leader {
    let EntityKind::Leader(l) = &e.kind else { panic!("not a leader") };
    l.clone()
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
fn the_record_is_read_whole() {
    let d = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let l = leader(&leader_entity(&d));
    let f = l.frame.as_ref().expect("the frame");
    assert_eq!((f.text_height, f.text_width), (2.5, 6.0));
    assert!(
        near(f.direction, Vec3::new(1.0, 0.0, 0.0))
            && near(f.block_offset, Vec3::new(0.5, 0.5, 0.0))
            && near(f.annotation_offset, Vec3::new(1.0, 0.2, 0.0))
    );
    assert_eq!((l.links.creation, l.links.hookline_direction, l.links.hookline), (Some(0), Some(1), Some(1)));
    assert_eq!(l.links.annotation, Some(cadcraft_doc::Handle(0x41)));
}

#[test]
fn each_edit_moves_the_vectors_and_keeps_the_links_through_the_dxf_and_the_dwg() {
    let d = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let source = leader_entity(&d);
    let text = d.model.iter().find(|e| matches!(e.kind, EntityKind::MText(_))).expect("the annotation").as_ref().clone();
    let l0 = leader(&source);
    let f0 = l0.frame.clone().unwrap();
    for (name, m) in maps() {
        let k = m.scale_factor();
        let moved = source.transformed(&Mat4::from_mat3(&m)).expect("the edit");
        let mut out = Drawing::new_metric();
        out.model.push(text.clone());
        out.model.push(moved);
        for file in ["x.dxf", "x.dwg"] {
            let back = cadcraft_io::read(&cadcraft_io::write(&out, file).unwrap(), file).unwrap();
            let l = leader(&leader_entity(&back));
            assert!(l.vertices.iter().zip(&l0.vertices).all(|(a, b)| near(*a, m.apply(b.xy()).to3(0.0))), "{name}, {file}: vertices");
            let f = l.frame.as_ref().unwrap_or_else(|| panic!("{name}, {file}: no frame"));
            let dir = m.apply_vec(f0.direction.xy()).normalized().to3(0.0);
            assert!(near(f.direction, dir), "{name}, {file}: direction {:?} != {dir:?}", f.direction);
            assert!(near(f.block_offset, m.apply_vec(f0.block_offset.xy()).to3(0.0)), "{name}, {file}: block offset");
            assert!(near(f.annotation_offset, m.apply_vec(f0.annotation_offset.xy()).to3(0.0)), "{name}, {file}: annotation offset");
            // The DWG gives the annotation height (40) back as the style's and the width (41) as 0: acadrust does not carry them.
            if file == "x.dxf" {
                assert!((f.text_height - f0.text_height * k).abs() < 1e-9, "{name}, {file}: annotation height");
                assert!((f.text_width - f0.text_width * k).abs() < 1e-9, "{name}, {file}: annotation width");
            }
            assert_eq!((l.links.creation, l.links.hookline_direction), (Some(0), Some(1)), "{name}, {file}: type and hookline direction");
            // The DWG gives the hookline flag (75) back as 0: acadrust does not carry it.
            if file == "x.dxf" {
                assert_eq!(l.links.hookline, Some(1), "{name}, {file}: hookline flag");
            }
            assert_eq!(l.links.annotation, Some(cadcraft_doc::Handle(0x41)), "{name}, {file}: annotation");
        }
    }
}

#[test]
fn an_annotation_that_is_gone_is_not_written_as_a_dangling_handle() {
    let d = cadcraft_io::read_dxf(SOURCE.as_bytes()).unwrap();
    let mut out = Drawing::new_metric();
    out.model.push(leader_entity(&d));
    let back = cadcraft_io::read_dxf(&cadcraft_io::write(&out, "x.dxf").unwrap()).unwrap();
    assert_eq!(leader(&leader_entity(&back)).links.annotation, None);
}

#[test]
fn a_leader_made_here_writes_none_of_them() {
    let mut out = Drawing::new_metric();
    let handle = out.new_handle();
    out.model.push(Entity::new(handle, EntityKind::Leader(Leader { vertices: vec![Vec3::ZERO, Vec3::new(3.0, 2.0, 0.0)], ..Leader::default() })));
    let text = String::from_utf8(cadcraft_io::write(&out, "x.dxf").unwrap()).unwrap();
    let leader = &text[text.find("AcDbLeader").unwrap()..];
    let record = &leader[..leader.find("\n  0\n").unwrap_or(leader.len())];
    assert!(!record.contains("\n340\n") && !record.contains("\n211\n") && !record.contains("\n 73\n"), "{record}");
}

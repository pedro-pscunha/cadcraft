//! A dimension's links go in AutoCAD's DIMASSOC object; the private `CADCRAFT` ASSOC xdata is written only for the links that object
//! does not give back the same (an arbitrary angle on a circle, say), and files that have the xdata still read.

use cadcraft_doc::{AssocSnap, Circle, Common, DimAssoc, DimKind, Dimension, EntityKind, Line, Space};
use cadcraft_geom::Vec3;

fn dimension(kind: DimKind, defpt: Vec3, p14: Vec3, assoc: Vec<DimAssoc>) -> Dimension {
    Dimension {
        kind,
        defpt,
        text_mid: Vec3::ZERO,
        p13: Vec3::ZERO,
        p14,
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides: Default::default(),
        assoc,
        marks: Default::default(),
        leader_length: 0.0,
    }
}

fn assoc_xdata(text: &str) -> bool {
    cadcraft_dxf::parse(text.as_bytes()).unwrap().iter().any(|t| t.code == 1000 && t.str() == "ASSOC")
}

#[test]
fn links_the_standard_object_gives_back_travel_without_private_xdata() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let c = Common::default;
    let l1 = d.add(&Space::Model, c(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(0.0, 5.0, 0.0) })).unwrap();
    let l2 = d.add(&Space::Model, c(), EntityKind::Line(Line { a: Vec3::new(10.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) })).unwrap();
    let links = vec![
        DimAssoc { point: "p13".into(), handle: l1, snap: AssocSnap::Start },
        DimAssoc { point: "p14".into(), handle: l2, snap: AssocSnap::Start },
    ];
    let dim = d
        .add(
            &Space::Model,
            c(),
            EntityKind::Dimension(dimension(DimKind::Linear { rotation: 0.0 }, Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), links.clone())),
        )
        .unwrap();
    let text = cadcraft_io::write_dxf(&d);
    assert!(text.contains("AcDbOsnapPointRef") && !assoc_xdata(&text));
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let EntityKind::Dimension(dm) = &back.entity(dim).unwrap().kind else { panic!("dimension") };
    assert_eq!(dm.assoc, links);
}

#[test]
fn links_it_cannot_give_back_keep_the_xdata() {
    let mut d = cadcraft_doc::Drawing::new_metric();
    let circle = d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(20.0, 0.0, 0.0), radius: 2.0 })).unwrap();
    // A point at 1 radian on the circle: the standard object names a quadrant or the center, not this.
    let links = vec![DimAssoc { point: "p13".into(), handle: circle, snap: AssocSnap::OnCircle { angle: 1.0 } }];
    let dim =
        d.add(&Space::Model, Common::default(), EntityKind::Dimension(dimension(DimKind::Aligned, Vec3::ZERO, Vec3::ZERO, links.clone()))).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    assert!(assoc_xdata(&text));
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let EntityKind::Dimension(dm) = &back.entity(dim).unwrap().kind else { panic!("dimension") };
    assert_eq!(dm.assoc, links);
}

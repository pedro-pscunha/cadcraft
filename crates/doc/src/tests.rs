use crate::*;
use cadcraft_geom::{Mat3, Vec2, Vec3};

fn line(a: (f64, f64), b: (f64, f64)) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })
}

#[test]
fn new_drawing_has_standard_tables() {
    let d = Drawing::new_imperial();
    assert!(d.layer("0").is_some());
    assert!(d.linetype("continuous").is_some());
    assert!(d.text_style("Standard").is_some());
    assert_eq!(d.header.f64("TEXTSIZE", 0.0), 0.2);
    let m = Drawing::new_metric();
    assert!(m.dim_style("ISO-25").is_some());
}

#[test]
fn new_metric_drawing_has_metric_multileader_and_table_styles() {
    let m = Drawing::new_metric();
    let iso = m.dim_style("ISO-25").unwrap();
    let ml = m.mleader_styles.iter().find(|s| s.name == "Standard").unwrap();
    assert_eq!((ml.text_height, ml.arrow_size), (iso.text_height, iso.arrow_size));
    assert_eq!(ml.dogleg, 5.0);
    let ts = m.table_styles.iter().find(|s| s.name == "Standard").unwrap();
    assert_eq!(ts.text_height, iso.text_height);
    assert_eq!(iso.text_height, 2.5);

    let i = Drawing::new_imperial();
    assert_eq!(i.mleader_styles, vec![MLeaderStyle::default()]);
    assert_eq!(i.table_styles, vec![TableStyle::default()]);
    assert_eq!((i.mleader_styles[0].text_height, i.table_styles[0].text_height), (0.18, 0.18));
}

#[test]
fn legacy_point_display_angle_follows_rotation_and_mirror() {
    let angle = std::f64::consts::FRAC_PI_6;
    let mut rotation = cadcraft_geom::Mat4::IDENTITY;
    rotation.m[0][0] = 0.0;
    rotation.m[0][1] = -1.0;
    rotation.m[1][0] = 1.0;
    rotation.m[1][1] = 0.0;
    let mirror = cadcraft_geom::Mat4::from_mat3(&Mat3::mirror(Vec2::ZERO, Vec2::Y));
    for (matrix, expected) in [(rotation, angle + std::f64::consts::FRAC_PI_2), (mirror, std::f64::consts::PI - angle)] {
        let mut entity = Entity::new(Handle(7), EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 0.0), angle }));
        assert_eq!(entity.geometry_revision, GeometryRevision::LegacyProjectedV1);
        entity.try_transform(&matrix).unwrap();
        let EntityKind::Point(point) = entity.kind else { panic!("expected POINT") };
        assert!((point.angle - expected).abs() < 1e-14, "{} != {expected}", point.angle);
    }
}

#[test]
fn legacy_arc_translation_keeps_angles_far_from_the_origin() {
    let start = 0.3;
    let end = 0.9;
    let mut entity = Entity::new(
        Handle(8),
        EntityKind::Arc(Arc { center: Vec3::new(1e9, -2e9, 0.0), radius: 0.001, start, end }),
    );
    let mut translation = cadcraft_geom::Mat4::IDENTITY;
    translation.m[0][3] = 17.0;
    translation.m[1][3] = -23.0;
    entity.try_transform(&translation).unwrap();
    let EntityKind::Arc(arc) = entity.kind else { panic!("expected ARC") };
    assert!((arc.start - start).abs() < 1e-14, "{} != {start}", arc.start);
    assert!((arc.end - end).abs() < 1e-14, "{} != {end}", arc.end);
}

#[test]
fn transform_arc_mirror_keeps_shape() {
    let k = EntityKind::Arc(Arc { center: Vec3::ZERO, radius: 1.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 });
    let mut entity = Entity::new(Handle(0), k);
    entity.try_transform(&cadcraft_geom::Mat4::from_mat3(&Mat3::mirror(Vec2::ZERO, Vec2::Y))).unwrap();
    if let EntityKind::Arc(a) = entity.kind {
        let g = cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
        assert!(g.mid_point().near(Vec2::from_angle(3.0 * std::f64::consts::FRAC_PI_4), 1e-9));
    } else {
        panic!()
    }
}

#[test]
fn entity_serde_roundtrip() {
    let e = Entity::new(Handle(7), line((1.0, 2.0), (3.0, 4.0)));
    let s = serde_json::to_string(&e).unwrap();
    let back: Entity = serde_json::from_str(&s).unwrap();
    assert_eq!(e, back);
}

#[test]
fn paper_space_entities() {
    let mut d = Drawing::new_imperial();
    let h = d.add(&Space::Paper("Layout1".into()), Common::default(), line((0.0, 0.0), (1.0, 0.0))).unwrap();
    assert_eq!(d.space_of(h), Some(Space::Paper("Layout1".into())));
    assert!(d.remove_entity(h).is_some());
    assert_eq!(d.entity_count(), 0);
}

#[test]
fn space_names_its_tab() {
    assert!(!Space::Model.is_paper());
    assert_eq!(Space::Model.layout(), None);
    assert_eq!(Space::Model.tab_name(), "Model");
    let p = Space::Paper("Sheet A".into());
    assert!(p.is_paper());
    assert_eq!(p.layout(), Some("Sheet A"));
    assert_eq!(p.tab_name(), "Sheet A");
}

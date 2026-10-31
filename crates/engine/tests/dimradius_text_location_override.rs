//! DIMRADIUS decides what a text location outside the circle means from the style in force, overrides included (SETVAR), not from the named style alone.

use cadcraft_doc::{Circle, Drawing, Entity, EntityKind, Handle};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

/// A circle of radius 8 at the origin, the current style `ISO-25` with `style_dimtad`, then SETVAR DIMTAD `override_dimtad`.
fn session(style_dimtad: i64, override_dimtad: i64) -> Session {
    let mut d = Drawing::new_metric();
    d.model.push(Entity::new(Handle(0x10), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 8.0 })));
    let mut s = Session::new();
    s.open_drawing(d, "x.dxf", None);
    s.execute("dimstyle", &json!({"name": "ISO-25", "DIMTAD": style_dimtad})).unwrap();
    s.execute("setvar", &json!({"name": "DIMTAD", "value": override_dimtad})).unwrap();
    s
}

fn made(s: &mut Session) -> cadcraft_doc::Dimension {
    let r = s.execute("dimradius", &json!({"handle": "10", "at": [50, 0]})).unwrap();
    match &s.doc().unwrap().entity(Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap()).unwrap().kind {
        EntityKind::Dimension(dm) => dm.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_beside_the_line_in_the_style_but_not_in_the_override_goes_where_the_user_put_it() {
    let dm = made(&mut session(1, 0));
    assert!(dm.user_text_pos, "the override DIMTAD 0 puts the text at the location");
    assert_eq!((dm.text_mid.x, dm.text_mid.y), (50.0, 0.0));
}

#[test]
fn text_centred_in_the_style_but_beside_the_line_in_the_override_gives_only_the_direction() {
    let dm = made(&mut session(0, 1));
    assert!(!dm.user_text_pos, "the override DIMTAD 1 lays the text out itself");
}

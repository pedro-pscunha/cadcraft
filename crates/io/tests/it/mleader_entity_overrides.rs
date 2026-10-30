//! The own arrowhead bit (0x100 of group 90) of a multileader is written as the file had it while the arrowhead is still the one it
//! was read with, so a change of the style's arrowhead still reaches the multileader; and the leader line type (group 341 of the
//! entity) comes back.

use cadcraft_doc::*;
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, Version};

fn pairs(dxf: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = dxf.lines().collect();
    lines.chunks(2).filter_map(|c| Some((c.first()?.trim().parse().ok()?, c.get(1)?.trim().to_string()))).collect()
}

fn save(d: &Drawing) -> String {
    let opts = SaveOptions { version: Some(Version::R2010), binary: false };
    String::from_utf8(cadcraft_io::write_with(d, "x.dxf", &opts).unwrap()).unwrap()
}

/// The multileader record: its groups after the end of the content context (301 `}`).
fn entity_part(p: &[(i32, String)]) -> Vec<(i32, String)> {
    let start = p.iter().position(|(c, v)| *c == 0 && v == "MULTILEADER").unwrap();
    let rec: Vec<(i32, String)> = p[start + 1..].iter().take_while(|(c, _)| *c != 0).cloned().collect();
    let ctx_end = rec.iter().position(|(c, v)| *c == 301 && v == "}").unwrap();
    rec[ctx_end..].to_vec()
}

fn override_flags(text: &str) -> i64 {
    let part = entity_part(&pairs(text));
    part.iter().find(|(c, _)| *c == 90).and_then(|(_, v)| v.parse().ok()).unwrap()
}

fn drawing(arrow: &str) -> Drawing {
    let mut d = Drawing::new_metric();
    d.mleader_styles.push(MLeaderStyle { name: "Callout".into(), arrow_block: "_DOT".into(), ..MLeaderStyle::metric() });
    d.linetypes.push(Linetype { name: "DASHED".into(), ..Linetype::continuous() });
    let text = MText {
        insert: Vec3::new(5.9, 4.0, 0.0),
        height: 0.4,
        width: 6.0,
        attach: 4,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "NOTE".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
        drawing_direction: None,
    };
    let lines = vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 3.0, 0.0)]];
    let mut m = MLeader::single(Vec3::new(5.0, 4.0, 0.0), 0.75, false, lines, Some(text));
    m.style = "Callout".into();
    m.arrow_size = 0.3;
    m.arrow_block = arrow.into();
    d.model.push(Entity::new(Handle(0x80), EntityKind::MLeader(m)));
    d
}

fn mleader(d: &Drawing) -> MLeader {
    d.model.iter().find_map(|e| if let EntityKind::MLeader(m) = &e.kind { Some(m.clone()) } else { None }).unwrap()
}

#[test]
fn the_own_arrowhead_bit_follows_the_file_while_the_arrowhead_is_unchanged() {
    // Made here with an arrowhead: an override.
    let text = save(&drawing("_DOT"));
    assert_eq!(override_flags(&text) & 0x100, 0x100);
    // The file gave the style's own arrowhead without the bit (AutoCAD does): clear it in the text.
    let mut p = pairs(&text);
    let start = p.iter().position(|(c, v)| *c == 0 && v == "MULTILEADER").unwrap();
    let ctx_end = start + p[start..].iter().position(|(c, v)| *c == 301 && v == "}").unwrap();
    let flags = ctx_end + p[ctx_end..].iter().position(|(c, _)| *c == 90).unwrap();
    p[flags].1 = (p[flags].1.parse::<i64>().unwrap() & !0x100).to_string();
    let edited: String = p.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect();
    let mut d = cadcraft_io::read_dxf(edited.as_bytes()).unwrap();
    assert_eq!(override_flags(&save(&d)) & 0x100, 0, "written as the file had it");
    // An arrowhead other than the style's is an override.
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.arrow_block = "_NONE".into();
        }
    })
    .unwrap();
    assert_eq!(override_flags(&save(&d)) & 0x100, 0x100, "a changed arrowhead is an override");
}

#[test]
fn the_leader_line_type_comes_back() {
    let mut d = drawing("_DOT");
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.context.codes.line_type = Some("DASHED".into());
        }
    })
    .unwrap();
    let text = save(&d);
    let p = pairs(&text);
    let ltype = {
        let start = p.iter().position(|(c, v)| *c == 2 && v == "DASHED").unwrap();
        p[..start].iter().rev().find(|(c, _)| *c == 5).map(|(_, v)| v.clone()).unwrap()
    };
    assert!(entity_part(&p).contains(&(341, ltype)), "341 names the DASHED record");
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_eq!(mleader(&back).context.codes.line_type.as_deref(), Some("DASHED"));
}

#[test]
fn the_dogleg_length_of_the_entity_is_kept_beside_the_roots_own() {
    let mut d = drawing("_DOT");
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.roots[0].dogleg = 0.009;
            m.context.codes.groups.entity.dogleg_length = Some(0.1);
        }
    })
    .unwrap();
    let text = save(&d);
    let part = entity_part(&pairs(&text));
    let lengths: Vec<f64> = part.iter().filter(|(c, _)| *c == 41).filter_map(|(_, v)| v.parse().ok()).collect();
    assert_eq!(lengths, [0.1], "the entity's own value, not the root's 0.009");
    let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    assert_eq!(mleader(&back).context.codes.groups.entity.dogleg_length, Some(0.1));
    assert!((mleader(&back).roots[0].dogleg - 0.009).abs() < 1e-9);
}

/// The arrowhead size of the entity (42) is the file's while the size is the one read: AutoCAD scales the context's (140) by the annotation
/// scale and keeps its own unscaled, so one written from the other makes the arrow 35 times too small at the next regeneration. A size
/// that was edited is written to both.
#[test]
fn the_entity_arrowhead_size_is_kept_beside_the_contexts() {
    let mut d = drawing("_DOT");
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.arrow_size = 0.3;
            m.context.codes.groups.ctx.arrow_size = Some(0.3);
            m.context.codes.groups.entity.arrow_size = Some(9.0);
        }
    })
    .unwrap();
    let sizes = |d: &Drawing| -> (Vec<f64>, Vec<f64>) {
        let p = pairs(&save(d));
        let start = p.iter().position(|(c, v)| *c == 0 && v == "MULTILEADER").unwrap();
        let rec: Vec<(i32, String)> = p[start + 1..].iter().take_while(|(c, _)| *c != 0).cloned().collect();
        let ctx_end = rec.iter().position(|(c, v)| *c == 301 && v == "}").unwrap();
        let of =
            |part: &[(i32, String)], code: i32| part.iter().filter(|(c, _)| *c == code).filter_map(|(_, v)| v.parse().ok()).collect::<Vec<f64>>();
        (of(&rec[..ctx_end], 140), of(&rec[ctx_end..], 42))
    };
    assert_eq!(sizes(&d), (vec![0.3], vec![9.0]));
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.arrow_size = 0.5;
        }
    })
    .unwrap();
    assert_eq!(sizes(&d), (vec![0.5], vec![0.5]));
}

//! AutoCAD cuts the text of an MTEXT in chunks of 250 characters of the text itself (groups 3, then 1 for the rest);
//! a tab is one character there although the DXF writes it as `^I`.

use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, MText};
use cadcraft_geom::Vec3;

fn chunks(contents: &str) -> Vec<(i32, String)> {
    let mut d = Drawing::new_metric();
    let text = MText {
        insert: Vec3::ZERO,
        height: 2.5,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        contents: contents.into(),
        style: "Standard".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
        drawing_direction: None,
    };
    d.model.push(Entity::new(Handle(0x200), EntityKind::MText(text)));
    let dxf = cadcraft_io::write_dxf(&d);
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let record = cadcraft_dxf::records(&section.tags).into_iter().find(|(k, _)| k == "MTEXT").unwrap().1;
    record.iter().filter(|t| matches!(t.code, 1 | 3)).map(|t| (t.code, t.str())).collect()
}

#[test]
fn a_tab_counts_as_one_character_of_the_chunk() {
    let contents = format!("{}\t{}\t{}", "a".repeat(100), "b".repeat(100), "c".repeat(100));
    let groups = chunks(&contents);
    let lengths: Vec<(i32, usize)> = groups.iter().map(|(c, s)| (*c, s.chars().count())).collect();
    // 302 characters of text: a chunk of 250 (252 written with its two `^I`), then the 52 left.
    assert_eq!(lengths, [(3, 252), (1, 52)], "{groups:?}");
}

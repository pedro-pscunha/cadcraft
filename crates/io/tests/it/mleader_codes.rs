//! The attachment and connection codes of a multileader (the content context's DXF groups 174 to 177, the entity's 173, 95, 174, 175
//! and 176) and of its MLEADERSTYLE (174, 178, 175, 176, 177) survive a save and reopen in DXF and DWG: they are read and written, not
//! replaced by the constants the writer used to emit.

use cadcraft_doc::{Block, Drawing, Entity, EntityKind, Handle, Insert, Line, MLeader, MLeaderStyle, MText};
use cadcraft_dxf::{Version, records, sections};
use cadcraft_geom::Vec3;
use cadcraft_io::{SaveOptions, read, read_dxf, write, write_with};

fn drawing(block: bool) -> Drawing {
    let mut d = Drawing::new_metric();
    let text = MText {
        insert: Vec3::new(12.0, 20.0, 0.0),
        height: 0.4,
        width: 10.0,
        attach: 5,
        rotation: 0.0,
        style: "Standard".into(),
        contents: "x".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: None,
    };
    let mut m = MLeader::single(
        Vec3::new(10.0, 20.0, 0.0),
        2.0,
        false,
        vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 6.0, 0.0)]],
        (!block).then_some(text),
    );
    if block {
        let mut definition = Block::new("B");
        definition.entities.push(Entity::new(Handle(0x100), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })));
        d.blocks.insert("B".into(), std::sync::Arc::new(definition));
        m.block = Some(Insert {
            block: "B".into(),
            insert: Vec3::new(11.0, 20.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        });
    }
    let h = d.new_handle();
    d.model.push(Entity::new(h, EntityKind::MLeader(m)));
    d
}

fn dxf(d: &Drawing) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap()
}

/// The text of `dxf(&drawing(block))` with the content context's 174 to 177 and the entity's 176 set to other values: what a file
/// from AutoCAD with centre-extents connection and other attachments has.
fn source(block: bool) -> String {
    let out = dxf(&drawing(block));
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let (mut in_ml, mut in_ctx, mut i) = (false, false, 0);
    while i + 1 < lines.len() {
        let (code, value) = (lines[i].trim().to_string(), lines[i + 1].trim().to_string());
        if code == "0" {
            (in_ml, in_ctx) = (value == "MULTILEADER", false);
        }
        in_ctx = (in_ml && in_ctx && code != "301") || (in_ml && code == "300" && value.starts_with("CONTEXT_DATA"));
        let set = |lines: &mut Vec<String>, v: &str| lines[i + 1] = v.to_string();
        match (in_ml, in_ctx, code.as_str()) {
            (true, true, "174") => set(&mut lines, "3"),
            (true, true, "175") => set(&mut lines, "5"),
            (true, true, "176") => set(&mut lines, "2"),
            (true, true, "177") => set(&mut lines, "0"),
            (true, false, "176") => set(&mut lines, "0"),
            (true, false, "173") => set(&mut lines, "1"),
            (true, false, "95") => set(&mut lines, "3"),
            _ => {}
        }
        i += 2;
    }
    lines.join("\n") + "\n"
}

fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES" || s.name == "OBJECTS")
        .flat_map(|s| {
            records(&s.tags).into_iter().filter(|(k, _)| k == kind).flat_map(|(_, t)| t.into_iter().map(|t| (t.code, t.str()))).collect::<Vec<_>>()
        })
        .collect()
}

/// The codes of one record, by group code, in file order.
fn codes(g: &[(i32, String)], wanted: &[i32]) -> Vec<(i32, String)> {
    g.iter().filter(|(c, _)| wanted.contains(c)).cloned().collect()
}

#[test]
fn the_codes_of_a_text_multileader_survive_a_dxf_save() {
    let src = source(false);
    let saved = dxf(&read_dxf(src.as_bytes()).unwrap());
    let wanted = [95, 173, 174, 175, 176, 177];
    let (before, after) = (codes(&groups(&src, "MULTILEADER"), &wanted), codes(&groups(&saved, "MULTILEADER"), &wanted));
    assert!(before.contains(&(177, "0".into())) && before.contains(&(174, "3".into())), "{before:?}");
    assert_eq!(before, after);
}

#[test]
fn the_connection_of_a_block_multileader_survives_a_dxf_save() {
    let src = source(true);
    let saved = dxf(&read_dxf(src.as_bytes()).unwrap());
    let wanted = [95, 173, 174, 175, 176, 177];
    let (before, after) = (codes(&groups(&src, "MULTILEADER"), &wanted), codes(&groups(&saved, "MULTILEADER"), &wanted));
    // The entity's block attachment (176 outside the context) is 0 = centre extents.
    assert!(before.iter().filter(|c| **c == (176, "0".into())).count() == 1 && before.contains(&(177, "0".into())), "{before:?}");
    assert_eq!(before, after);
}

#[test]
fn a_changed_text_attachment_derives_the_codes_again_but_the_connection_stays() {
    let mut d = read_dxf(source(false).as_bytes()).unwrap();
    let h = d.model.iter().next().unwrap().handle;
    d.modify_entity(h, |e| {
        if let EntityKind::MLeader(m) = &mut e.kind {
            m.text.as_mut().unwrap().attach = 1;
        }
    })
    .unwrap();
    let g = groups(&dxf(&d), "MULTILEADER");
    // Attachment 1 (top left): top of the top line (0) on both sides and column alignment 0; the connection (177) is the file's 0.
    assert_eq!(codes(&g, &[174, 175, 176, 177])[..4], [(174, "0".into()), (175, "0".into()), (176, "0".into()), (177, "0".into())]);
}

fn styled() -> Drawing {
    let mut d = drawing(false);
    d.mleader_styles.push(MLeaderStyle {
        name: "Callout".into(),
        text_left_attachment: 3,
        text_right_attachment: 5,
        text_angle_type: 2,
        text_alignment: 2,
        block_connection: 1,
        ..MLeaderStyle::default()
    });
    d
}

#[test]
fn the_codes_of_a_multileader_style_survive_a_dxf_save() {
    let out = dxf(&styled());
    let back = read_dxf(out.as_bytes()).unwrap();
    let style = back.mleader_styles.iter().find(|s| s.name == "Callout").unwrap();
    assert_eq!(
        (style.text_left_attachment, style.text_right_attachment, style.text_angle_type, style.text_alignment, style.block_connection),
        (3, 5, 2, 2, 1)
    );
}

#[test]
fn the_codes_survive_a_dwg_save() {
    let src = read_dxf(source(false).as_bytes()).unwrap();
    let mut d = src.clone();
    d.mleader_styles = styled().mleader_styles;
    let bytes = write(&d, "x.dwg").unwrap();
    // The DWG itself holds them (acadrust reads the objects).
    let stored = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes.clone())).read().unwrap();
    let leader = stored
        .entities()
        .find_map(|e| if let acadrust::entities::EntityType::MultiLeader(m) = e { Some((**m).clone()) } else { None })
        .expect("a MULTILEADER in the DWG");
    use acadrust::entities::{BlockContentConnectionType, TextAlignmentType, TextAttachmentType};
    assert_eq!(leader.context.block_connection_type, BlockContentConnectionType::BlockExtents);
    assert_eq!(leader.context.text_left_attachment, TextAttachmentType::MiddleOfBottomLine);
    assert_eq!(leader.context.text_right_attachment, TextAttachmentType::BottomLine);
    assert_eq!(leader.context.text_alignment, TextAlignmentType::Right);
    // And reading the DWG back gives the same codes in the model and in the next save.
    let back = read(&bytes, "x.dwg").unwrap();
    let EntityKind::MLeader(m) = &back.model.iter().next().unwrap().kind else { panic!("a multileader") };
    let c = m.context.codes;
    assert_eq!((c.ctx_left, c.ctx_right, c.ctx_alignment, c.ctx_connection), (Some(3), Some(5), Some(2), Some(0)), "{c:?}");
    let style = back.mleader_styles.iter().find(|s| s.name == "Callout").expect("the style");
    assert_eq!(
        (style.text_left_attachment, style.text_right_attachment, style.text_angle_type, style.text_alignment, style.block_connection),
        (3, 5, 2, 2, 1)
    );
}

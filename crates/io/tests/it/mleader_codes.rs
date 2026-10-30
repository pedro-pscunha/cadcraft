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

/// A style with every field at a value that is not the default.
fn full_style() -> MLeaderStyle {
    // The colour method is the top byte: 0xC0 ByLayer, 0xC1 ByBlock, 0xC2 a true colour (0xC2RRGGBB), 0xC3 an index (0xC3000005).
    let true_color = |rgb: u32| (0xC200_0000u32 | rgb) as i32 as i64;
    MLeaderStyle {
        name: "Full".into(),
        arrow_size: 0.3,
        text_height: 0.5,
        landing_gap: 0.2,
        dogleg: 0.7,
        text_style: "Slanted".into(),
        arrow_block: "_DOT".into(),
        annotative: true,
        text_left_attachment: 3,
        text_right_attachment: 5,
        text_angle_type: 2,
        text_alignment: 2,
        block_connection: 1,
        version: 2,
        content_type: 1,
        draw_mleader_order: 0,
        draw_leader_order: 1,
        max_leader_points: 4,
        first_segment_angle: 0.5,
        second_segment_angle: 0.75,
        leader_line_type: 2,
        line_color: true_color(0x102030),
        line_type: "DASHED".into(),
        line_weight: 50,
        enable_landing: false,
        enable_dogleg: false,
        description: "A description".into(),
        default_text: "Default".into(),
        text_color: (0xC300_0005u32) as i32 as i64,
        text_frame: true,
        text_always_left: true,
        align_space: 3.5,
        block: "B".into(),
        block_color: true_color(0x708090),
        block_scale: [2.0, 3.0, 4.0],
        enable_block_scale: false,
        block_rotation: 0.5,
        enable_block_rotation: false,
        scale: 2.5,
        overwritten: true,
        break_gap: 0.25,
        attach_direction: 1,
        attach_top: 4,
        attach_bottom: 6,
    }
}

fn drawing_with_full_style() -> Drawing {
    let mut d = drawing(true);
    d.text_styles.push(cadcraft_doc::TextStyle { name: "Slanted".into(), oblique: 0.2, ..cadcraft_doc::TextStyle::default() });
    d.linetypes.push(cadcraft_doc::Linetype::simple("DASHED", "dashes", &[0.5, -0.25]));
    d.mleader_styles.push(full_style());
    d
}

fn style_of(d: &Drawing) -> MLeaderStyle {
    d.mleader_styles.iter().find(|s| s.name == "Full").cloned().expect("the style")
}

#[test]
fn every_field_of_a_multileader_style_survives_a_dxf_save() {
    let d = drawing_with_full_style();
    let out = dxf(&d);
    let back = read_dxf(out.as_bytes()).unwrap();
    assert_eq!(style_of(&back), full_style());
    // The second save writes the same object.
    let without_handles = |g: Vec<(i32, String)>| g.into_iter().filter(|(c, _)| !matches!(*c, 5 | 330 | 340 | 341 | 342 | 343)).collect::<Vec<_>>();
    assert_eq!(without_handles(groups(&dxf(&back), "MLEADERSTYLE")), without_handles(groups(&out, "MLEADERSTYLE")));
}

#[test]
fn a_style_made_in_cadcraft_is_written_with_the_defaults() {
    let mut d = drawing(false);
    d.mleader_styles.push(MLeaderStyle { name: "Plain".into(), ..MLeaderStyle::default() });
    let g = groups(&dxf(&d), "MLEADERSTYLE");
    let value = |code: i32| g.iter().filter(|(c, _)| *c == code).map(|(_, v)| v.clone()).collect::<Vec<_>>();
    // The constants this writer always emitted.
    for (code, want) in
        [(179, "2"), (170, "2"), (171, "1"), (172, "0"), (90, "2"), (173, "1"), (91, "-1056964608"), (92, "-2"), (290, "1"), (291, "1")]
    {
        assert!(value(code).contains(&want.to_string()), "{code}: {:?}", value(code));
    }
}

/// The style of the DWG `bytes`, field by field as acadrust reads it: the fields its DXF reader takes.
fn assert_acadrust_fields(bytes: &[u8], want: &MLeaderStyle) {
    use acadrust::objects::{BlockContentConnectionType, LeaderContentType, ObjectType, TextAlignmentType, TextAngleType, TextAttachmentType};
    let stored = acadrust::DwgReader::from_stream(std::io::Cursor::new(bytes.to_vec())).read().unwrap();
    let style = stored
        .objects
        .values()
        .find_map(|o| {
            if let ObjectType::MultiLeaderStyle(s) = o {
                (s.text_left_attachment == TextAttachmentType::MiddleOfBottomLine).then(|| s.clone())
            } else {
                None
            }
        })
        .expect("the style in the DWG");
    assert_eq!(style.content_type, LeaderContentType::Block);
    assert_eq!(style.block_content_connection, BlockContentConnectionType::BasePoint);
    assert_eq!(
        (style.text_right_attachment, style.text_angle_type, style.text_alignment),
        (TextAttachmentType::BottomLine, TextAngleType::from(2), TextAlignmentType::Right)
    );
    assert!(
        !style.enable_landing
            && !style.enable_dogleg
            && style.text_frame
            && !style.enable_block_scale
            && !style.enable_block_rotation
            && style.is_annotative
    );
    assert_eq!(
        (style.scale_factor, style.landing_gap, style.landing_distance, style.arrowhead_size, style.text_height),
        (2.5, want.landing_gap, want.dogleg, want.arrow_size, want.text_height)
    );
    // The groups acadrust's DXF reader used to drop or mislabel.
    assert_eq!((style.description.as_str(), style.default_text.as_str()), ("A description", "Default"));
    assert_eq!((style.max_leader_points, style.first_segment_angle, style.second_segment_angle), (4, 0.5, 0.75));
    assert!(style.text_always_left);
    assert_eq!(style.line_color, acadrust::types::Color::Rgb { r: 0x10, g: 0x20, b: 0x30 });
    assert_eq!(style.text_color, acadrust::types::Color::Index(5));
    assert_eq!(style.block_content_color, acadrust::types::Color::Rgb { r: 0x70, g: 0x80, b: 0x90 });
    assert_eq!((style.align_space, style.break_gap_size, style.block_content_rotation), (3.5, 0.25, 0.5));
    assert_eq!((style.block_content_scale_x, style.block_content_scale_y, style.block_content_scale_z), (2.0, 3.0, 4.0));
}

#[test]
fn every_field_of_a_multileader_style_survives_a_dwg_save() {
    let bytes = write(&drawing_with_full_style(), "x.dwg").unwrap();
    let want = full_style();
    assert_acadrust_fields(&bytes, &want);
    // CADCraft reads the DWG back to the same style, field for field, the name from the dictionary key.
    let read_back = read(&bytes, "x.dwg").unwrap();
    assert_eq!(style_of(&read_back), want);
    assert_eq!(read_back.mleader_styles.iter().filter(|s| s.name == "Full").count(), 1);
}

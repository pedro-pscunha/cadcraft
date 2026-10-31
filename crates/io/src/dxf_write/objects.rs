use super::{Drawing, Group, MLeaderStyle, TableStyle, W, dxf_ext};

/// A TABLESTYLE object (DXF Reference, OBJECTS: TABLESTYLE): margins, title/header
/// suppression and per row type (data, column header, title) text and border settings.
/// A table style's name is its dictionary key, and group 3 is its description (the text field of the DWG object, which acadrust
/// calls the style's name). The border linetypes and override flags are written for the bridge only (`bridge`, given with the
/// handles of the written linetypes by upper-case name).
pub(super) fn table_style_obj(
    w: &mut W,
    d: &Drawing,
    s: &TableStyle,
    h: &str,
    dict: &str,
    bridge: Option<&std::collections::HashMap<String, String>>,
) {
    w.s(0, "TABLESTYLE");
    w.s(5, h);
    w.group("ACAD_REACTORS", 330, &[dict]);
    w.s(330, dict);
    w.s(100, "AcDbTableStyle");
    // The version of the object (0), as AutoCAD writes it first.
    w.i(280, 0);
    w.s(3, &s.description);
    w.i(70, i64::from(s.flow_up));
    // The flags word as read; its suppression bits (1 title, 2 header) follow the style's switches, which are all a style made
    // here has set.
    w.i(71, i64::from(s.flags & !3 | i16::from(!s.title) | i16::from(!s.header) << 1));
    w.f(40, s.margin);
    // The vertical margin is the Data cell style's (the cell styles read it from group 41).
    w.f(41, s.cell_style(crate::dxf_table::CELL_TYPES[0]).margin_v);
    w.i(280, i64::from(!s.title));
    w.i(281, i64::from(!s.header));
    // Data, column header and title cell styles; CADCraft's complete cell styles in xdata.
    for kind in crate::dxf_table::CELL_TYPES {
        let cs = s.cell_style(kind);
        let style = if d.text_style(&cs.text_style).is_some() { cs.text_style.clone() } else { "Standard".to_string() };
        w.t.extend(crate::dxf_table::cell_type_tags(&cs, &style, bridge));
    }
    let x = crate::dxf_table::cell_styles_xdata(s);
    if !x.is_empty() {
        w.s(1001, dxf_ext::APP);
        for c in x {
            w.s(1000, c);
        }
    }
}

/// A GROUP object (DXF Reference, OBJECTS: GROUP): description, unnamed (`*A…`) and selectable
/// flags, and the member entities that were written to this file.
pub(super) fn group_obj(w: &mut W, g: &Group, h: &str, dict: &str, written: &std::collections::HashSet<String>) {
    w.s(0, "GROUP");
    w.s(5, h);
    w.group("ACAD_REACTORS", 330, &[dict]);
    w.s(330, dict);
    w.s(100, "AcDbGroup");
    w.s(300, &g.description);
    w.i(70, i64::from(g.name.starts_with('*')));
    w.i(71, i64::from(g.selectable));
    for m in g.members.iter().map(|m| m.hex()).filter(|m| written.contains(m)) {
        w.s(340, m);
    }
}

/// A MLEADERSTYLE object (DXF Reference, OBJECTS: MLEADERSTYLE): MText content and straight
/// The handles an MLEADERSTYLE names: its text style, its arrowhead block, its line's linetype and its content block (none: the
/// group is left out).
pub(super) struct StyleRefs {
    pub text_style: Option<String>,
    pub arrow: Option<String>,
    pub line_type: Option<String>,
    pub block: Option<String>,
}

/// A MLEADERSTYLE object (DXF Reference, OBJECTS: MLEADERSTYLE): every field the model has, the ones a file gave as it gave
/// them, a style made in CADCraft with the defaults of [`MLeaderStyle::default`].
pub(super) fn mleader_style_obj(w: &mut W, s: &MLeaderStyle, h: &str, dict: &str, refs: &StyleRefs) {
    w.s(0, "MLEADERSTYLE");
    w.s(5, h);
    w.group("ACAD_REACTORS", 330, &[dict]);
    w.s(330, dict);
    w.s(100, "AcDbMLeaderStyle");
    w.i(179, s.version);
    w.i(170, s.content_type);
    w.i(171, s.draw_mleader_order);
    w.i(172, s.draw_leader_order);
    w.i(90, s.max_leader_points);
    w.f(40, s.first_segment_angle);
    w.f(41, s.second_segment_angle);
    w.i(173, s.leader_line_type);
    w.i(91, s.line_color);
    if let Some(lt) = &refs.line_type {
        w.s(340, lt);
    }
    w.i(92, s.line_weight);
    w.i(290, i64::from(s.enable_landing));
    w.f(42, s.landing_gap);
    w.i(291, i64::from(s.enable_dogleg));
    w.f(43, s.dogleg);
    w.s(3, &s.description);
    if let Some(a) = &refs.arrow {
        w.s(341, a);
    }
    w.f(44, s.arrow_size);
    w.s(300, &s.default_text);
    if let Some(ts) = &refs.text_style {
        w.s(342, ts);
    }
    w.i(174, s.text_left_attachment);
    w.i(178, s.text_right_attachment);
    w.i(175, s.text_angle_type);
    w.i(176, s.text_alignment);
    w.i(93, s.text_color);
    w.f(45, s.text_height);
    w.i(292, i64::from(s.text_frame));
    w.i(297, i64::from(s.text_always_left));
    w.f(46, s.align_space);
    if let Some(b) = &refs.block {
        w.s(343, b);
    }
    w.i(94, s.block_color);
    for (c, v) in [47, 49, 140].into_iter().zip(s.block_scale) {
        w.f(c, v);
    }
    w.i(293, i64::from(s.enable_block_scale));
    w.f(141, s.block_rotation);
    w.i(294, i64::from(s.enable_block_rotation));
    w.i(177, s.block_connection);
    w.f(142, s.scale);
    w.i(295, i64::from(s.overwritten));
    w.i(296, i64::from(s.annotative));
    w.f(143, s.break_gap);
    w.i(271, s.attach_direction);
    w.i(272, s.attach_top);
    w.i(273, s.attach_bottom);
}

/// The key of each scale in the scale list: the one it had in its file, else the next free key of
/// AutoCAD's own series (`A0`..`A9`, `B0`..`B9`, and so on). A key the list already uses is not given twice.
fn scale_keys(scales: &[(&cadcraft_doc::AnnoScale, String)]) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    let mut keys: Vec<Option<String>> =
        scales.iter().map(|(s, _)| Some(s.key.clone()).filter(|k| !k.is_empty() && used.insert(k.to_lowercase()))).collect();
    // ponytail: past 260 scales the letter runs out of the alphabet and the key is `S<n>`.
    let mut series = (0usize..).map(|n| if n < 260 { format!("{}{}", char::from(b'A' + (n / 10) as u8), n % 10) } else { format!("S{n}") });
    keys.iter_mut().map(|key| key.take().unwrap_or_else(|| series.by_ref().find(|k| used.insert(k.to_lowercase())).unwrap_or_default())).collect()
}

/// The ACAD_SCALELIST dictionary and its SCALE objects (DXF Reference, OBJECTS section): name
/// (300), paper units (140), drawing units (141), unit scale flag (290).
pub(super) fn scale_list(w: &mut W, scales: &[(&cadcraft_doc::AnnoScale, String)], dict: &str, root: &str) {
    w.s(0, "DICTIONARY");
    w.s(5, dict);
    w.group("ACAD_REACTORS", 330, &[root]);
    w.s(330, root);
    w.s(100, "AcDbDictionary");
    w.i(281, 1);
    for (key, (_, h)) in scale_keys(scales).into_iter().zip(scales) {
        w.s(3, key);
        w.s(350, h.clone());
    }
    for (s, h) in scales {
        w.s(0, "SCALE");
        w.s(5, h.clone());
        w.group("ACAD_REACTORS", 330, &[dict]);
        w.s(330, dict);
        w.s(100, "AcDbScale");
        w.i(70, 0);
        w.s(300, dxf_ext::xdata_str(&s.name));
        w.f(140, s.paper_units);
        w.f(141, s.drawing_units);
        w.i(290, i64::from(s.is_unit_scale()));
    }
}

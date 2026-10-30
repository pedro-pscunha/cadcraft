//! File-format follow-ups: native old-target MULTILEADER, versioned DIMSTYLE fields, code pages and dictionaries.

use cadcraft_doc::*;
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_io::{SaveOptions, Version};

fn save(d: &Drawing, name: &str, v: Version) -> Vec<u8> {
    cadcraft_io::write_with(d, name, &SaveOptions { version: Some(v), binary: false }).unwrap()
}

fn dxf(d: &Drawing, v: Version) -> String {
    String::from_utf8(save(d, "x.dxf", v)).unwrap()
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).collect()
}

/// Number of records of `kind` (`0` / `kind` pairs) in DXF text.
fn records(text: &str, kind: &str) -> usize {
    lines(text).chunks(2).filter(|p| *p == ["0", kind]).count()
}

fn count(d: &Drawing, f: impl Fn(&EntityKind) -> bool) -> usize {
    d.model.iter().filter(|e| f(&e.kind)).count()
}

fn mleader_drawing() -> (Drawing, MLeader) {
    let mut d = Drawing::new_metric();
    let m = MLeader {
        roots: vec![cadcraft_doc::MLeaderRoot::new(
            Vec3::new(5.0, 4.0, 0.0),
            0.75,
            false,
            vec![vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 3.0, 0.0)], vec![Vec3::new(1.0, -2.0, 0.0)]],
        )],
        text: Some(MText {
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
        }),
        style: "Standard".into(),
        arrow_size: 1.0,
        spline: false,
        block: None,
        arrow_block: String::new(),
        context: cadcraft_doc::MLeaderContext::default(),
    };
    d.add(&Space::Model, Common { layer: "0".into(), ..Common::default() }, EntityKind::MLeader(m.clone())).unwrap();
    (d, m)
}

#[test]
fn multileader_before_2007_stays_native_with_the_autocad_version_marker() {
    let (d, _) = mleader_drawing();
    for v in [Version::R2000, Version::R2004] {
        let text = dxf(&d, v);
        assert_eq!(records(&text, "MULTILEADER"), 1, "{v:?}");
        assert_eq!(records(&text, "LEADER"), 0, "{v:?}");
        assert_eq!(records(&text, "MTEXT"), 0, "{v:?}");
        assert!(lines(&text).windows(4).any(|g| g == ["1001", "ACAD_MLEADERVER", "1070", "2"]), "{v:?}");
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_eq!(count(&back, |k| matches!(k, EntityKind::MLeader(m) if m.text.as_ref().is_some_and(|t|t.contents=="NOTE"))), 1);
    }
}

#[test]
fn multileader_from_2007_is_native() {
    let (d, m) = mleader_drawing();
    for v in [Version::R2007, Version::R2018] {
        let text = dxf(&d, v);
        assert_eq!(records(&text, "MULTILEADER"), 1, "{v:?}");
        assert_eq!(records(&text, "LEADER"), 0, "{v:?}");
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        let got: Vec<&MLeader> = back.model.iter().filter_map(|e| if let EntityKind::MLeader(m) = &e.kind { Some(m) } else { None }).collect();
        assert_eq!(got, vec![&with_codes_of(&m, got[0])], "{v:?}");
    }
}

#[test]
fn multileader_in_dwg_follows_the_dwg_version() {
    let (d, _) = mleader_drawing();
    for version in [Version::R2000, Version::R2004] {
        let old = cadcraft_io::read(&save(&d, "x.dwg", version), "x.dwg").unwrap();
        assert_eq!(count(&old, |k| matches!(k, EntityKind::MLeader(_))), 1, "{version:?} DWG keeps MULTILEADER");
        assert_eq!(count(&old, |k| matches!(k, EntityKind::Leader(_))), 0);
        assert_eq!(count(&old, |k| matches!(k, EntityKind::MText(_))), 0);
    }
    let new = cadcraft_io::read(&save(&d, "x.dwg", Version::R2018), "x.dwg").unwrap();
    assert_eq!(count(&new, |k| matches!(k, EntityKind::Leader(_))), 0, "2018 DWG keeps the multileader whole");
}

fn dim_drawing() -> (Drawing, DimStyle) {
    let mut d = Drawing::new_imperial();
    d.linetypes.push(Linetype::simple("DASHED", "Dashed", &[0.5, -0.25]));
    d.linetypes.push(Linetype::simple("CENTER", "Center", &[1.25, -0.25, 0.25, -0.25]));
    let st = DimStyle {
        name: "Detail".into(),
        arc_symbol: 1,
        dim_linetype: "DASHED".into(),
        ext_linetype1: "CENTER".into(),
        ext_linetype2: "DASHED".into(),
        fixed_ext: true,
        fixed_ext_length: 0.75,
        jog_angle: 30.0,
        ..DimStyle::default()
    };
    d.dim_styles.push(st.clone());
    (d, st)
}

/// The DIMSTYLE record named `name`.
fn dimstyle_record<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let l = lines(text);
    let at = l.windows(2).position(|w| w == ["2", name]).unwrap();
    let end = l[at..].windows(2).position(|w| w == ["0", "DIMSTYLE"] || w == ["0", "ENDTAB"]).map_or(l.len(), |e| at + e);
    l[at..end].to_vec()
}

#[test]
fn newer_dimstyle_variables_are_table_groups_from_2004() {
    let (d, st) = dim_drawing();
    let r2000 = dxf(&d, Version::R2000);
    let rec = dimstyle_record(&r2000, "Detail");
    assert!(rec.windows(4).any(|w| w == ["1001", "CADCRAFT", "1000", "DIMVARS"]), "R2000: xdata");
    assert!(!rec.chunks(2).any(|p| p[0] == "345"), "R2000: no DIMLTYPE group");
    for v in [Version::R2004, Version::R2018] {
        let text = dxf(&d, v);
        let rec = dimstyle_record(&text, "Detail");
        assert!(!rec.contains(&"DIMVARS"), "{v:?}: no DIMVARS xdata");
        for code in ["49", "50", "90", "290", "345", "346", "347"] {
            assert!(rec.chunks(2).any(|p| p[0] == code), "{v:?}: group {code}");
        }
        let jog = rec.chunks(2).find(|p| p[0] == "50").unwrap()[1].parse::<f64>().unwrap();
        assert!((jog - 30.0).abs() < 1e-9, "DIMJOGANG in degrees: {jog}");
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.dim_style("Detail").unwrap(), &st, "{v:?}");
    }
    assert_eq!(cadcraft_io::read_dxf(r2000.as_bytes()).unwrap().dim_style("Detail").unwrap(), &st);
}

fn code_page(text: &str) -> &str {
    let l = lines(text);
    let at = l.iter().position(|s| *s == "$DWGCODEPAGE").unwrap();
    l[at + 2]
}

#[test]
fn code_page_is_kept_before_2007() {
    let mut d = Drawing::new_metric();
    d.header.set_str("DWGCODEPAGE", "ANSI_1251");
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Text(Text {
            insert: Vec3::ZERO,
            align_pt: None,
            height: 2.5,
            value: "Привет".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Left,
            valign: VAlign::Baseline,
            backward: false,
            upside_down: false,
        }),
    )
    .unwrap();
    for v in [Version::R2000, Version::R2004] {
        let bytes = save(&d, "x.dxf", v);
        assert!(bytes.is_ascii(), "{v:?}: ASCII with escapes");
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(code_page(&text), "ANSI_1251", "{v:?}");
        let back = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.header.str("DWGCODEPAGE", ""), "ANSI_1251");
        assert!(back.model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "Привет")));
    }
    assert_eq!(code_page(&dxf(&d, Version::R2018)), "ANSI_1252", "2007+: UTF-8 text");
    // Without one, or with one CADCraft doesn't know: ANSI_1252.
    d.header.set_str("DWGCODEPAGE", "NO_SUCH\n0\nEOF");
    assert_eq!(code_page(&dxf(&d, Version::R2000)), "ANSI_1252");
    assert_eq!(code_page(&dxf(&Drawing::new_metric(), Version::R2000)), "ANSI_1252");
}

#[test]
fn modelled_dictionaries_are_not_kept_twice() {
    let mut d = Drawing::new_metric();
    let wall = MLineStyle { name: "Wall".into(), ..MLineStyle::default() };
    d.mline_styles.push(wall.clone());
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::MLine(MLine::new(&wall, &[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)], false, MLineJust::Zero, 1.0)),
    )
    .unwrap();
    let first = dxf(&d, Version::R2018);
    let read = cadcraft_io::read_dxf(first.as_bytes()).unwrap();
    let kept: Vec<&str> = read.foreign.objects.iter().map(|o| o.kind.as_str()).collect();
    assert!(kept.is_empty(), "nothing CADCraft writes itself is kept raw: {kept:?}");
    let again = dxf(&read, Version::R2018);
    let third = dxf(&cadcraft_io::read_dxf(again.as_bytes()).unwrap(), Version::R2018);
    for kind in ["MLINESTYLE", "SCALE", "MLEADERSTYLE", "TABLESTYLE", "DICTIONARY", "XRECORD", "GROUP", "LAYOUT"] {
        assert_eq!(records(&again, kind), records(&first, kind), "{kind} after one round trip");
        assert_eq!(records(&third, kind), records(&first, kind), "{kind} after two");
    }
    let l = lines(&again);
    for key in ["ACAD_MLINESTYLE", "ACAD_SCALELIST", "ACAD_GROUP", "ACAD_MLEADERSTYLE", "ACAD_PLOTSTYLENAME", "CADCRAFT_HANDSEED"] {
        assert_eq!(l.windows(2).filter(|w| *w == ["3", key]).count(), 1, "{key} once in the named object dictionary");
    }
}

/// `m` with the attachment and connection codes a file gave `read` (the writer's own are derived and read back as codes).
fn with_codes_of(m: &cadcraft_doc::MLeader, read: &cadcraft_doc::MLeader) -> cadcraft_doc::MLeader {
    cadcraft_doc::MLeader { context: cadcraft_doc::MLeaderContext { codes: read.context.codes.clone(), ..m.context.clone() }, ..m.clone() }
}

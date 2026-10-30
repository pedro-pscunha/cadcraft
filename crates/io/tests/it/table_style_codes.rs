//! The groups of a TABLESTYLE (description 3, flow direction 70, flags 71, vertical margin 41) are read and written back as read, in
//! DXF and in DWG. A style made here keeps the groups the writer emitted before.

use std::io::Cursor;

use acadrust::objects::{ObjectType, TableFlowDirection, TableStyle as RStyle, TableStyleFlags};
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReader, DwgWriter};
use cadcraft_doc::{CellStyle, Drawing, TableStyle};
use cadcraft_dxf::{Version, records, sections};
use cadcraft_io::{SaveOptions, read, read_dxf, write_with};

pub(super) fn dxf(d: &Drawing) -> String {
    String::from_utf8(write_with(d, "x.dxf", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()).unwrap()
}

/// The groups (code, text) of the first record of `kind` in `text`, in file order.
pub(super) fn groups(text: &str, kind: &str) -> Vec<(i32, String)> {
    groups_with(text, kind, |_| true)
}

/// The groups of the first record of `kind` in `text` for which `pick` holds.
pub(super) fn groups_with(text: &str, kind: &str, pick: impl Fn(&[(i32, String)]) -> bool) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES" || s.name == "OBJECTS")
        .flat_map(|s| records(&s.tags).into_iter().filter(|(k, _)| k == kind).map(|(_, t)| t).collect::<Vec<_>>())
        .map(|t| t.into_iter().map(|t| (t.code, t.str().trim().to_string())).collect::<Vec<_>>())
        .find(|g| pick(g))
        .unwrap_or_default()
}

pub(super) fn at(g: &[(i32, String)], code: i32) -> Vec<String> {
    g.iter().filter(|(c, _)| *c == code).map(|(_, v)| v.clone()).collect()
}

fn plan() -> TableStyle {
    let mut st =
        TableStyle { name: "Plan".into(), margin: 0.07, description: "Parts list".into(), flow_up: true, flags: 4 | 16, ..TableStyle::default() };
    // The vertical margin (41) is the cell styles': it differs from the horizontal one (40).
    for kind in [CellStyle::DATA, CellStyle::TITLE, CellStyle::HEADER] {
        let mut cs = st.cell_style(kind);
        cs.margin_v = 0.2;
        st.set_cell_style(cs);
    }
    st
}

#[test]
fn a_table_style_keeps_its_description_flow_flags_and_vertical_margin_in_dxf() {
    let mut d = Drawing::new_imperial();
    d.table_styles.push(plan());
    let text = dxf(&d);
    let plan_group = |g: &[(i32, String)]| g.contains(&(3, "Parts list".to_string()));
    let g = groups_with(&text, "TABLESTYLE", plan_group);
    assert!(!g.is_empty(), "no TABLESTYLE with the description");
    assert_eq!(
        (at(&g, 70), at(&g, 71), at(&g, 40), at(&g, 41)),
        (["1".into()].to_vec(), ["20".into()].to_vec(), ["0.07".into()].to_vec(), ["0.2".into()].to_vec())
    );
    let back = read_dxf(text.as_bytes()).unwrap();
    assert_eq!(back.table_styles.iter().find(|s| s.name == "Plan"), d.table_styles.iter().find(|s| s.name == "Plan"));
    // The second save writes the same groups.
    assert_eq!(groups_with(&dxf(&back), "TABLESTYLE", plan_group), g);
}

#[test]
fn a_suppressed_title_sets_its_bit_of_the_flags_and_a_style_made_here_keeps_the_old_groups() {
    let mut d = Drawing::new_imperial();
    d.table_styles.push(TableStyle { name: "Bare".into(), title: false, header: false, flags: 4, ..TableStyle::default() });
    d.table_styles.push(TableStyle { name: "Made".into(), margin: 0.1, ..TableStyle::default() });
    let text = dxf(&d);
    let styles: Vec<Vec<(i32, String)>> = {
        let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
        let objects = sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap();
        records(&objects.tags)
            .into_iter()
            .filter(|(k, _)| k == "TABLESTYLE")
            .map(|(_, t)| t.into_iter().map(|t| (t.code, t.str().trim().to_string())).collect())
            .collect()
    };
    let flags: Vec<String> = styles.iter().flat_map(|g| at(g, 71)).collect();
    assert!(flags.contains(&"7".to_string()), "4 | title | header: {flags:?}");
    let made = styles.iter().find(|g| at(g, 40) == ["0.1".to_string()]).unwrap();
    assert_eq!(
        (at(made, 70), at(made, 71), at(made, 41), at(made, 3)),
        (["0".into()].to_vec(), ["0".into()].to_vec(), ["0.1".into()].to_vec(), [String::new()].to_vec())
    );
    let back = read_dxf(text.as_bytes()).unwrap();
    let bare = back.table_styles.iter().find(|s| s.name == "Bare").unwrap();
    assert_eq!((bare.flags, bare.title, bare.header), (4, false, false));
}

/// The AutoCAD-side TABLESTYLE: the description is the text field of the DWG object, the name the dictionary key.
fn style_doc(version: DxfVersion, description: &str) -> CadDocument {
    let mut doc = CadDocument::with_version(version);
    let standard = doc.objects.iter().find_map(|(h, o)| matches!(o, ObjectType::TableStyle(_)).then_some(*h)).expect("the Standard table style");
    let dict = doc
        .objects
        .iter()
        .find_map(|(h, o)| match o {
            ObjectType::Dictionary(d) if d.entries.iter().any(|(_, e)| *e == standard) => Some(*h),
            _ => None,
        })
        .expect("the table style dictionary");
    let mut style = RStyle::standard();
    style.handle = doc.allocate_handle();
    style.owner_handle = dict;
    style.name = description.to_string();
    style.flow_direction = TableFlowDirection::Up;
    style.flags = TableStyleFlags::from_bits_retain(4);
    style.horizontal_margin = 0.07;
    style.vertical_margin = 0.2;
    let handle = style.handle;
    doc.objects.insert(handle, ObjectType::TableStyle(style));
    let Some(ObjectType::Dictionary(d)) = doc.objects.get_mut(&dict) else { panic!("no table style dictionary") };
    d.add_entry("Plan", handle);
    doc
}

pub(super) fn read_dwg(bytes: &[u8]) -> CadDocument {
    DwgReader::from_stream(Cursor::new(bytes.to_vec())).read().expect("acadrust reads the DWG")
}

/// The DWG as acadrust writes it, after a CADCraft save as `version`, and after a save of that save.
pub(super) fn dwg_saves(doc: &CadDocument, version: Version) -> [(&'static str, CadDocument); 3] {
    let opts = SaveOptions { version: Some(version), binary: false };
    let save = |dwg: &[u8]| write_with(&read(dwg, "x.dwg").expect("cadcraft read"), "x.dwg", &opts).expect("cadcraft save");
    let source = DwgWriter::write_to_vec(doc).expect("write dwg");
    let first = save(&source);
    let second = save(&first);
    [("source", read_dwg(&source)), ("first save", read_dwg(&first)), ("second save", read_dwg(&second))]
}

fn plan_of(doc: &CadDocument) -> RStyle {
    doc.objects
        .values()
        .find_map(|o| match o {
            ObjectType::TableStyle(s) if s.name != "Standard" => Some(s.clone()),
            _ => None,
        })
        .expect("a table style besides Standard")
}

/// What a DWG of `version` holds of the style `Plan`: description, flow direction, flags, margins.
fn style_fields(doc: &CadDocument) -> (String, TableFlowDirection, i16, f64, f64) {
    let s = plan_of(doc);
    (s.name.clone(), s.flow_direction, s.flags.bits() & 4, s.horizontal_margin, s.vertical_margin)
}

#[test]
fn a_table_style_keeps_its_groups_through_a_dwg_save() {
    // Every field survives in a DWG of either layout (from R2010 the flow direction is a bit of the Table cell style).
    for (version, source) in [(Version::R2007, DxfVersion::AC1021), (Version::R2018, DxfVersion::AC1032)] {
        let [(_, held), (_, first), (_, second)] = dwg_saves(&style_doc(source, "Parts list"), version);
        let (held, first, second) = (style_fields(&held), style_fields(&first), style_fields(&second));
        assert_eq!(held, ("Parts list".to_string(), TableFlowDirection::Up, 4, 0.07, 0.2), "{version:?}: the source");
        assert_eq!(first, held, "{version:?}: first save");
        assert_eq!(second, held, "{version:?}: second save");
    }
}

#[test]
fn an_empty_description_stays_empty_through_a_dwg_save() {
    for (version, source) in [(Version::R2007, DxfVersion::AC1021), (Version::R2018, DxfVersion::AC1032)] {
        for (what, back) in dwg_saves(&style_doc(source, ""), version) {
            assert_eq!(plan_of(&back).name, "", "{version:?} {what}");
        }
    }
    // A style made here has no description: the DWG object's text field is empty, as AutoCAD writes it for the Standard style.
    let mut d = Drawing::new_imperial();
    d.table_styles.push(TableStyle { name: "Made".into(), ..TableStyle::default() });
    let dwg = write_with(&d, "x.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    assert_eq!(plan_of(&read_dwg(&dwg)).name, "");
}

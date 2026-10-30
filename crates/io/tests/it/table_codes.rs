//! The groups of an ACAD_TABLE (value flags 90, override masks 93 to 96 with their values, and per cell the flag 91, rotation 145,
//! flag value 172, auto-fit 174, virtual edge 178 and the cell's own style groups) are read and written back as read, in DXF and in
//! DWG. A table made here keeps the groups the writer emitted before.

use acadrust::entities::table::{LegacyBorderOverrides, LegacyTableStyleOverride, Table as RTable};
use acadrust::types::{Color as RColor, DxfVersion, Vector3};
use acadrust::{CadDocument, EntityType};
use cadcraft_doc::{Common, Drawing, EntityKind, RawTag, Space, Table, TableCell, TableOverrides};
use cadcraft_dxf::Version;
use cadcraft_geom::Vec3;
use cadcraft_io::read_dxf;

use super::table_style_codes::{at, dwg_saves, dxf, groups};

fn overridden_cell(rows: usize, cols: usize) -> RTable {
    let mut t = RTable::new(Vector3::new(1.0, 2.0, 0.0), rows, cols);
    for (r, c) in (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))) {
        t.set_cell_text(r, c, "x");
    }
    t.value_flags = 2;
    t.override_flag = true;
    t.legacy_style_override = Some(LegacyTableStyleOverride {
        flags: 0x1 | 0x4 | 0x8,
        title_suppressed: Some(true),
        flow_direction: Some(1),
        horizontal_cell_margin: Some(0.2),
        ..LegacyTableStyleOverride::default()
    });
    t.override_border_color = true;
    t.legacy_border_colors = Some(LegacyBorderOverrides { flags: 3, values: vec![RColor::Index(1), RColor::Index(3)] });
    for r in &mut t.rows {
        for (i, c) in r.cells.iter_mut().enumerate() {
            c.edge_flags = 3;
            c.auto_fit = true;
            c.flag = 5;
            c.virtual_edge = 2;
            c.rotation = 0.5 + i as f64;
            let mut style = acadrust::entities::CellStyle::new();
            style.override_flags = 0x01 | 0x08;
            style.legacy_override_bits = true;
            style.property_flags |= acadrust::entities::table::legacy_override_properties(style.override_flags);
            style.alignment = 5;
            style.content_color = RColor::Index(2);
            c.style = Some(style);
        }
    }
    t
}

fn plan_table() -> Table {
    let cell = |t: &str, i: i32| TableCell {
        text: t.into(),
        flag_value: 3,
        override_flag: 5,
        auto_fit: true,
        virtual_edge: 2,
        rotation: 0.5 + f64::from(i),
        style_groups: [(170, "5"), (64, "2")].into_iter().map(|(code, value)| RawTag { code, value: value.into() }).collect(),
        ..Default::default()
    };
    Table {
        insert: Vec3::new(1.0, 10.0, 0.0),
        col_widths: vec![3.0, 2.0],
        row_heights: vec![0.6, 0.4],
        cells: vec![vec![cell("a", 0), cell("b", 1)], vec![cell("c", 0), cell("d", 1)]],
        style: "Standard".into(),
        text_height: 0.2,
        overrides: TableOverrides {
            value_flags: Some(2),
            style: 0x1 | 0x4 | 0x8,
            border_color: 3,
            border_lineweight: 1,
            border_visibility: 1,
            values: [(280, "1"), (70, "1"), (40, "0.2"), (64, "1"), (65, "3"), (274, "50"), (284, "0")]
                .into_iter()
                .map(|(code, value)| RawTag { code, value: value.into() })
                .collect(),
        },
        ..Table::default()
    }
}

#[test]
fn a_table_keeps_its_masks_values_and_cell_codes_in_dxf() {
    let mut d = Drawing::new_imperial();
    let t = plan_table();
    d.add(&Space::Model, Common::default(), EntityKind::Table(t.clone())).unwrap();
    let text = dxf(&d);
    let g = groups(&text, "ACAD_TABLE");
    assert_eq!(
        (at(&g, 90), at(&g, 93), at(&g, 94), at(&g, 95), at(&g, 96)),
        (["2".into()].to_vec(), ["13".into()].to_vec(), ["3".into()].to_vec(), ["1".into()].to_vec(), ["1".into()].to_vec())
    );
    assert_eq!(at(&g, 145).len(), 4);
    let back = read_dxf(text.as_bytes()).unwrap();
    let first = |d: &Drawing| d.model.iter().find_map(|e| if let EntityKind::Table(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
    assert_eq!(first(&back).overrides, t.overrides);
    assert_eq!(first(&back).cells, t.cells);
    // The second save writes the same codes (the generated handles and the order of the geometry groups can differ).
    let kept = |g: &[(i32, String)]| g.iter().filter(|(c, _)| ![5, 330, 342, 343, 141, 142].contains(c)).cloned().collect::<Vec<_>>();
    assert_eq!(kept(&groups(&dxf(&back), "ACAD_TABLE")), kept(&g), "the second save");
}

#[test]
fn a_table_made_here_keeps_the_old_groups() {
    let mut d = Drawing::new_imperial();
    let t = Table {
        col_widths: vec![3.0],
        row_heights: vec![0.6],
        cells: vec![vec![TableCell { text: "a".into(), ..Default::default() }]],
        ..Table::default()
    };
    d.add(&Space::Model, Common::default(), EntityKind::Table(t)).unwrap();
    let g = groups(&dxf(&d), "ACAD_TABLE");
    for code in [93, 94, 95, 96, 172, 174, 178] {
        assert_eq!(at(&g, code), ["0".to_string()], "group {code}");
    }
    // No title row: 0x06 (has a block) + 0x10 (rows run down) + 0x20 (title suppressed).
    assert_eq!(at(&g, 90), ["54".to_string()], "group 90");
    assert_eq!((at(&g, 171), at(&g, 145)), (["1".into()].to_vec(), ["0.0".into()].to_vec()));
}

fn table_of(doc: &CadDocument) -> RTable {
    doc.entities()
        .find_map(|e| match e {
            EntityType::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("a table")
}

/// What a DWG holds of the table: 90, the style and border color override masks with their values, and per cell 172, 174, 91, 178, 145.
type TableFields =
    (i32, Option<(i32, Option<bool>, Option<i16>, Option<f64>)>, Option<(i32, Vec<RColor>)>, Vec<(u8, bool, i32, i16, f64, Option<(i32, RColor)>)>);

fn table_fields(doc: &CadDocument) -> TableFields {
    let t = table_of(doc);
    (
        t.value_flags,
        t.legacy_style_override.as_ref().map(|o| (o.flags, o.title_suppressed, o.flow_direction, o.horizontal_cell_margin)),
        t.legacy_border_colors.as_ref().map(|o| (o.flags, o.values.clone())),
        t.rows
            .iter()
            .flat_map(|r| r.cells.iter())
            .map(|c| (c.edge_flags, c.auto_fit, c.flag, c.virtual_edge, c.rotation, c.style.as_ref().map(|s| (s.alignment, s.content_color))))
            .collect(),
    )
}

#[test]
fn a_table_keeps_its_masks_values_and_cell_codes_through_a_dwg_save() {
    // R2007 holds every field. R2010 and later keep the table as table content, which holds the value flags, the flow direction
    // and the margins of the overrides, and per cell the rotation and auto-fit; the title suppression, the border colour mask and
    // the cell flags have no place there. A CADCraft save must keep what acadrust's own DWG holds.
    for (version, source) in [(Version::R2007, DxfVersion::AC1021), (Version::R2018, DxfVersion::AC1032)] {
        let mut doc = CadDocument::with_version(source);
        doc.add_entity(EntityType::Table(Box::new(overridden_cell(2, 2)))).unwrap();
        let [(_, held), (_, first), (_, second)] = dwg_saves(&doc, version);
        let (held, first, second) = (table_fields(&held), table_fields(&first), table_fields(&second));
        if version == Version::R2007 {
            let cells = (0..4).map(|i| (3, true, 5, 2, 0.5 + f64::from(i % 2), Some((5, RColor::Index(2))))).collect();
            let overrides = (13, Some(true), Some(1), Some(0.2));
            assert_eq!(held, (2, Some(overrides), Some((3, vec![RColor::Index(1), RColor::Index(3)])), cells), "the source");
        }
        if version == Version::R2018 {
            let rotations: Vec<f64> = held.3.iter().map(|c| c.4).collect();
            assert_eq!((held.0, held.1), (2, Some((12, None, Some(1), Some(0.2)))), "R2018: the table fields");
            assert_eq!(rotations, [0.5, 1.5, 0.5, 1.5], "R2018: the cell rotations");
            assert!(held.3.iter().all(|c| c.1), "R2018: auto-fit");
        }
        assert_eq!(first, held, "{version:?}: first save");
        assert_eq!(second, held, "{version:?}: second save");
    }
}

#[test]
fn a_table_made_here_writes_the_value_flags_autocad_writes_and_a_file_value_is_kept() {
    // AutoCAD LT 2025 writes 90 = 22 (0x16: has a block, rows run down) for a -TABLE with a title and a header row; the ODA spec adds
    // 0x20 when the title row is suppressed.
    let made = |title: bool, flags: Option<i32>| {
        let mut t = plan_table();
        t.title = title;
        t.overrides = TableOverrides { value_flags: flags, ..TableOverrides::default() };
        let mut d = Drawing::new_metric();
        d.add(&Space::Model, Common::default(), EntityKind::Table(t)).unwrap();
        d
    };
    let flags = |d: &Drawing| at(&groups(&dxf(d), "ACAD_TABLE"), 90).first().cloned();
    assert_eq!(flags(&made(true, None)).as_deref(), Some("22"));
    assert_eq!(flags(&made(false, None)).as_deref(), Some("54"));
    // A value read from a file stays as read, whatever the table's own state (the first 90 is the table's, the next are cells').
    assert_eq!(flags(&made(true, Some(2))).as_deref(), Some("2"));
    assert_eq!(flags(&made(false, Some(0))).as_deref(), Some("0"));
}

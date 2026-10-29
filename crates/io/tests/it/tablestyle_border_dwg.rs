//! A table style's border linetypes and override flags (`TableBorderPropertyFlags`) survive a DWG
//! save: the bridge DXF carries them (340 to 345 and 94 to 99 after the border groups of a cell
//! style) and acadrust writes them into the DWG's cell styles and reads them back. Needs acadrust
//! 7cb2356 or later.

use cadcraft_doc::{CellStyle, Drawing, Linetype, TableStyle};

fn dashed_borders() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.linetypes.push(Linetype::simple("DASHED", "Dashed", &[0.5, -0.25]));
    let mut st = TableStyle { name: "Plan".into(), ..TableStyle::default() };
    let mut data = st.cell_style(CellStyle::DATA);
    data.borders.top.linetype = Some("DASHED".into());
    data.borders.top.overrides = 2 | 4;
    data.borders.right.overrides = 8;
    st.set_cell_style(data);
    d.table_styles.push(st);
    d
}

fn assert_borders(d: &Drawing, what: &str) {
    let plan = d.table_styles.iter().find(|s| s.name == "Plan").expect("the table style Plan");
    let b = plan.cell_style(CellStyle::DATA).borders;
    assert_eq!((b.top.linetype.as_deref(), b.top.overrides), (Some("DASHED"), 6), "{what}: top border");
    // The right border has only the visibility override; acadrust gives it the ByBlock linetype.
    assert_eq!(b.right.overrides, 8, "{what}: right border");
}

#[test]
fn border_linetypes_and_override_flags_survive_a_dwg_save() {
    let dwg = cadcraft_io::write(&dashed_borders(), "x.dwg").unwrap();
    assert_borders(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "DWG");
    // The groups alone, without CADCraft's own xdata, carry them.
    let bridge = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    assert_borders(&cadcraft_io::read_dxf(bridge.replace("CELLSTYLES", "SOMETHING_ELSE").as_bytes()).unwrap(), "DWG without xdata");
    // A second save keeps them.
    let again = cadcraft_io::write(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_borders(&cadcraft_io::read(&again, "x.dwg").unwrap(), "second DWG save");
}

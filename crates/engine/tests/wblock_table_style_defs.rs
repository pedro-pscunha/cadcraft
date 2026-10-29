//! WBLOCK of a table writes the definitions its table style needs: the text styles of its cell
//! styles and the linetypes of their borders, although no other selected entity uses them.

use cadcraft_doc::{CellStyle, Common, Drawing, EntityKind, Linetype, Space, Table, TableCell, TableStyle, TextStyle};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io, set_write_noted};
use serde_json::json;

fn hooks() {
    set_io(IoHooks {
        read: |bytes, path| cadcraft_io::read(bytes, path).map_err(|e| e.to_string()),
        write: |drawing, path| cadcraft_io::write(drawing, path).map_err(|e| e.to_string()),
        plot: None,
    });
    set_write_noted(|drawing, path, opts| {
        cadcraft_io::write_with_noted(drawing, path, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary })
            .map_err(|e| e.to_string())
    });
}

/// A drawing with the text style `Notes`, the linetype `DASHED`, the table style `Plan` (Header
/// text in `Notes`, a dashed top border on the Data cells) and a table in that style.
fn source() -> (Drawing, String) {
    let mut d = Drawing::new_imperial();
    d.text_styles.push(TextStyle { name: "Notes".into(), ..TextStyle::default() });
    d.linetypes.push(Linetype::simple("DASHED", "Dashed", &[0.5, -0.25]));
    let mut st = TableStyle { name: "Plan".into(), ..TableStyle::default() };
    let mut header = st.cell_style(CellStyle::HEADER);
    header.text_style = "Notes".into();
    st.set_cell_style(header);
    let mut data = st.cell_style(CellStyle::DATA);
    data.borders.top.linetype = Some("DASHED".into());
    st.set_cell_style(data);
    d.table_styles.push(st);
    let cell = |t: &str| TableCell { text: t.into(), ..Default::default() };
    let t = Table {
        col_widths: vec![3.0, 2.0],
        row_heights: vec![0.4, 0.4],
        cells: vec![vec![cell("a"), cell("b")]],
        style: "Plan".into(),
        ..Table::default()
    };
    let h = d.add(&Space::Model, Common::default(), EntityKind::Table(t)).unwrap();
    (d, h.hex())
}

#[test]
fn wblock_of_a_table_keeps_the_text_styles_and_border_linetypes_of_its_table_style() {
    hooks();
    let (d, handle) = source();
    let mut session = Session::new();
    session.open_drawing(d, "source.dxf", None);
    let path = std::env::temp_dir().join(format!("cadcraft-wblock-table-style-{}.dwg", std::process::id()));
    session.execute("wblock", &json!({"handles": [handle], "path": path})).unwrap();
    let dwg = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    // What the DWG holds: read back without CADCraft's own xdata (the cell styles in JSON).
    let bridge = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    let back = cadcraft_io::read_dxf(bridge.replace("CELLSTYLES", "SOMETHING_ELSE").as_bytes()).unwrap();
    assert!(back.text_style("Notes").is_some(), "the text style of the Header cell style is written");
    assert!(back.linetypes.iter().any(|l| l.name.eq_ignore_ascii_case("DASHED")), "the border linetype is written");
    let plan = back.table_styles.iter().find(|s| s.name == "Plan").expect("the table style Plan");
    // (A DWG does not keep the text style a cell style names, WBLOCK or not: that is acadrust's row
    // style, so only the definition is checked.)
    assert_eq!(plan.cell_style(CellStyle::DATA).borders.top.linetype.as_deref(), Some("DASHED"));
}

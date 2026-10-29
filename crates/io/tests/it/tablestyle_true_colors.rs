//! True colours of a table style (the text, the fill and each border of a cell type) survive a
//! save. The DXF Reference has no TABLESTYLE group for them, so a user DXF holds them in
//! CADCraft's xdata; the DWG bridge carries them in 420, 421 and 422 to 427, as acadrust does.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{CellStyle, Drawing, TableStyle};

fn drawing() -> Drawing {
    let mut d = Drawing::new_imperial();
    let mut st = TableStyle { name: "Plan".into(), ..TableStyle::default() };
    let mut data = st.cell_style(CellStyle::DATA);
    data.text_color = Color::True(Rgb(1, 2, 3));
    data.fill = Some(Color::True(Rgb(4, 5, 6)));
    data.borders.top.color = Color::True(Rgb(10, 200, 30));
    data.borders.right.color = Color::True(Rgb(250, 0, 128));
    data.borders.left.color = Color::Index(3);
    st.set_cell_style(data);
    d.table_styles.push(st);
    d
}

fn assert_colors(d: &Drawing, what: &str) {
    let data = d.table_styles.iter().find(|s| s.name == "Plan").expect("the table style Plan").cell_style(CellStyle::DATA);
    assert_eq!(data.text_color, Color::True(Rgb(1, 2, 3)), "{what}: text");
    assert_eq!(data.fill, Some(Color::True(Rgb(4, 5, 6))), "{what}: fill");
    assert_eq!(data.borders.top.color, Color::True(Rgb(10, 200, 30)), "{what}: top border");
    assert_eq!(data.borders.right.color, Color::True(Rgb(250, 0, 128)), "{what}: right border");
    assert_eq!(data.borders.left.color, Color::Index(3), "{what}: an ACI border stays ACI");
}

#[test]
fn true_colors_survive_a_user_dxf_save() {
    let d = drawing();
    let text = cadcraft_io::write_dxf(&d);
    assert_colors(&cadcraft_io::read_dxf(text.as_bytes()).unwrap(), "DXF");
    // No private group in the TABLESTYLE record (its groups before the xdata).
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "OBJECTS").map(|s| cadcraft_dxf::records(&s.tags)).unwrap_or_default();
    let styles = objects.iter().filter(|(k, _)| k == "TABLESTYLE").flat_map(|(_, t)| t.iter().take_while(|t| t.code < 1000));
    assert!(styles.clone().all(|t| !(420..=427).contains(&t.code)), "no 420-427 in a user DXF TABLESTYLE");
}

#[test]
fn true_colors_survive_a_dwg_save_without_cadcraft_xdata() {
    let dwg = cadcraft_io::write(&drawing(), "x.dwg").unwrap();
    assert_colors(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "DWG");
    // The groups alone carry them (an AutoCAD file has no CADCraft xdata).
    let bridge = String::from_utf8(cadcraft_dwg::dwg_to_dxf(&dwg).unwrap()).unwrap();
    assert_colors(&cadcraft_io::read_dxf(bridge.replace("CELLSTYLES", "SOMETHING_ELSE").as_bytes()).unwrap(), "DWG without xdata");
    let again = cadcraft_io::write(&cadcraft_io::read(&dwg, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_colors(&cadcraft_io::read(&again, "x.dwg").unwrap(), "second DWG save");
}

//! A layout whose file gave no paper size name (group 4 empty) is saved without one, not with the default ANSI A.

use super::foreign_passthrough::{saved_twice, section, styled_objects, val};

#[test]
fn an_empty_paper_size_name_stays_empty() {
    let source = styled_objects().replace("4\nISO_A4_(297.00_x_210.00_MM)\n", "4\n\n");
    let d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    let text = saved_twice(&d).into_iter().next().unwrap();
    let layouts: Vec<String> = section(&text, "OBJECTS")
        .into_iter()
        .filter(|(k, t)| k == "LAYOUT" && t.iter().any(|g| g.code == 1 && g.str() == "A4"))
        .map(|(_, t)| val(&t, 4))
        .collect();
    assert!(!layouts.is_empty() && layouts.iter().all(String::is_empty), "{layouts:?}");
}

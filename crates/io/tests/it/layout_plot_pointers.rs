//! The shade plot handle (group 333 of a layout's `AcDbPlotSettings`, a hard pointer to a visual
//! style) of the Model layout and of a paper layout names the same object after a save, as DXF
//! and as DWG, and again after a second save. A save numbers kept objects anew, so the handle is
//! remapped, never copied. A handle whose object is gone becomes 0.

use cadcraft_dxf::Tag;

use super::foreign_passthrough::{dxf, layouts, saved_twice, section, subclass, text_of, val};

/// The layouts of `foreign_passthrough` (Model and Sheet) with the visual styles `Wire` (61) and
/// `Hidden` (62) in the named object dictionary, and the shade plot handles `model` and `sheet`.
fn fixture(model: &str, sheet: &str) -> String {
    let styles = dxf(&[
        (0, "DICTIONARY"),
        (5, "60"),
        (330, "C"),
        (100, "AcDbDictionary"),
        (281, "1"),
        (3, "Hidden"),
        (350, "62"),
        (3, "Wire"),
        (350, "61"),
        (0, "VISUALSTYLE"),
        (5, "61"),
        (330, "60"),
        (100, "AcDbVisualStyle"),
        (2, "Wire"),
        (70, "0"),
        (0, "VISUALSTYLE"),
        (5, "62"),
        (330, "60"),
        (100, "AcDbVisualStyle"),
        (2, "Hidden"),
        (70, "1"),
    ]);
    let text = layouts()
        .replace("3\nACAD_LAYOUT\n350\n20\n", "3\nACAD_LAYOUT\n350\n20\n3\nACAD_VISUALSTYLE\n350\n60\n")
        .replace("149\n-56.2\n", &format!("149\n-56.2\n333\n{model}\n"))
        .replace("149\n-200\n", &format!("149\n-200\n333\n{sheet}\n"));
    let at = text.rfind("0\nENDSEC\n").expect("the OBJECTS section ends");
    format!("{}{styles}{}", &text[..at], &text[at..])
}

/// The name of the object a layout's shade plot handle names in `text`, `Some("")` for the null
/// handle (a DWG stores no group for it), `None` when the layout or the object is not in the file.
fn shade_plot_style(text: &str, layout: &str) -> Option<String> {
    let objects = section(text, "OBJECTS");
    let (_, record) = objects.iter().find(|(k, t)| k == "LAYOUT" && text_of(&subclass(t, "AcDbLayout"), 1) == layout)?;
    let plot = subclass(record, "AcDbPlotSettings");
    let handle = plot.iter().find(|(c, _)| *c == 333).map_or("0".to_string(), |(_, v)| v.trim().to_string());
    if handle == "0" {
        return Some(String::new());
    }
    let target: &[Tag] = objects.iter().find(|(_, t)| val(t, 5) == handle).map(|(_, t)| t.as_slice())?;
    Some(val(target, 2))
}

fn assert_styles(text: &str, what: &str) {
    assert_eq!(shade_plot_style(text, "Model").as_deref(), Some("Wire"), "{what}: the Model layout's shade plot handle");
    assert_eq!(shade_plot_style(text, "Sheet").as_deref(), Some("Hidden"), "{what}: the paper layout's shade plot handle");
}

#[test]
fn shade_plot_handles_name_the_same_visual_style_after_a_save() {
    let d = cadcraft_io::read_dxf(fixture("61", "62").as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let what = ["DXF", "DWG"][i];
        assert_styles(&text, what);
        // A second save of the saved file keeps them.
        let again = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
        assert_styles(&cadcraft_io::write_dxf(&again), &format!("{what}, second save"));
    }
}

#[test]
fn a_shade_plot_handle_whose_object_is_gone_becomes_null() {
    let d = cadcraft_io::read_dxf(fixture("61", "FFF").as_bytes()).unwrap();
    for (i, text) in saved_twice(&d).into_iter().enumerate() {
        let what = ["DXF", "DWG"][i];
        assert_eq!(shade_plot_style(&text, "Model").as_deref(), Some("Wire"), "{what}");
        assert_eq!(shade_plot_style(&text, "Sheet").as_deref(), Some(""), "{what}: the dangling handle is 0");
    }
}

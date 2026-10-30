//! A layer's transparency (`AcCmTransparency` xdata) keeps the alpha the file gave while the
//! percentage still shows it: 127 and 128 are both 50 percent, and AutoCAD stores either.

use cadcraft_doc::{Drawing, Layer};

use super::foreign_passthrough::{saved_twice, section};

fn drawing(layers: &[(&str, u8, Option<u8>)]) -> Drawing {
    let mut d = Drawing::new_metric();
    for (name, transparency, alpha) in layers {
        d.layers.push(Layer { name: (*name).into(), transparency: *transparency, transparency_alpha: *alpha, ..Layer::default() });
    }
    d
}

/// The `AcCmTransparency` value (group 1071) of a layer in a DXF text.
fn stored(text: &str, layer: &str) -> Option<i64> {
    let (_, tags) = section(text, "TABLES").into_iter().find(|(k, t)| k == "LAYER" && t.iter().any(|g| g.code == 2 && g.str() == layer))?;
    let app = tags.iter().position(|g| g.code == 1001 && g.str() == "AcCmTransparency")?;
    tags[app..].iter().find(|g| g.code == 1071).map(cadcraft_dxf::Tag::i64)
}

#[test]
fn the_alpha_of_the_file_is_written_back() {
    let d = drawing(&[("A127", 50, Some(127)), ("A128", 50, Some(128)), ("Plain", 50, None)]);
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        assert_eq!(stored(&text, "A127"), Some(0x0200_007f), "{what}: 127");
        assert_eq!(stored(&text, "A128"), Some(0x0200_0080), "{what}: 128");
        // No alpha given: the one AutoCAD stores for 50 percent (127, truncated).
        assert_eq!(stored(&text, "Plain"), Some(0x0200_007f), "{what}: by percentage");
    }
}

#[test]
fn a_read_keeps_the_alpha_and_a_change_of_percentage_drops_it() {
    let d = drawing(&[("A128", 50, Some(128))]);
    let mut read = cadcraft_io::read(cadcraft_io::write_dxf(&d).as_bytes(), "x.dxf").unwrap();
    assert_eq!(read.layer("A128").map(|l| (l.transparency, l.transparency_alpha)), Some((50, Some(128))));
    let text = cadcraft_io::write_dxf(&read);
    assert_eq!(stored(&text, "A128"), Some(0x0200_0080), "written again as read");
    if let Some(l) = read.layer_mut("A128") {
        l.transparency = 60;
    }
    // 60 percent: the alpha of the file no longer shows as 60, so AutoCAD's own (102) is written.
    assert_eq!(stored(&cadcraft_io::write_dxf(&read), "A128"), Some(0x0200_0066));
}

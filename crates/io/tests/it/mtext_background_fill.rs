//! The background fill of an MTEXT (groups 90, 63, 421, 45 and 441 of its AcDbMText subclass) and the visual style pointer of a layer
//! (348) are not modelled and come back as the file had them.

use super::foreign_passthrough::{dxf, saved_twice, section, val};

fn source() -> String {
    let groups: Vec<(i32, &str)> = vec![
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (0, "LAYER"),
        (5, "40"),
        (100, "AcDbSymbolTableRecord"),
        (100, "AcDbLayerTableRecord"),
        (2, "Walls"),
        (70, "0"),
        (62, "7"),
        (6, "Continuous"),
        (348, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "MTEXT"),
        (5, "30"),
        (100, "AcDbEntity"),
        (8, "Walls"),
        (100, "AcDbMText"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (40, "2.5"),
        (71, "1"),
        (1, "NOTE"),
        (90, "3"),
        (63, "9"),
        (421, "13158600"),
        (45, "1.1"),
        (441, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    dxf(&groups)
}

#[test]
fn the_background_fill_and_the_layer_pointer_come_back() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let (_, mtext) = section(&text, "ENTITIES").into_iter().find(|(k, _)| k == "MTEXT").unwrap();
        // A DWG keeps the fill setting, colour and scale; the ACI colour (63) is only in a DXF.
        for (code, want) in [(90, "3"), (421, "13158600"), (45, "1.1")] {
            assert_eq!(val(&mtext, code), want, "{what}: MTEXT group {code}");
        }
        if what == "DXF" {
            assert_eq!((val(&mtext, 63), val(&mtext, 441)), ("9".into(), "0".into()));
        }
        if what == "DXF" {
            let (_, layer) = section(&text, "TABLES").into_iter().find(|(k, t)| k == "LAYER" && val(t, 2) == "Walls").unwrap();
            assert_eq!(val(&layer, 348), "0", "layer group 348");
        }
    }
}

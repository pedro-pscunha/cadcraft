//! Paper layouts keep their independent plot settings, including the source scale representation.
use super::foreign_passthrough::{layouts, section, subclass, text_of};
use cadcraft_dxf::Tag;

fn fixture(units: i64, rotation: i64, standard: bool) -> String {
    let source = layouts();
    let (prefix, rest) = source.split_once("0\nLAYOUT\n5\n31\n").unwrap();
    let (plot, suffix) = rest.split_once("100\nAcDbLayout\n").unwrap();
    let mut tags = cadcraft_dxf::parse_ascii(plot).unwrap();
    let values = [
        (1, "Assigned setup".to_string()),
        (6, "Camera".to_string()),
        (72, units.to_string()),
        (73, rotation.to_string()),
        (74, "3".to_string()),
        (75, if standard { "25" } else { "17" }.to_string()),
        (142, "1".to_string()),
        (143, if standard { "50" } else { "1" }.to_string()),
        (147, if standard { "0.02" } else { "0.5" }.to_string()),
    ];
    for tag in &mut tags {
        if let Some((_, value)) = values.iter().find(|(code, _)| *code == tag.code) {
            *tag = Tag::s(tag.code, value);
        }
        if tag.code == 70 {
            *tag = Tag::i(70, (tag.i64() & !16) | if standard { 16 } else { 0 });
        }
        if tag.code == 44 {
            *tag = Tag::f(44, 1024.0);
        }
        if tag.code == 45 {
            *tag = Tag::f(45, 768.0);
        }
    }
    let header = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$MEASUREMENT\n70\n1\n0\nENDSEC\n\
0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nVIEW\n70\n1\n0\nVIEW\n5\n60\n100\nAcDbSymbolTableRecord\n100\nAcDbViewTableRecord\n2\nCamera\n70\n0\n40\n20\n41\n30\n10\n0\n20\n0\n11\n0\n21\n0\n31\n1\n12\n0\n22\n0\n32\n0\n0\nENDTAB\n0\nENDSEC\n";
    format!("{header}{prefix}0\nLAYOUT\n5\n31\n{}100\nAcDbLayout\n{suffix}", cadcraft_dxf::write_ascii(&tags))
}

fn plot(bytes: &[u8]) -> Vec<(i32, String)> {
    let text = std::str::from_utf8(bytes).unwrap();
    let (_, tags) = section(text, "OBJECTS").into_iter().find(|(k, t)| k == "LAYOUT" && text_of(&subclass(t, "AcDbLayout"), 1) == "Sheet").unwrap();
    subclass(&tags, "AcDbPlotSettings")
}

fn value(tags: &[(i32, String)], code: i32) -> &str {
    &tags.iter().find(|(c, _)| *c == code).unwrap().1
}

#[test]
fn paper_plot_fields_survive_two_saves() {
    for (units, rotation, standard) in [(2, 3, true), (0, 2, false)] {
        let source = fixture(units, rotation, standard);
        let expected = plot(source.as_bytes());
        for format in ["saved.dxf", "saved.dwg"] {
            let mut drawing = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
            for _ in 0..2 {
                let saved = cadcraft_io::write(&drawing, format).unwrap();
                let text = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
                let actual = plot(&text);
                for code in [1, 6] {
                    assert_eq!(value(&actual, code), value(&expected, code), "{format}: {code}");
                }
                for code in [72, 73, 74, 75, 142, 143, 147, 44, 45] {
                    let (a, e) = (value(&actual, code).parse::<f64>().unwrap(), value(&expected, code).parse::<f64>().unwrap());
                    assert!((a - e).abs() < 1e-9, "{format}: {code}: {a} != {e}");
                }
                assert_eq!(value(&actual, 70).parse::<i64>().unwrap() & 16, if standard { 16 } else { 0 }, "{format}: standard-scale flag");
                drawing = cadcraft_io::read(&saved, format).unwrap();
            }
        }
    }
}

#[test]
fn page_edits_replace_only_the_source_fields_whose_meaning_changes() {
    let mut drawing = cadcraft_io::read_dxf(fixture(2, 3, true).as_bytes()).unwrap();
    let page = &mut drawing.layouts.iter_mut().find(|l| l.name == "Sheet").unwrap().page;
    page.landscape = !page.landscape;
    page.paper_mm = true;
    page.scale = 0.5;
    page.scale_to_fit = false;
    page.plot_area = "window".into();
    let saved = cadcraft_io::write(&drawing, "edited.dxf").unwrap();
    let actual = plot(&saved);
    for (code, expected) in [(1, "Assigned setup"), (6, ""), (72, "1"), (73, "1"), (74, "4"), (75, "16")] {
        assert_eq!(value(&actual, code), expected, "{code}");
    }
    assert_eq!(value(&actual, 70).parse::<i64>().unwrap() & 16, 0);
    assert_eq!(value(&actual, 142).parse::<f64>().unwrap(), 0.5);
    assert_eq!(value(&actual, 143).parse::<f64>().unwrap(), 1.0);
    assert_eq!(value(&actual, 147).parse::<f64>().unwrap(), 0.5);
}

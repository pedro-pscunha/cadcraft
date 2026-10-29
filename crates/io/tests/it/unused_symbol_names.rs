//! Symbol tables and named plot styles are not reduced to the names entities use.

use cadcraft_doc::{Drawing, Layer};
use cadcraft_dxf::{Tag, records, sections};

fn appids(text: &[u8]) -> Vec<String> {
    let tags = cadcraft_dxf::parse(text).unwrap();
    sections(&tags)
        .iter()
        .filter(|s| s.name == "TABLES")
        .flat_map(|s| records(&s.tags))
        .filter(|(k, _)| k == "APPID")
        .filter_map(|(_, t)| t.iter().find(|g| g.code == 2).map(Tag::str))
        .collect()
}

fn plot_styles(text: &[u8]) -> Vec<String> {
    let tags = cadcraft_dxf::parse(text).unwrap();
    let objects: Vec<_> = sections(&tags).iter().filter(|s| s.name == "OBJECTS").flat_map(|s| records(&s.tags)).collect();
    let root = &objects[0].1;
    let dict = root.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ACAD_PLOTSTYLENAME").unwrap()[1].str();
    let body = &objects.iter().find(|(_, t)| t.iter().any(|g| g.code == 5 && g.str() == dict)).unwrap().1;
    body.iter().filter(|g| g.code == 3).map(Tag::str).collect()
}

#[test]
fn unnamed_styles_survive_without_becoming_selectable_by_name() {
    let source = "0\nSECTION\n2\nTABLES\n\
        0\nTABLE\n2\nSTYLE\n5\n10\n70\n1\n\
        0\nSTYLE\n5\n11\n330\n10\n100\nAcDbSymbolTableRecord\n100\nAcDbTextStyleTableRecord\n2\n\n70\n0\n40\n0\n41\n1\n50\n0\n71\n0\n3\nTXT.SHX\n4\n\n0\nENDTAB\n\
        0\nTABLE\n2\nDIMSTYLE\n5\n20\n70\n1\n\
        0\nDIMSTYLE\n105\n21\n330\n20\n100\nAcDbSymbolTableRecord\n100\nAcDbDimStyleTableRecord\n2\n\n70\n0\n40\n2.5\n340\n11\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n";
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for _ in 0..2 {
        assert!(d.text_style("").is_none());
        assert!(d.dim_style("").is_none());
        // A DWG keeps the records nameless, as AutoCAD files have them.
        let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
        let back = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
        check_unnamed_styles(&back, false);
        // A user DXF names them: AutoCAD refuses a table record with an empty name.
        check_unnamed_styles(cadcraft_io::write_dxf(&d).as_bytes(), true);
        d = cadcraft_io::read_dxf(&back).unwrap();
    }
}

/// The unnamed STYLE and DIMSTYLE of `text`: nameless (a DWG), or with a name of their own, not that of
/// another record (`named`, a user DXF).
fn check_unnamed_styles(text: &[u8], named: bool) {
    let tags = cadcraft_dxf::parse(text).unwrap();
    let records: Vec<_> = sections(&tags).iter().filter(|s| s.name == "TABLES").flat_map(|s| records(&s.tags)).collect();
    let value = |tags: &[Tag], code| tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default();
    // The records that hold the source's TXT.SHX font, and the dimension style that points at one.
    let styles: Vec<_> = records.iter().filter(|(k, t)| k == "STYLE" && value(t, 3) == "TXT.SHX").collect();
    let dims: Vec<_> = records.iter().filter(|(k, t)| k == "DIMSTYLE" && t.iter().any(|t| t.code == 40 && t.f64() == 2.5)).collect();
    assert_eq!((styles.len(), dims.len()), (1, 1), "the unnamed STYLE and DIMSTYLE are kept");
    assert_eq!(value(&styles[0].1, 3), "TXT.SHX");
    assert_eq!(value(&dims[0].1, 340), value(&styles[0].1, 5));
    for (kind, record) in [("STYLE", &styles[0].1), ("DIMSTYLE", &dims[0].1)] {
        let name = value(record, 2);
        assert_eq!(!name.trim().is_empty(), named, "{kind} name {name:?}");
        if named {
            let same = records.iter().filter(|(k, t)| k == kind && value(t, 2).eq_ignore_ascii_case(&name)).count();
            assert_eq!(same, 1, "{kind} name {name:?} is unique");
        }
    }
    // The tables count the records.
    for kind in ["STYLE", "DIMSTYLE"] {
        let table = records.iter().find(|(k, t)| k == "TABLE" && value(t, 2) == kind).map(|(_, t)| value(t, 70).trim().parse::<usize>().unwrap());
        assert_eq!(table, Some(records.iter().filter(|(k, _)| k == kind).count()), "{kind} table count");
    }
}

#[test]
fn unused_appids_survive_dxf_and_dwg_saves() {
    let source = "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nAPPID\n5\n10\n70\n1\n0\nAPPID\n5\n11\n330\n10\n100\nAcDbSymbolTableRecord\n100\nAcDbRegAppTableRecord\n2\nSPARE_APP\n70\n0\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n";
    let mut d = cadcraft_io::read_dxf(source.as_bytes()).unwrap();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        assert!(appids(text.as_bytes()).iter().any(|n| n == "SPARE_APP"));
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        let back = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
        assert!(appids(&back).iter().any(|n| n == "SPARE_APP"));
        d = cadcraft_io::read_dxf(&back).unwrap();
    }
}

#[test]
fn unused_named_plot_styles_survive_dxf_and_dwg_saves() {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { plot_style: "SparePen".into(), ..Layer::new("Pens") });
    let text = cadcraft_io::write_dxf(&d);
    let mut d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    d.layer_mut("Pens").unwrap().plot_style = "Normal".into();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        assert!(plot_styles(text.as_bytes()).iter().any(|n| n == "SparePen"));
        let dwg = cadcraft_dwg::dxf_to_dwg(text.as_bytes()).unwrap();
        let back = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
        assert!(plot_styles(&back).iter().any(|n| n == "SparePen"));
        d = cadcraft_io::read_dxf(&back).unwrap();
    }
}

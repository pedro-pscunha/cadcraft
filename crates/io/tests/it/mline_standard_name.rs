//! AutoCAD names its built-in multiline style `STANDARD` in the MLINESTYLE object (group 2) and `Standard` as its key in the
//! `ACAD_MLINESTYLE` dictionary. A style read under the key keeps that group 2 on a save.

use cadcraft_doc::{Drawing, MLineStyle};

#[test]
fn the_standard_style_keeps_its_upper_case_object_name_under_the_dictionary_key() {
    let mut d = Drawing::new_metric();
    d.mline_styles = vec![MLineStyle { name: "Standard".into(), ..MLineStyle::default() }];
    let tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap();
    let records = cadcraft_dxf::records(&section.tags);
    let style = records.iter().find(|(k, _)| k == "MLINESTYLE").expect("a style").1.clone();
    let name = style.iter().find(|t| t.code == 2).map(|t| t.str());
    assert_eq!(name.as_deref(), Some("STANDARD"));
    let key_of_style = records
        .iter()
        .filter(|(k, _)| k == "DICTIONARY")
        .flat_map(|(_, b)| b.windows(2).map(<[_]>::to_vec).collect::<Vec<_>>())
        .any(|w| w[0].code == 3 && w[0].str() == "Standard" && w[1].code == 350);
    assert!(key_of_style, "the dictionary key stays as the style is named");
}

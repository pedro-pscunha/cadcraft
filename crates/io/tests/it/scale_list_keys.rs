//! ACAD_SCALELIST keeps the key AutoCAD gave each scale (`A0`, `B3`, `*A1`); a scale without one
//! gets the next free key of AutoCAD's series.

use cadcraft_doc::{AnnoScale, Drawing};
use cadcraft_dxf::{Tag, records, sections};

fn scale(handle: &str, name: &str, paper: f64, drawing: f64) -> String {
    format!(
        "0\nSCALE\n5\n{handle}\n102\n{{ACAD_REACTORS\n330\nB7\n102\n}}\n330\nB7\n100\nAcDbScale\n70\n0\n300\n{name}\n140\n{paper}\n141\n{drawing}\n290\n0\n"
    )
}

fn source() -> String {
    let mut s = String::from("0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n3\nACAD_SCALELIST\n350\nB7\n");
    s.push_str("0\nDICTIONARY\n5\nB7\n330\nC\n100\nAcDbDictionary\n281\n1\n");
    for (key, handle) in [("*A1", "A1"), ("A0", "A2"), ("B3", "A3")] {
        s.push_str(&format!("3\n{key}\n350\n{handle}\n"));
    }
    s.push_str(&scale("A1", "1:200", 1.0, 200.0));
    s.push_str(&scale("A2", "1:50", 1.0, 50.0));
    s.push_str(&scale("A3", "1:20", 1.0, 20.0));
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// The keys of the ACAD_SCALELIST dictionary of a written drawing, with the scale each names.
fn written_keys(text: &str) -> Vec<(String, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let objects: Vec<(String, Vec<Tag>)> = sections(&tags).into_iter().filter(|s| s.name == "OBJECTS").flat_map(|s| records(&s.tags)).collect();
    let handle = |body: &[Tag]| body.iter().find(|t| t.code == 5).unwrap().str();
    let root = objects.iter().find(|(k, b)| k == "DICTIONARY" && b.iter().any(|t| t.code == 3 && t.str() == "ACAD_SCALELIST")).unwrap();
    let list_handle = root.1.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ACAD_SCALELIST").unwrap()[1].str();
    let list = objects.iter().find(|(k, b)| k == "DICTIONARY" && handle(b) == list_handle).unwrap();
    list.1
        .windows(2)
        .filter(|p| p[0].code == 3)
        .map(|p| {
            let object = objects.iter().find(|(k, b)| k == "SCALE" && handle(b) == p[1].str()).unwrap();
            (p[0].str(), object.1.iter().find(|t| t.code == 300).unwrap().str())
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect()
}

#[test]
fn a_scale_keeps_the_key_of_its_file() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert_eq!(written_keys(&cadcraft_io::write_dxf(&d)), pairs(&[("*A1", "1:200"), ("A0", "1:50"), ("B3", "1:20")]));
}

#[test]
fn a_new_scale_takes_the_next_free_key_of_the_series() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    d.scales.push(AnnoScale::new("1:7", 1.0, 7.0));
    d.scales.push(AnnoScale::new("1:9", 1.0, 9.0));
    assert_eq!(written_keys(&cadcraft_io::write_dxf(&d)), pairs(&[("*A1", "1:200"), ("A0", "1:50"), ("A1", "1:7"), ("A2", "1:9"), ("B3", "1:20")]));
}

#[test]
fn a_drawing_never_read_from_a_file_numbers_its_scales_from_a0() {
    let mut d = Drawing::new_metric();
    d.scales = vec![AnnoScale::new("1:5", 1.0, 5.0), AnnoScale::new("1:10", 1.0, 10.0)];
    assert_eq!(written_keys(&cadcraft_io::write_dxf(&d)), pairs(&[("A0", "1:5"), ("A1", "1:10")]));
}

/// The unit scale flag (290) is on AutoCAD's scale "1:1" only, not on another scale of factor 1.
#[test]
fn only_the_scale_named_one_to_one_is_the_unit_scale() {
    let mut d = Drawing::new_metric();
    d.scales = vec![AnnoScale::new("1:1", 1.0, 1.0), AnnoScale::new("1'-0\" = 1'-0\"", 12.0, 12.0), AnnoScale::new("1:50", 1.0, 50.0)];
    let tags = cadcraft_dxf::parse(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
    let flags: Vec<(String, i64)> = sections(&tags)
        .into_iter()
        .filter(|s| s.name == "OBJECTS")
        .flat_map(|s| records(&s.tags))
        .filter(|(k, _)| k == "SCALE")
        .map(|(_, body)| (body.iter().find(|t| t.code == 300).unwrap().str(), body.iter().find(|t| t.code == 290).unwrap().i64()))
        .collect();
    assert_eq!(flags, vec![("1:1".to_string(), 1), ("1'-0\" = 1'-0\"".to_string(), 0), ("1:50".to_string(), 0)]);
}

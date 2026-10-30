//! Rebuilt source dictionaries keep their duplicate-record cloning policy.
use super::foreign_passthrough::{entry, layouts, section, val};

const POLICIES: [(&str, i64); 7] = [
    ("ACAD_LAYOUT", 3),
    ("ACAD_GROUP", 4),
    ("ACAD_MLEADERSTYLE", 5),
    ("ACAD_TABLESTYLE", 0),
    ("ACAD_MLINESTYLE", 2),
    ("ACAD_SCALELIST", 3),
    ("ACAD_PLOTSTYLENAME", 4),
];

fn source() -> String {
    let mut text = layouts()
        .replace("100\nAcDbDictionary\n281\n1\n3\nACAD_LAYOUT", "100\nAcDbDictionary\n281\n2\n3\nACAD_LAYOUT")
        .replace("5\n20\n330\nC\n100\nAcDbDictionary\n281\n1\n", "5\n20\n330\nC\n100\nAcDbDictionary\n281\n3\n");
    let mut entries = String::new();
    let mut dictionaries = String::new();
    for (i, (key, policy)) in POLICIES.iter().enumerate().skip(1) {
        let handle = 0x20 + i;
        entries.push_str(&format!("3\n{key}\n350\n{handle:X}\n"));
        dictionaries.push_str(&format!("0\nDICTIONARY\n5\n{handle:X}\n330\nC\n100\nAcDbDictionary\n281\n{policy}\n"));
    }
    text = text.replace("3\nACAD_LAYOUT\n350\n20\n", &format!("3\nACAD_LAYOUT\n350\n20\n{entries}"));
    let at = text.rfind("0\nENDSEC\n").unwrap();
    text.insert_str(at, &dictionaries);
    format!("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n{text}")
}

fn assert_policies(bytes: &[u8]) {
    let records = section(std::str::from_utf8(bytes).unwrap(), "OBJECTS");
    let (_, root) = records.iter().find(|(k, t)| k == "DICTIONARY" && val(t, 330) == "0").unwrap();
    assert_eq!(val(root, 281), "2");
    for (key, policy) in POLICIES {
        let handle = entry(root, key);
        let (_, dictionary) = records.iter().find(|(_, t)| val(t, 5) == handle).unwrap();
        assert_eq!(val(dictionary, 281).parse::<i64>().unwrap(), policy, "{key}");
    }
}

#[test]
fn cloning_policy_survives_modelled_dictionary_rebuilds() {
    for format in ["saved.dxf", "saved.dwg"] {
        let mut drawing = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
        for _ in 0..2 {
            let saved = cadcraft_io::write(&drawing, format).unwrap();
            let dxf = if format.ends_with("dwg") { cadcraft_dwg::dwg_to_dxf(&saved).unwrap() } else { saved.clone() };
            assert_policies(&dxf);
            drawing = cadcraft_io::read(&saved, format).unwrap();
        }
    }
}

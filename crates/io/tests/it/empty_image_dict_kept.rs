//! An ACAD_IMAGE_DICT with no image definition is kept as the file had it (AutoCAD's own save keeps it): the dictionary and its entry in the
//! named object dictionary.

use cadcraft_dxf::{Tag, records, sections};

fn source() -> String {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n0\nENDSEC\n\
     0\nSECTION\n2\nOBJECTS\n\
     0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nACAD_IMAGE_DICT\n350\n40\n\
     0\nDICTIONARY\n5\n40\n102\n{ACAD_REACTORS\n330\nC\n102\n}\n330\nC\n100\nAcDbDictionary\n281\n1\n\
     0\nENDSEC\n0\nEOF\n"
        .into()
}

fn objects(text: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    records(&sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap().tags)
}

fn handle(tags: &[Tag]) -> String {
    tags.iter().find(|t| t.code == 5).map(Tag::str).unwrap_or_default()
}

#[test]
fn an_empty_image_dictionary_survives_saves() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        let objs = objects(&text);
        let root =
            objs.iter().find(|(_, t)| t.iter().any(|g| g.code == 3 && g.str() == "ACAD_IMAGE_DICT")).expect("the named object dictionary lists it");
        let listed = root.1.windows(2).find(|p| p[0].code == 3 && p[0].str() == "ACAD_IMAGE_DICT").map(|p| p[1].str()).unwrap();
        let dict = objs.iter().find(|(kind, t)| kind == "DICTIONARY" && handle(t) == listed).expect("the dictionary it names is written");
        assert!(!dict.1.iter().any(|g| g.code == 3), "it stays empty");
        d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    }
}

#[test]
fn a_kept_empty_image_dictionary_does_not_clash_with_the_one_a_new_image_needs() {
    // An IMAGE without a definition gets one on save, with an ACAD_IMAGE_DICT of its own: the kept empty one must not be written beside it.
    let image = "0\nIMAGE\n5\n31\n100\nAcDbEntity\n8\n0\n100\nAcDbRasterImage\n90\n0\n10\n5\n20\n5\n30\n0\n11\n0.01\n21\n0\n31\n0\n12\n0\n22\n0.01\n32\n0\n13\n640\n23\n480\n340\n99\n70\n15\n71\n2\n91\n4\n14\n-0.5\n24\n-0.5\n14\n639.5\n24\n-0.5\n14\n320\n24\n479.5\n14\n-0.5\n24\n-0.5\n";
    let text = source().replace("0\nSECTION\n2\nENTITIES\n", &format!("0\nSECTION\n2\nENTITIES\n{image}"));
    let d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    let objs = objects(&saved);
    let keys = objs
        .iter()
        .filter(|(kind, _)| kind == "DICTIONARY")
        .map(|(_, t)| t.windows(2).filter(|p| p[0].code == 3 && p[0].str() == "ACAD_IMAGE_DICT").count())
        .sum::<usize>();
    assert_eq!(keys, 1, "one entry for the image dictionary in the named object dictionary");
    let handles: Vec<String> = objs.iter().map(|(_, t)| handle(t)).filter(|h| !h.is_empty()).collect();
    let unique: std::collections::BTreeSet<&String> = handles.iter().collect();
    assert_eq!(handles.len(), unique.len(), "no handle is written twice");
}

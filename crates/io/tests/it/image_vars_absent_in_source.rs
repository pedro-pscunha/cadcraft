//! A file that has an image dictionary but no RASTERVARIABLES saves without one: AutoCAD's own save does not add it.

use cadcraft_dxf::{records, sections};

fn source() -> String {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n0\nENDSEC\n\
     0\nSECTION\n2\nENTITIES\n0\nENDSEC\n\
     0\nSECTION\n2\nOBJECTS\n\
     0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n3\nACAD_IMAGE_DICT\n350\n40\n\
     0\nDICTIONARY\n5\n40\n330\nC\n100\nAcDbDictionary\n3\nUnusedLogo\n350\n41\n\
     0\nIMAGEDEF\n5\n41\n330\n40\n100\nAcDbRasterImageDef\n90\n0\n1\nimages/logo.jpg\n10\n640\n20\n480\n11\n0.25\n21\n0.5\n280\n0\n281\n2\n\
     0\nENDSEC\n0\nEOF\n"
        .into()
}

fn kinds(text: &str) -> Vec<String> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    records(&sections(&tags).into_iter().find(|s| s.name == "OBJECTS").unwrap().tags).into_iter().map(|(kind, _)| kind).collect()
}

#[test]
fn a_file_without_raster_variables_is_saved_without_them() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for _ in 0..2 {
        let text = cadcraft_io::write_dxf(&d);
        let kinds = kinds(&text);
        assert_eq!(kinds.iter().filter(|k| *k == "IMAGEDEF").count(), 1, "the unused definition is kept");
        assert!(!kinds.iter().any(|k| k == "RASTERVARIABLES"), "{kinds:?}");
        d = cadcraft_io::read_dxf(text.as_bytes()).unwrap();
    }
}

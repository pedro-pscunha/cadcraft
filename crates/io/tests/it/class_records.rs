//! A class keeps the application name and proxy flags AutoCAD gave it, and the classes no object uses stay: AutoCAD's own save of the
//! same drawings keeps all 59 (Citycon) and 299 (DPInc) classes, with the same group 3 and 90.

use super::foreign_passthrough::{dxf, section, val};

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "CLASSES"),
        (0, "CLASS"),
        (1, "MLEADERSTYLE"),
        (2, "AcDbMLeaderStyle"),
        (3, "ACDB_MLEADERSTYLE_CLASS"),
        (90, "1153"),
        (91, "0"),
        (280, "0"),
        (281, "0"),
        (0, "CLASS"),
        (1, "NPOCOLLECTION"),
        (2, "AcDbImpNonPersistentObjectCollection"),
        (3, "ObjectDBX Classes"),
        (90, "0"),
        (91, "0"),
        (280, "0"),
        (281, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

#[test]
fn the_application_and_flags_of_the_file_are_kept_and_unused_classes_stay() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let classes = section(&text, "CLASSES");
    let class = |name: &str| classes.iter().find(|(k, t)| k == "CLASS" && val(t, 1) == name).map(|(_, t)| t.clone());
    let style = class("MLEADERSTYLE").expect("MLEADERSTYLE");
    assert_eq!((val(&style, 3).as_str(), val(&style, 90).as_str()), ("ACDB_MLEADERSTYLE_CLASS", "1153"));
    let unused = class("NPOCOLLECTION").expect("a class no object uses stays");
    assert_eq!(val(&unused, 2), "AcDbImpNonPersistentObjectCollection");
    // The classes the writer needs still come once each.
    assert_eq!(classes.iter().filter(|(k, t)| k == "CLASS" && val(t, 1) == "SCALE").count(), 1);
    // And the file reads back the same way.
    let again = cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
    assert_eq!(section(&again, "CLASSES").len(), classes.len());
}

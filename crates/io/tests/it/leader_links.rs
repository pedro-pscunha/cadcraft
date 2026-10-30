//! A LEADER's annotation type (73) and annotation link (340) are modelled and written once, and the annotation it points at lists
//! the leader among its persistent reactors, as AutoCAD writes them.

use super::foreign_passthrough::{dxf, section, val};

fn source() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LEADER"),
        (5, "60"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbLeader"),
        (3, "Standard"),
        (71, "1"),
        (72, "0"),
        (73, "0"),
        (76, "2"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (10, "5"),
        (20, "5"),
        (30, "0"),
        (340, "61"),
        (0, "MTEXT"),
        (5, "61"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbMText"),
        (10, "5"),
        (20, "5"),
        (30, "0"),
        (40, "2"),
        (71, "1"),
        (1, "NOTE"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

#[test]
fn the_annotation_link_is_written_once_and_the_annotation_lists_the_leader() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let ents = section(&text, "ENTITIES");
    let (_, leader) = ents.iter().find(|(k, _)| k == "LEADER").unwrap();
    assert_eq!(leader.iter().filter(|t| t.code == 340).count(), 1, "one annotation link");
    assert_eq!(leader.iter().filter(|t| t.code == 73).count(), 1, "one annotation type");
    let (_, mtext) = ents.iter().find(|(k, _)| k == "MTEXT").unwrap();
    let handle = val(leader, 5);
    let start = mtext.iter().position(|t| t.code == 102 && t.str() == "{ACAD_REACTORS").unwrap();
    let reactors: Vec<String> = mtext[start + 1..].iter().take_while(|t| t.code == 330).map(|t| t.str()).collect();
    assert_eq!(reactors, [handle]);
}

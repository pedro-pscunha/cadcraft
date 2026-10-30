//! The BLOCK and ENDBLK entities of a block definition keep the properties a file gave them that the model has no field for:
//! linetype, linetype scale, colour, lineweight (AutoCAD writes `370 -2`, by block) and transparency.

use super::foreign_passthrough::{dxf, saved_twice, section, val};

fn source() -> String {
    let props = [(6, "DASHED"), (48, "0.5"), (62, "3"), (370, "-2"), (440, "33554687")];
    let mut groups: Vec<(i32, &str)> = vec![(0, "SECTION"), (2, "BLOCKS"), (0, "BLOCK"), (5, "A0"), (330, "A1"), (100, "AcDbEntity"), (8, "0")];
    groups.extend(props);
    groups.extend([(100, "AcDbBlockBegin"), (2, "Box"), (70, "0"), (10, "0"), (20, "0"), (30, "0"), (3, "Box"), (1, "")]);
    groups.extend([(0, "ENDBLK"), (5, "A2"), (330, "A1"), (100, "AcDbEntity"), (8, "0")]);
    groups.extend(props);
    groups.extend([(100, "AcDbBlockEnd"), (0, "ENDSEC"), (0, "EOF")]);
    dxf(&groups)
}

#[test]
fn block_and_endblk_keep_their_line_properties() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let blocks = section(&text, "BLOCKS");
        let begin = blocks.iter().position(|(k, t)| k == "BLOCK" && val(t, 2) == "Box").unwrap();
        for (kind, at) in [("BLOCK", begin), ("ENDBLK", begin + 1)] {
            let (found, tags) = &blocks[at];
            assert_eq!(found, kind);
            // A DWG keeps what its objects hold: the lineweight and the colour of the block entities.
            assert_eq!(val(tags, 370), "-2", "{what} {kind}: lineweight");
            assert_eq!(val(tags, 62), "3", "{what} {kind}: colour");
            if what == "DXF" {
                assert_eq!(val(tags, 6), "DASHED", "{kind}: linetype");
                assert_eq!(val(tags, 440), "33554687", "{kind}: transparency");
            }
        }
    }
}

#[test]
fn the_text_of_group_1_of_a_block_comes_back() {
    let groups = [
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (5, "A0"),
        (330, "A1"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockBegin"),
        (2, "G$C1"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (3, "G$C1"),
        (1, "Acad:XRef"),
        (0, "ENDBLK"),
        (5, "A2"),
        (330, "A1"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbBlockEnd"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    let d = cadcraft_io::read_dxf(dxf(&groups).as_bytes()).unwrap();
    let text = cadcraft_io::write_dxf(&d);
    let (_, block) = section(&text, "BLOCKS").into_iter().find(|(k, t)| k == "BLOCK" && val(t, 2) == "G$C1").unwrap();
    assert_eq!(block.iter().filter(|t| t.code == 1).map(|t| t.str()).collect::<Vec<_>>(), ["Acad:XRef"]);
}

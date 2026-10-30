//! An entity's fixed transparency (group 440, `0x02000000` and an alpha) is written back with the alpha the file gave, though several
//! alpha values show as one percentage (127 and 128 are both 50 percent), and the drawings of the corpus hold both. The alphas AutoCAD
//! writes for whole percentages are truncated (76 for 70 percent, 178 for 30, 226 for 11); a percentage set here gets that alpha.

use cadcraft_doc::{Handle, Transparency};

/// Lines with the alphas the real drawings hold, handles 100 and up.
const ALPHAS: [u8; 10] = [51, 76, 102, 127, 128, 153, 165, 178, 216, 226];

fn source() -> String {
    let mut text = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (i, alpha) in ALPHAS.iter().enumerate() {
        text += &format!(
            "0\nLINE\n5\n{:X}\n100\nAcDbEntity\n8\n0\n440\n{}\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n",
            0x100 + i,
            0x0200_0000 + i64::from(*alpha)
        );
    }
    text + "0\nENDSEC\n0\nEOF\n"
}

fn written(d: &cadcraft_doc::Drawing, name: &str) -> Vec<u8> {
    let dxf = match name {
        "dwg" => cadcraft_io::write_dxf(&cadcraft_io::read(&cadcraft_io::write(d, "x.dwg").unwrap(), "x.dwg").unwrap()),
        _ => cadcraft_io::write_dxf(d),
    };
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    tags.iter().filter(|t| t.code == 440).map(|t| u8::try_from(t.i64() & 0xff).unwrap()).collect()
}

#[test]
fn the_alpha_a_file_gave_is_written_back_exactly_in_dxf_and_dwg() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    assert_eq!(d.entity(Handle(0x104)).unwrap().common.transparency, Transparency::Percent(50));
    for name in ["dxf", "dwg"] {
        assert_eq!(written(&d, name), ALPHAS, "{name}");
        // A second save is the same.
        let again = cadcraft_io::read_dxf(cadcraft_io::write_dxf(&d).as_bytes()).unwrap();
        assert_eq!(written(&again, name), ALPHAS, "{name}, second save");
    }
}

#[test]
fn a_percentage_set_here_gets_the_truncated_alpha_unless_it_still_shows_the_files() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    // The entity that was alpha 128 (50 percent) is set to 40 percent: alpha 153. Set back to 50 it shows the file's 128 again.
    d.modify_entity(Handle(0x104), |e| e.common.transparency = Transparency::Percent(40)).unwrap();
    assert_eq!(written(&d, "dxf")[4], 153);
    d.modify_entity(Handle(0x104), |e| e.common.transparency = Transparency::Percent(50)).unwrap();
    assert_eq!(written(&d, "dxf")[4], 128, "the same percentage shows the file's alpha again");
    // Without the file's alpha, the truncated one.
    d.modify_entity(Handle(0x104), |e| {
        e.common.transparency = Transparency::Percent(30);
        e.common.transparency_alpha = None;
    })
    .unwrap();
    assert_eq!(written(&d, "dxf")[4], 178);
}

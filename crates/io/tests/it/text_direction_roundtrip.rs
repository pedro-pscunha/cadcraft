//! Groups of MTEXT that a CADCraft edit does not change survive a save after the edit: the drawing direction (72) of an MTEXT and
//! of a multiline attribute's embedded text, the embedded text's group 46, and the group 72 after the attribute type. The fixture
//! is the multiline attribute drawing of `multiline_attributes.rs` with non-default values and an MTEXT.

use cadcraft_doc::{Drawing, EntityKind, MText};

fn dxf(src: &str) -> Vec<u8> {
    let mut out = String::new();
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        out.push_str(&format!("{code}\n{value}\n"));
    }
    out.into_bytes()
}

const MTEXT: &str =
    "0 MTEXT\n5 50\n330 1F\n100 AcDbEntity\n8 0\n100 AcDbMText\n10 0\n20 0\n30 0\n40 2\n41 0\n71 1\n72 3\n1 VERTICAL\n7 Standard\n73 1\n44 1\n";

fn fixture() -> Vec<u8> {
    let src = super::multiline_attributes::FIXTURE
        // The ATTRIB's group 72 after its type, then its embedded text's group 46 and drawing direction.
        .replacen("71 2\n72 1\n", "71 2\n72 3\n", 1)
        .replacen("46 0\n71 8\n72 5\n1 NORTH", "46 2.5\n71 8\n72 3\n1 NORTH", 1)
        .replacen("0 ENDSEC\n0 SECTION\n2 OBJECTS", &format!("{MTEXT}0 ENDSEC\n0 SECTION\n2 OBJECTS"), 1);
    dxf(&src)
}

fn mtext(d: &Drawing) -> &MText {
    d.model.iter().find_map(|e| if let EntityKind::MText(t) = &e.kind { Some(t) } else { None }).expect("an MTEXT")
}

fn attribute(d: &Drawing) -> &cadcraft_doc::Attrib {
    d.model.iter().find_map(|e| if let EntityKind::Insert(i) = &e.kind { i.attribs.first() } else { None }).expect("an attribute")
}

/// The group 72 after the type of the multiline definition in block TAG (`None`: no such definition).
fn definition_type_72(d: &Drawing) -> Option<Option<i64>> {
    let block = d.block("TAG")?;
    block.entities.iter().find_map(|e| if let EntityKind::AttDef(a) = &e.kind { a.multiline.as_ref().map(|m| m.type_72) } else { None })
}

/// The drawing with the MTEXT and the attribute moved, so no source geometry is replayed for them.
fn moved(d: &Drawing) -> Drawing {
    let mut d = d.clone();
    let handles: Vec<_> = d.model.iter().map(|e| e.handle).collect();
    for h in handles {
        d.model
            .modify(h, |e| match &mut e.kind {
                EntityKind::MText(t) => t.insert.x += 1.0,
                EntityKind::Insert(i) => {
                    for a in &mut i.attribs {
                        a.text.insert.x += 1.0;
                        if let Some(m) = &mut a.multiline {
                            m.mtext.insert.x += 1.0;
                        }
                    }
                }
                _ => {}
            })
            .unwrap();
    }
    d
}

fn check(d: &Drawing, what: &str) {
    assert_eq!(mtext(d).drawing_direction, Some(3), "{what}: MTEXT 72");
    let m = attribute(d).multiline.as_ref().expect("a multiline attribute");
    assert_eq!(m.mtext.drawing_direction, Some(3), "{what}: embedded 72");
    assert_eq!(m.embedded_46, Some(2.5), "{what}: embedded 46");
    assert_eq!(m.type_72, Some(3), "{what}: 72 after the attribute type");
}

#[test]
fn the_text_direction_groups_are_read() {
    check(&cadcraft_io::read_dxf(&fixture()).unwrap(), "read");
}

#[test]
fn they_survive_a_save_after_the_text_was_moved_in_the_dxf_and_the_dwg() {
    let d = moved(&cadcraft_io::read_dxf(&fixture()).unwrap());
    let again = cadcraft_io::read(&cadcraft_io::write(&d, "x.dxf").unwrap(), "x.dxf").unwrap();
    check(&again, "DXF");
    assert_eq!(definition_type_72(&again), Some(Some(0)), "DXF: the definition's 72 after the type");
    let dwg = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    assert_eq!(mtext(&dwg).drawing_direction, Some(3), "DWG: MTEXT 72");
    let m = attribute(&dwg).multiline.as_ref().expect("a multiline attribute in the DWG");
    assert_eq!(m.mtext.drawing_direction, Some(3), "DWG: embedded 72");
    assert_eq!(m.embedded_46, Some(2.5), "DWG: embedded 46");
    // A DWG has no group 72 after the attribute type: the DWG reader writes a fixed value, which is not read.
    assert_eq!(m.type_72, None, "DWG: the 72 after the type");
    assert_eq!(definition_type_72(&dwg), Some(None), "DWG: the definition's 72 after the type");
}

/// The value of the first group `code` in the record of kind `kind` of the entities section.
fn group(d: &Drawing, kind: &str, code: i32) -> Option<String> {
    let text = cadcraft_io::write(d, "x.dxf").unwrap();
    let tags = cadcraft_dxf::parse(&text).unwrap();
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").unwrap();
    let (_, record) = cadcraft_dxf::records(&section.tags).into_iter().find(|(k, _)| k == kind)?;
    record.iter().find(|t| t.code == code).map(|t| t.str().trim().to_string())
}

#[test]
fn text_made_in_cadcraft_keeps_the_old_defaults() {
    let mut d = Drawing::new_metric();
    let t = MText { drawing_direction: None, ..mtext(&cadcraft_io::read_dxf(&fixture()).unwrap()).clone() };
    d.add(&cadcraft_doc::Space::Model, cadcraft_doc::Common::default(), EntityKind::MText(t)).unwrap();
    assert_eq!(group(&d, "MTEXT", 72).as_deref(), Some("1"), "left to right");
}

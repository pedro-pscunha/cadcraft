//! Plan v3, 4.1: the read turns every record of a Raw component into the raw entity of 4.2, and reads the others Legacy. The
//! need of a record is the stand-in of `activation_fixtures` until the section 3 predicate exists.

use cadcraft_doc::{Drawing, EntityKind, GeometryRevision, Handle};
use cadcraft_io::activation::{FamilySet, ReaderMode, ReaderPolicy};

use crate::activation_fixtures::{cases, stand_in};

fn raw_path() -> ReaderPolicy {
    ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE).with_raw_path()
}

fn fixture(name: &str) -> String {
    cases().into_iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name}")).text
}

fn read(text: &str, policy: ReaderPolicy) -> Drawing {
    cadcraft_io::read_with_need(text.as_bytes(), "x.dxf", policy, &stand_in).unwrap()
}

fn raw_kinds(d: &Drawing) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let stores = std::iter::once(&d.model).chain(d.blocks.values().map(|b| &b.entities));
    for store in stores {
        for e in store.iter() {
            if let EntityKind::Raw(raw) = &e.kind {
                out.push((raw.parent().kind().to_string(), raw.followers().iter().map(|f| f.kind().to_string()).collect()));
            }
        }
    }
    out
}

fn legacy_count(d: &Drawing) -> usize {
    let stores = std::iter::once(&d.model).chain(d.blocks.values().map(|b| &b.entities));
    stores.flat_map(|s| s.iter()).filter(|e| e.geometry_revision == GeometryRevision::LegacyProjectedV1).count()
}

#[test]
fn a_needing_member_of_a_definition_that_cannot_activate_is_raw_and_its_sibling_is_legacy() {
    // B4: before stage 6 a definition cannot activate; the tilted CIRCLE is kept Raw, the LINE is read Legacy.
    let d = read(&fixture("b4_unreferenced_definition_with_needing_member"), raw_path());
    let members: Vec<_> = d.block("DEF").unwrap().entities.iter().collect();
    assert_eq!(members.len(), 2);
    assert!(matches!(&members[0].kind, EntityKind::Raw(raw) if raw.parent().kind() == "CIRCLE" && raw.parent().handle() == Handle(0xB1)));
    assert_eq!(members[0].geometry_revision, GeometryRevision::RawSource);
    assert!(matches!(members[1].kind, EntityKind::Line(_)));
    assert_eq!(members[1].geometry_revision, GeometryRevision::LegacyProjectedV1);
}

#[test]
fn a_needing_dimension_is_raw_and_its_graphics_block_stays_legacy() {
    let d = read(&fixture("b5_dimension_with_star_d_block"), raw_path());
    assert_eq!(raw_kinds(&d), [("DIMENSION".to_string(), Vec::<String>::new())]);
    assert!(d.block("*D1").unwrap().entities.iter().all(|e| matches!(e.kind, EntityKind::Line(_))));
}

#[test]
fn a_needing_insert_with_attributes_is_one_raw_unit_of_four_records() {
    let d = read(&fixture("b2_needs_insert_with_attributes_raw"), raw_path());
    assert_eq!(raw_kinds(&d), [("INSERT".to_string(), vec!["ATTRIB".to_string(), "ATTRIB".to_string(), "SEQEND".to_string()])]);
    assert_eq!(d.model.iter().count(), 1, "the ATTRIBs and the SEQEND are not entities of their own");
    assert!(d.block("DEF").unwrap().entities.iter().all(|e| matches!(e.kind, EntityKind::Line(_))));
}

#[test]
fn a_needing_polyline_is_one_raw_unit_with_its_vertices() {
    let polyline = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n5\nA1\n8\n0\n66\n1\n70\n0\n210\n0\n220\n1\n230\n1\n\
0\nVERTEX\n5\nA2\n8\n0\n10\n0\n20\n0\n30\n0\n0\nVERTEX\n5\nA3\n8\n0\n10\n1\n20\n1\n30\n0\n0\nSEQEND\n5\nA4\n8\n0\n\
0\nLINE\n5\nA5\n8\n0\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n0\nENDSEC\n0\nEOF\n";
    let d = read(polyline, raw_path());
    assert_eq!(raw_kinds(&d), [("POLYLINE".to_string(), vec!["VERTEX".to_string(), "VERTEX".to_string(), "SEQEND".to_string()])]);
    assert_eq!(legacy_count(&d), 1, "the LINE");
}

#[test]
fn each_raw_unit_leaves_a_read_note_with_its_handle_and_the_reason() {
    let d = read(&fixture("b4_unreferenced_definition_with_needing_member"), raw_path());
    let notes: Vec<_> = d.read_notes.iter().filter(|n| n.contains("kept as read")).collect();
    assert_eq!(notes.len(), 1, "{:?}", d.read_notes);
    assert!(notes[0].contains("CIRCLE B1") && notes[0].contains("block definition"), "{}", notes[0]);
}

#[test]
fn without_the_raw_path_every_record_is_read_legacy_as_before() {
    for name in ["b4_unreferenced_definition_with_needing_member", "b2_needs_insert_with_attributes_raw", "b5_dimension_with_star_d_block"] {
        let text = fixture(name);
        let plain = cadcraft_io::read(text.as_bytes(), "x.dxf").unwrap();
        assert!(raw_kinds(&plain).is_empty(), "{name}");
        // The stand-in alone changes nothing: the graph is not built.
        let with_need = read(&text, ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE));
        assert_eq!(with_need, plain, "{name}");
    }
}

#[test]
fn a_file_with_no_needing_record_reads_the_same_with_the_raw_path() {
    let text = fixture("a10_cycle_all_plain");
    assert_eq!(read(&text, raw_path()), read(&text, ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE)));
}

#[test]
fn a_unit_that_cannot_be_kept_raw_is_read_legacy_with_a_note() {
    // A needing INSERT with an ATTRIB and no closing SEQEND: a raw compound needs the SEQEND.
    let text = "0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nA1\n8\n0\n66\n1\n2\nNOPE\n10\n0\n20\n0\n30\n0\n210\n0\n220\n1\n230\n1\n\
0\nATTRIB\n5\nA2\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n1\nv\n2\nT\n0\nENDSEC\n0\nEOF\n";
    let d = read(text, raw_path());
    assert!(raw_kinds(&d).is_empty());
    assert_eq!(legacy_count(&d), 1);
    assert!(d.read_notes.iter().any(|n| n.contains("cannot be kept raw")), "{:?}", d.read_notes);
}

#[test]
fn a_needing_unit_read_legacy_because_it_cannot_be_raw_is_named_in_every_save() {
    // The same unit as above: a needing INSERT with an ATTRIB and no closing SEQEND. It is read Legacy with a read note, and every
    // later save says again that it needed a canonical reading, so no save goes out silent about it.
    let text = "0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nA1\n8\n0\n66\n1\n2\nNOPE\n10\n0\n20\n0\n30\n0\n210\n0\n220\n1\n230\n1\n\
0\nATTRIB\n5\nA2\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n1\nv\n2\nT\n0\nENDSEC\n0\nEOF\n";
    let mut d = read(text, raw_path());
    for name in ["x.dxf", "x.dwg"] {
        let (_, notes) = cadcraft_io::write_with_noted(&d, name, &cadcraft_io::SaveOptions::default()).unwrap();
        assert!(notes.iter().any(|n| n.contains("INSERT A1") && n.contains("read Legacy")), "{name}: {notes:?}");
    }
    // Once the record is gone from the drawing, the save has nothing to say about it.
    d.remove_entity(Handle(0xA1));
    let (_, notes) = cadcraft_io::write_with_noted(&d, "x.dxf", &cadcraft_io::SaveOptions::default()).unwrap();
    assert!(!notes.iter().any(|n| n.contains("INSERT A1")), "{notes:?}");
    // A record that is Legacy because nothing needs more says nothing.
    let plain = read(&fixture("a10_cycle_all_plain"), raw_path());
    let (_, notes) = cadcraft_io::write_with_noted(&plain, "x.dxf", &cadcraft_io::SaveOptions::default()).unwrap();
    assert!(!notes.iter().any(|n| n.contains("read Legacy")), "{notes:?}");
}

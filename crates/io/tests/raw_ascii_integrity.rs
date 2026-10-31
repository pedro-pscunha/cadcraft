use std::sync::Arc;

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, Line, RawCompound, RawRecord, RawTag};
use cadcraft_geom::Vec3;

#[path = "support/raw_compounds.rs"]
mod support;

fn line(handle: Handle) -> Entity {
    Entity::new(handle, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 2.0, 3.0) }))
}

#[test]
fn a_raw_parent_uses_the_written_containing_space_instead_of_its_old_owner() {
    for owner in [None, Some("1F")] {
        let mut drawing = support::drawing("INSERT");
        let EntityKind::Raw(raw) = &drawing.entity(Handle(0x200)).unwrap().kind else { panic!("Raw INSERT") };
        let mut tags = raw.parent().tags().to_vec();
        tags.retain(|tag| !(tag.code == 330 && Handle::parse_hex(&tag.value) == Some(Handle(0))));
        if let Some(owner) = owner {
            tags.insert(1, RawTag { code: 330, value: owner.into() });
        }
        let parent = RawRecord::new(raw.parent().kind().into(), tags).unwrap();
        let raw = RawCompound::new(parent, raw.followers().to_vec(), raw.source_common().clone(), raw.reason().into()).unwrap();
        drawing.model.replace(Entity::from_raw(raw));
        assert!(cadcraft_io::write(&drawing, "owner.dxf").is_ok());
    }
}

#[test]
fn ascii_refuses_missing_named_targets_and_changed_common_invariants() {
    for common in [false, true] {
        let mut drawing = support::drawing("INSERT");
        if common {
            let mut entity = drawing.entity(Handle(0x200)).unwrap().as_ref().clone();
            entity.common.extrusion = Vec3::new(0.0, 0.0, 1.0);
            drawing.model.replace(entity);
        } else {
            drawing.blocks.remove("Symbol").unwrap();
        }
        assert!(matches!(cadcraft_io::write(&drawing, "invalid.dxf"), Err(cadcraft_io::IoError::RawPreservation { .. })));
    }
}

#[test]
fn ascii_refuses_owned_metadata_references_that_remapping_discards() {
    let mut drawing = support::drawing("INSERT");
    Arc::make_mut(&mut drawing.foreign).objects.retain(|object| object.handle != Handle(0x501));
    assert!(matches!(cadcraft_io::write(&drawing, "metadata.dxf"), Err(cadcraft_io::IoError::RawPreservation { .. })));
}

#[test]
fn ascii_does_not_bless_an_embedded_pointer_as_the_parent_owner() {
    let tags = [(5, "200"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbPrivateRecord"), (101, "Embedded Object"), (330, "90")]
        .into_iter()
        .map(|(code, value)| RawTag { code, value: value.into() })
        .collect();
    let raw =
        RawCompound::new(RawRecord::new("PRIVATE_RECORD".into(), tags).unwrap(), vec![], Common::default(), "Uninterpreted embedded source".into())
            .unwrap();
    let mut drawing = Drawing::new_metric();
    drawing.model.push(Entity::from_raw(raw));
    drawing.model.push(line(Handle(0x90)));
    assert!(matches!(cadcraft_io::write(&drawing, "embedded.dxf"), Err(cadcraft_io::IoError::RawPreservation { .. })));
}

#[test]
fn ascii_keeps_literal_unicode_escape_text_after_one_wire_decode() {
    for value in [r"caf\U+00E9", "café"] {
        let mut drawing = support::drawing("INSERT");
        let EntityKind::Raw(raw) = &drawing.entity(Handle(0x200)).unwrap().kind else { panic!("Raw INSERT") };
        let followers = raw
            .followers()
            .iter()
            .map(|record| {
                let mut tags = record.tags().to_vec();
                if record.handle() == Handle(0x201) {
                    tags.iter_mut().find(|tag| tag.code == 1).unwrap().value = value.into();
                }
                RawRecord::new(record.kind().into(), tags).unwrap()
            })
            .collect();
        let raw = RawCompound::new(raw.parent().clone(), followers, raw.source_common().clone(), raw.reason().into()).unwrap();
        drawing.model.replace(Entity::from_raw(raw));
        let control = cadcraft_io::write_dxf(&drawing);
        let tags = cadcraft_dxf::parse(control.as_bytes()).unwrap();
        assert!(tags.iter().any(|tag| tag.code == 1 && tag.str() == value));
        let bytes = cadcraft_io::write(&drawing, "unicode.dxf").expect("the parsed Raw string must retain its value after one wire decode");
        let tags = cadcraft_dxf::parse(&bytes).unwrap();
        assert!(tags.iter().any(|tag| tag.code == 1 && tag.str() == value));
    }
}

#[test]
fn ascii_refuses_a_required_reference_to_an_erased_entity() {
    let mut drawing = support::drawing("INSERT");
    let EntityKind::Raw(raw) = &drawing.entity(Handle(0x200)).unwrap().kind else { panic!("Raw INSERT") };
    let mut tags = raw.parent().tags().to_vec();
    let at = tags.iter().position(|tag| tag.code == 1001).unwrap();
    tags.insert(at, RawTag { code: 340, value: "A0".into() });
    let parent = RawRecord::new(raw.parent().kind().into(), tags).unwrap();
    let raw = RawCompound::new(parent, raw.followers().to_vec(), raw.source_common().clone(), raw.reason().into()).unwrap();
    drawing.model.replace(Entity::from_raw(raw));
    drawing.model.push(line(Handle(0xA0)));
    assert!(cadcraft_io::write(&drawing, "valid.dxf").is_ok());
    drawing.remove_entity(Handle(0xA0)).unwrap();
    match cadcraft_io::write(&drawing, "dangling.dxf") {
        Err(cadcraft_io::IoError::RawPreservation { unit, record, handle, field, .. }) => {
            assert_eq!((unit, record.as_str(), handle), (Handle(0x200), "INSERT", Handle(0x200)));
            assert!(field.contains("340"), "{field}");
        }
        result => panic!("ASCII must refuse the dangling required reference: {:?}", result.map(|bytes| bytes.len())),
    }
}

#[test]
fn ascii_refuses_parent_and_follower_identity_collisions_across_spaces() {
    for handle in [Handle(0x200), Handle(0x201)] {
        let mut drawing = support::drawing("INSERT");
        let raw = drawing.model.remove(Handle(0x200)).unwrap();
        Arc::make_mut(drawing.blocks.get_mut("Symbol").unwrap()).entities.push(raw.as_ref().clone());
        drawing.model.push(line(handle));
        match cadcraft_io::write(&drawing, "ambiguous.dxf") {
            Err(cadcraft_io::IoError::RawPreservation { unit, handle: refused, .. }) => {
                assert_eq!(unit, Handle(0x200));
                assert_eq!(refused, handle);
            }
            result => panic!("ASCII must refuse ambiguous identity {handle}: {:?}", result.map(|bytes| bytes.len())),
        }
    }
}

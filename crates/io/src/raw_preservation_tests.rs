use std::io::{Cursor, Write};

use acadrust::entities::{AttributeEntity, EntityType};
use acadrust::xdata::XDataValue;
use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, RawCompound, RawRecord, RawTag};
use cadcraft_dxf::{Tag, records, sections};

#[path = "../tests/support/raw_compounds.rs"]
mod fixtures;

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Value,
    Tag,
    RemoveXdata,
    ChangeXdata,
    Style,
    Reactor,
    Order,
}

fn drawing() -> Drawing {
    let mut drawing = fixtures::drawing("INSERT");
    let mut style = drawing.text_styles.first().unwrap().clone();
    style.name = "ChangedStyle".into();
    drawing.text_styles.push(style);
    drawing
}

fn record_list(bytes: &[u8]) -> Vec<(String, Vec<Tag>)> {
    let dxf = cadcraft_dwg::dwg_to_dxf(bytes).unwrap();
    let tags = cadcraft_dxf::parse(&dxf).unwrap();
    sections(&tags)
        .into_iter()
        .filter(|section| section.name == "ENTITIES")
        .flat_map(|section| records(&section.tags))
        .filter(|(_, tags)| {
            tags.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()).is_some_and(|handle| (0x200..=0x203).contains(&handle.0)))
        })
        .collect()
}

fn numeric_projection(records: &[(String, Vec<Tag>)]) -> Vec<(u64, Vec<(i32, String)>)> {
    let mut result: Vec<_> = records
        .iter()
        .map(|(_, tags)| {
            let handle = Handle::parse_hex(&tags.iter().find(|tag| tag.code == 5).unwrap().str()).unwrap().0;
            let values = tags
                .iter()
                .filter(|tag| matches!(tag.value, cadcraft_dxf::Value::Real(_) | cadcraft_dxf::Value::Int(_) | cadcraft_dxf::Value::Bool(_)))
                .map(|tag| (tag.code, tag.str()))
                .collect();
            (handle, values)
        })
        .collect();
    result.sort_by_key(|(handle, _)| *handle);
    result
}

fn change_attribute(attribute: &mut AttributeEntity, mutation: Mutation) {
    attribute.common.raw_record = None;
    match mutation {
        Mutation::Value => attribute.value = "Changed value".into(),
        Mutation::Tag => attribute.tag = "CHANGED_TAG".into(),
        Mutation::Style => attribute.text_style = "ChangedStyle".into(),
        Mutation::RemoveXdata | Mutation::ChangeXdata => {
            let mut record = attribute.common.extended_data.remove_record("RAW_APP").unwrap();
            if matches!(mutation, Mutation::RemoveXdata) {
                record.values.retain(|value| !matches!(value, XDataValue::String(_)));
            } else {
                let value = record.values.iter_mut().find(|value| matches!(value, XDataValue::String(_))).unwrap();
                *value = XDataValue::String("Changed xdata string".into());
            }
            attribute.common.extended_data.raw_dwg_eed.clear();
            attribute.common.extended_data.upsert_record(record);
        }
        Mutation::Reactor | Mutation::Order => {}
    }
}

fn mutate(bytes: Vec<u8>, mutation: Mutation) -> Vec<u8> {
    let before = record_list(&bytes);
    let mut document = acadrust::DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    for entity in document.entities_mut() {
        match entity {
            EntityType::Insert(insert) if insert.common.handle.value() == 0x200 => {
                if matches!(mutation, Mutation::Reactor) {
                    insert.common.reactors.clear();
                }
                if matches!(mutation, Mutation::Order) {
                    insert.attributes.swap(0, 1);
                }
                for attribute in &mut insert.attributes {
                    if attribute.common.handle.value() == 0x201 {
                        change_attribute(attribute, mutation);
                    }
                }
            }
            EntityType::Polyline2D(polyline) if polyline.common.handle.value() == 0x200 && matches!(mutation, Mutation::Order) => {
                polyline.vertices.swap(0, 1)
            }
            EntityType::AttributeEntity(attribute) if attribute.common.handle.value() == 0x201 => change_attribute(attribute, mutation),
            _ => {}
        }
    }
    let changed = acadrust::DwgWriter::write_to_vec(&document).unwrap();
    let after = record_list(&changed);
    assert_eq!(before.len(), after.len(), "{mutation:?} changed record count");
    assert_eq!(numeric_projection(&before), numeric_projection(&after), "{mutation:?} changed an identity or numeric field");
    assert_ne!(before, after, "{mutation:?} did not reach the encoded bytes");
    changed
}

#[test]
fn all_non_numeric_raw_corruption_refuses_and_leaves_the_output_file_unchanged() {
    let drawing = drawing();
    for mutation in
        [Mutation::Value, Mutation::Tag, Mutation::RemoveXdata, Mutation::ChangeXdata, Mutation::Style, Mutation::Reactor, Mutation::Order]
    {
        let drawing = if matches!(mutation, Mutation::Order) { fixtures::drawing("POLYLINE") } else { drawing.clone() };
        let path = std::env::temp_dir().join(format!("cadcraft-raw-guard-{}-{mutation:?}.dwg", std::process::id()));
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.write_all(b"existing target must not change").unwrap();
        drop(file);
        let result = crate::write_dwg_with(&drawing, crate::Version::R2018, |bridge, page| {
            let (bytes, notes) = cadcraft_dwg::dxf_to_dwg_as_noted(bridge, "AC1032", page)?;
            Ok((mutate(bytes, mutation), notes))
        });
        if let Ok((bytes, _)) = &result {
            std::fs::write(&path, bytes).unwrap();
        }
        let target = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_err(), "{mutation:?} published corrupted DWG bytes");
        assert_eq!(target, b"existing target must not change");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("200") && error.contains("field"), "{mutation:?}: {error}");
    }
}

#[test]
fn a_changed_plot_style_is_checked_against_the_model() {
    let mut drawing = fixtures::drawing("INSERT");
    drawing.model.modify_metadata(Handle(0x200), |common| common.plot_style = "NamedStyle".into()).unwrap();
    assert!(crate::write(&drawing, "plot-style.dwg").is_ok());
    let result = crate::write_dwg_with(&drawing, crate::Version::R2018, |bridge, page| {
        let (bytes, notes) = cadcraft_dwg::dxf_to_dwg_as_noted(bridge, "AC1032", page)?;
        let mut document = acadrust::DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
        for entity in document.entities_mut() {
            if let EntityType::Insert(insert) = entity {
                insert.common.plotstyle_flags = 0;
                insert.common.plotstyle_handle = None;
            }
        }
        Ok((acadrust::DwgWriter::write_to_vec(&document).unwrap(), notes))
    });
    let error = result.expect_err("the named plot style must not disappear").to_string();
    assert!(error.contains("200") && error.contains("plot style"), "{error}");
}

#[test]
fn arbitrary_handles_are_not_remapped_as_object_references() {
    let source = vec![Tag::s(320, "90"), Tag::s(1005, "90")];
    let mut actual = Vec::new();
    crate::dxf_write::remap_raw_references(&source, |_| Some("91".into()), &mut actual);
    assert_eq!(actual[0].str(), "90");
    assert_eq!(actual[1].str(), "91");
}

#[test]
fn binary_dxf_refuses_raw_units_instead_of_replacing_source_number_text() {
    let drawing = fixtures::drawing("INSERT");
    let options = crate::SaveOptions { binary: true, version: None };
    let result = crate::write_with(&drawing, "raw.dxf", &options);
    assert!(result.is_err(), "binary DXF cannot preserve source numeric spelling");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("200") && error.contains("binary DXF"), "{error}");
}

#[test]
fn unexposed_raw_fields_remain_in_dxf_but_refuse_dwg() {
    for kind in ["INSERT", "POLYLINE"] {
        let mut drawing = fixtures::drawing(kind);
        let EntityKind::Raw(source) = &drawing.entity(Handle(0x200)).unwrap().kind else { panic!("raw") };
        let mut tags = source.parent().tags().to_vec();
        let at = tags.iter().position(|tag| tag.code == 1001).unwrap();
        tags.insert(at, RawTag { code: 318, value: "CAFE".into() });
        let parent = RawRecord::new(kind.into(), tags).unwrap();
        let raw = RawCompound::new(parent, source.followers().to_vec(), source.source_common().clone(), source.reason().into()).unwrap();
        assert!(drawing.model.replace(Entity::from_raw(raw)));
        let dxf = crate::write(&drawing, "unexposed.dxf").unwrap();
        assert!(String::from_utf8(dxf).unwrap().lines().any(|line| line == "CAFE"));
        let error = crate::write(&drawing, "unexposed.dwg").expect_err("the raw field must not disappear").to_string();
        assert!(error.contains("200") && error.contains("318"), "{kind}: {error}");
    }
}

#[derive(Clone, Copy, Debug)]
enum PayloadMutation {
    Number,
    MissingFollower,
    MetadataValue,
    MetadataOrder,
}

#[test]
fn numeric_follower_and_owned_metadata_corruption_refuse() {
    for mutation in [PayloadMutation::Number, PayloadMutation::MissingFollower, PayloadMutation::MetadataValue, PayloadMutation::MetadataOrder] {
        let drawing = fixtures::drawing("POLYLINE");
        let result = crate::write_dwg_with(&drawing, crate::Version::R2018, |bridge, page| {
            let (bytes, notes) = cadcraft_dwg::dxf_to_dwg_as_noted(bridge, "AC1032", page)?;
            let mut document = acadrust::DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
            let mut changed = false;
            for entity in document.entities_mut() {
                if let EntityType::Polyline2D(polyline) = entity {
                    match mutation {
                        PayloadMutation::Number => {
                            polyline.vertices[0].location.x += 0.125;
                            changed = true;
                        }
                        PayloadMutation::MissingFollower => {
                            polyline.vertices.pop();
                            changed = true;
                        }
                        PayloadMutation::MetadataValue | PayloadMutation::MetadataOrder => {}
                    }
                }
            }
            for object in document.objects.values_mut() {
                if let acadrust::objects::ObjectType::XRecord(record) = object
                    && record.entries.iter().any(|entry| entry.value == acadrust::objects::XRecordValue::String("preserved metadata".into()))
                {
                    match mutation {
                        PayloadMutation::MetadataValue => {
                            record.entries[0] = acadrust::objects::XRecordEntry::string(1, "Changed metadata");
                            changed = true;
                        }
                        PayloadMutation::MetadataOrder => {
                            record.entries.swap(0, 1);
                            changed = true;
                        }
                        PayloadMutation::Number | PayloadMutation::MissingFollower => {}
                    }
                    record.raw_dwg_data = None;
                    record.raw_data.clear();
                }
            }
            assert!(changed, "{mutation:?} did not reach the native model");
            Ok((acadrust::DwgWriter::write_to_vec(&document).unwrap(), notes))
        });
        let error = result.expect_err("the corrupted raw payload must refuse").to_string();
        assert!(error.contains("200") && error.contains("field"), "{mutation:?}: {error}");
    }
}

#[test]
fn numeric_wire_equivalence_does_not_hide_changed_bits_or_repeated_defaults() {
    let numeric = |value: &str| super::fields::parse(&[RawTag { code: 40, value: value.into() }], true).unwrap();
    assert_eq!(numeric("1.0"), numeric("1"));
    assert_ne!(numeric("1.0"), numeric("1.0000000000000002"));
    assert_ne!(numeric("0"), numeric("-0"));
    let source = super::fields::parse(
        &[RawTag { code: 100, value: "AcDbBlockReference".into() }, RawTag { code: 41, value: "1".into() }, RawTag { code: 41, value: "1".into() }],
        true,
    )
    .unwrap();
    let repeated = source.keys().find(|field| field.code == 41 && field.index == 1).unwrap();
    assert!(super::fields::default_value("INSERT", repeated, &source).is_none());
}

#[test]
fn metadata_field_order_cannot_hide_behind_per_code_indices() {
    for prefix in [vec![(1001, "RAW_APP")], vec![(100, "AcDbXrecord")], vec![(102, "{PRIVATE")]] {
        let suffix = if prefix[0].0 == 102 { vec![(102, "}")] } else { Vec::new() };
        let a: Vec<_> = prefix
            .iter()
            .copied()
            .chain([(1000, "value"), (1070, "7")])
            .chain(suffix.iter().copied())
            .map(|(code, value)| RawTag { code, value: value.into() })
            .collect();
        let b: Vec<_> = prefix
            .iter()
            .copied()
            .chain([(1070, "7"), (1000, "value")])
            .chain(suffix.iter().copied())
            .map(|(code, value)| RawTag { code, value: value.into() })
            .collect();
        assert_ne!(super::fields::parse(&a, true).unwrap(), super::fields::parse(&b, true).unwrap());
    }
}

#[test]
fn hostile_metadata_depth_and_path_expansion_refuse_before_allocation() {
    let depth = cadcraft_doc::foreign::MAX_FOREIGN_DEPTH + 1;
    let tags: Vec<_> = std::iter::repeat_n(RawTag { code: 102, value: "{PRIVATE".into() }, depth)
        .chain(std::iter::repeat_n(RawTag { code: 102, value: "}".into() }, depth))
        .collect();
    assert!(super::fields::parse(&tags, true).is_err());
    let tags = [RawTag { code: 100, value: "X".repeat(1 << 20) }];
    let mut expanded = tags.to_vec();
    expanded.extend(std::iter::repeat_n(RawTag { code: 70, value: "0".into() }, 65));
    assert!(super::fields::parse(&expanded, true).is_err());
}

#[test]
fn ambiguous_follower_or_metadata_identities_refuse_before_native_encoding() {
    for handle in [Handle(0x201), Handle(0x501)] {
        let mut drawing = fixtures::drawing("INSERT");
        drawing.model.push(Entity::new(
            handle,
            EntityKind::Line(cadcraft_doc::Line { a: cadcraft_geom::Vec3::ZERO, b: cadcraft_geom::Vec3::new(1.0, 0.0, 0.0) }),
        ));
        let mut encoded = false;
        let result = crate::write_dwg_with(&drawing, crate::Version::R2018, |_, _| {
            encoded = true;
            Err("the ambiguous input must not reach native encoding".into())
        });
        assert!(!encoded);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("200") && error.contains(&handle.hex()) && error.contains("field"), "{error}");
    }
}

#[test]
fn raw_reference_checks_use_the_final_kept_symbol_handles() {
    for kind in ["INSERT", "POLYLINE"] {
        let mut drawing = fixtures::drawing(kind);
        let symbols = std::sync::Arc::make_mut(&mut drawing.source_symbols);
        for (role, name, handle) in [
            ("BLOCK_RECORD", "*MODEL_SPACE", 0x7000),
            ("STYLE", "STANDARD", 0x7001),
            ("LAYER", "0", 0x7002),
            ("NOD/ACAD_PLOTSTYLENAME", "NAMEDSTYLE", 0x7003),
        ] {
            symbols.handles.insert((role.into(), name.into()), Handle(handle));
        }
        drawing.model.modify_metadata(Handle(0x200), |common| common.plot_style = "NamedStyle".into()).unwrap();
        let bytes = crate::write(&drawing, "kept-symbols.dwg").unwrap();
        let inspection = cadcraft_dwg::inspect_dwg(&bytes, cadcraft_dwg::Limits::DEFAULT).unwrap();
        for handle in [0x7000, 0x7001, 0x7002, 0x7003] {
            assert!(inspection.handles.contains(&handle), "{kind}: missing kept handle {handle:X}");
        }
    }
}

#[test]
fn final_source_handle_restoration_does_not_translate_arbitrary_handles() {
    let mut drawing = Drawing::new_metric();
    std::sync::Arc::make_mut(&mut drawing.source_symbols).handles.insert(("STYLE".into(), "STANDARD".into()), Handle(0x7001));
    let mut tags = cadcraft_dxf::parse(
        b"0\nSECTION\n2\nTABLES\n0\nSTYLE\n5\n100\n2\nStandard\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\n200\n320\n100\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    let remapped = crate::dxf_raw::restore_source_handles(&mut tags, &drawing);
    assert_eq!(remapped.get(&Handle(0x100)), Some(&Handle(0x7001)));
    assert_eq!(tags.iter().find(|tag| tag.code == 320).unwrap().str(), "100");
}

#[test]
fn an_unmodified_native_control_passes_the_same_runtime_check() {
    for kind in ["INSERT", "POLYLINE"] {
        let drawing = fixtures::drawing(kind);
        let (bytes, _) =
            crate::write_dwg_with(&drawing, crate::Version::R2018, |bridge, page| cadcraft_dwg::dxf_to_dwg_as_noted(bridge, "AC1032", page)).unwrap();
        assert!(!bytes.is_empty());
    }
}

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

fn attribute_version_groups(bytes: &[u8]) -> (String, Vec<i64>) {
    let parsed = cadcraft_dxf::parse(bytes).unwrap();
    let sections = sections(&parsed);
    let header = sections.iter().find(|section| section.name == "HEADER").unwrap();
    let version = header.tags.windows(2).find(|pair| pair[0].code == 9 && pair[0].str() == "$ACADVER").unwrap()[1].str();
    let attribute = sections
        .iter()
        .filter(|section| section.name == "ENTITIES")
        .flat_map(|section| records(&section.tags))
        .find(|(kind, tags)| kind == "ATTRIB" && tags.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.str()) == Some(Handle(0x201))))
        .unwrap()
        .1;
    let mut in_attribute = false;
    let groups = attribute
        .iter()
        .filter_map(|tag| {
            if tag.code == 100 {
                in_attribute = tag.str() == "AcDbAttribute";
            }
            (in_attribute && tag.code == 280).then(|| tag.i64())
        })
        .collect();
    (version, groups)
}

#[test]
fn binary_inspection_promotes_pre_r2007_and_adds_only_the_documented_attribute_zero_fields() {
    let source = include_bytes!("../tests/data/activation/r5_raw_insert_with_full_tables.dxf");
    let need = |view: &crate::activation::records::RecordView<'_>| {
        if matches!(view.kind, "INSERT" | "ATTRIB") { crate::activation::Need::Needs { readable: true } } else { crate::activation::Need::Plain }
    };
    let policy = crate::activation::ReaderPolicy::new(crate::activation::ReaderMode::Off, crate::activation::FamilySet::NONE).with_raw_path();
    let drawing = crate::read_with_need(source, "source.dxf", policy, &need).unwrap();
    let bridge = crate::dxf_write::dwg_bridge(&drawing, cadcraft_dxf::Version::R2000);
    assert_eq!(attribute_version_groups(&bridge.bytes), ("AC1015".into(), vec![0, 0]), "the first bridge already carries both explicit zeros");
    let first = crate::write(&drawing, "first.dwg").unwrap();
    let ascii = cadcraft_dwg::dwg_to_dxf(&first).unwrap();
    let binary = cadcraft_dwg::inspect_dwg(&first, cadcraft_dwg::Limits::DEFAULT).unwrap().dxf;
    assert_eq!(attribute_version_groups(&ascii), ("AC1018".into(), vec![]), "normal reopen exposes the R2004 DWG actually written");
    assert_eq!(attribute_version_groups(&binary), ("AC1024".into(), vec![0, 0]), "only the inspection rendition promotes binary DXF to R2010");
    let reopened = crate::read_with_need(&first, "first.dwg", policy, &need).unwrap();
    let EntityKind::Raw(raw) = &reopened.entity(Handle(0x200)).unwrap().kind else { panic!("Raw INSERT") };
    let attribute = raw.records().find(|record| record.handle() == Handle(0x201)).unwrap();
    assert!(!attribute.tags().iter().any(|tag| tag.code == 280), "Raw admission must not add a version field");
    let bridge = crate::dxf_write::dwg_bridge(&reopened, crate::save_version(&reopened));
    assert_eq!(
        attribute_version_groups(&bridge.bytes),
        ("AC1015".into(), vec![]),
        "the second bridge uses the R2000 schema and does not add the fields either"
    );
    let second = crate::write(&reopened, "second.dwg").expect("the binary inspection's documented zero defaults must not prevent a second save");
    assert_eq!(attribute_version_groups(&cadcraft_dwg::dwg_to_dxf(&second).unwrap()), ("AC1018".into(), vec![]));
}

#[test]
fn nonzero_attribute_version_and_a_third_zero_occurrence_still_refuse() {
    for extra in [false, true] {
        let mut drawing = fixtures::drawing("INSERT");
        let EntityKind::Raw(raw) = &drawing.entity(Handle(0x200)).unwrap().kind else { panic!("Raw INSERT") };
        let followers = raw
            .followers()
            .iter()
            .map(|record| {
                let mut tags = record.tags().to_vec();
                if record.handle() == Handle(0x201) {
                    if extra {
                        let end = tags.iter().position(|tag| tag.code == 1001).unwrap_or(tags.len());
                        tags.insert(end, RawTag { code: 280, value: "0".into() });
                    } else {
                        tags.iter_mut().find(|tag| tag.code == 280).unwrap().value = "1".into();
                    }
                }
                RawRecord::new(record.kind().into(), tags).unwrap()
            })
            .collect();
        let unit = RawCompound::new(raw.parent().clone(), followers, raw.source_common().clone(), raw.reason().into()).unwrap();
        drawing.model.replace(Entity::from_raw(unit));
        let error = crate::write(&drawing, "invalid-version.dwg").expect_err("only the two documented zero defaults can be equivalent");
        match error {
            crate::IoError::RawPreservation { record, handle, field, .. } => {
                assert_eq!((record.as_str(), handle), ("ATTRIB", Handle(0x201)));
                assert!(field.contains(if extra { "group 280[2]" } else { "group 280[0]" }), "{field}");
            }
            error => panic!("unexpected error: {error}"),
        }
    }
}

#[test]
fn the_attribute_zero_rule_cannot_hide_a_changed_lock_bit() {
    let drawing = fixtures::drawing("INSERT");
    let result = crate::write_dwg_with(&drawing, crate::Version::R2018, |bridge, page| {
        let (bytes, notes) = cadcraft_dwg::dxf_to_dwg_as_noted(bridge, "AC1032", page)?;
        let mut document = acadrust::DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
        let mut changed = false;
        for entity in document.entities_mut() {
            if let EntityType::Insert(insert) = entity {
                for attribute in &mut insert.attributes {
                    if attribute.common.handle.value() == 0x201 {
                        attribute.common.raw_record = None;
                        attribute.lock_position = true;
                        changed = true;
                    }
                }
            }
        }
        assert!(changed);
        Ok((acadrust::DwgWriter::write_to_vec(&document).unwrap(), notes))
    });
    match result {
        Err(crate::IoError::RawPreservation { record, handle, field, .. }) => {
            assert_eq!((record.as_str(), handle), ("ATTRIB", Handle(0x201)));
            assert!(field.contains("group 280[1]"), "{field}");
        }
        result => panic!("a changed lock bit must refuse: {result:?}"),
    }
}

#[test]
fn attribute_version_equivalence_is_limited_to_two_zero_fields_in_the_own_subclass() {
    let source = super::fields::parse(
        &[
            RawTag { code: 100, value: "AcDbAttribute".into() },
            RawTag { code: 280, value: "0".into() },
            RawTag { code: 280, value: "0".into() },
            RawTag { code: 280, value: "0".into() },
        ],
        true,
    )
    .unwrap();
    for (field, value) in source.iter().filter(|(field, _)| field.code == 280) {
        let default = super::fields::default_value("ATTRIB", field, &source);
        assert_eq!(default, (field.index < 2).then_some(super::fields::Value::Integer(0)));
        if field.index < 2 {
            assert_eq!(default.as_ref(), Some(value));
        }
        assert!(super::fields::default_value("ATTDEF", field, &source).is_none());
    }
    for tags in [
        vec![(100, "AcDbText"), (280, "0")],
        vec![(100, "AcDbAttribute"), (102, "{PRIVATE"), (280, "0"), (102, "}")],
        vec![(100, "AcDbAttribute"), (1001, "RAW_APP"), (280, "0")],
        vec![(100, "AcDbAttribute"), (101, "Embedded Object"), (280, "0")],
    ] {
        let tags: Vec<_> = tags.into_iter().map(|(code, value)| RawTag { code, value: value.into() }).collect();
        let fields = super::fields::parse(&tags, true).unwrap();
        let field = fields.keys().find(|field| field.code == 280).unwrap();
        assert!(super::fields::default_value("ATTRIB", field, &fields).is_none());
    }
    let nonzero =
        super::fields::parse(&[RawTag { code: 100, value: "AcDbAttribute".into() }, RawTag { code: 280, value: "1".into() }], true).unwrap();
    let (field, value) = nonzero.iter().find(|(field, _)| field.code == 280).unwrap();
    assert_ne!(super::fields::default_value("ATTRIB", field, &nonzero).as_ref(), Some(value));
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

use std::sync::Arc;

use cadcraft_color::Color;
use cadcraft_doc::foreign::ForeignObject;
use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Line, RawCompound, RawRecord, RawTag};
use cadcraft_dxf::{Tag, records, sections};
use cadcraft_geom::{Mat4, Vec3};

fn tags(source: &str) -> Vec<RawTag> {
    source
        .lines()
        .map(|line| {
            let (code, value) = line.split_once(' ').unwrap();
            RawTag { code: code.parse().unwrap(), value: value.into() }
        })
        .collect()
}

fn record(kind: &str, handle: u64, owner: u64, own: &str, metadata: bool) -> RawRecord {
    let mut out = vec![RawTag { code: 5, value: format!("{handle:04X}") }];
    if metadata {
        out.extend(tags("102 {ACAD_REACTORS\n330 0090\n102 }\n102 {ACAD_XDICTIONARY\n360 0500\n102 }"));
    }
    out.extend(tags(&format!("330 {owner:04X}\n100 AcDbEntity\n8 0")));
    if kind != "SEQEND" {
        out.extend(tags("6 Continuous\n62 2\n48 2.5000"));
    }
    out.extend(tags(own));
    if kind != "SEQEND" {
        out.extend(tags(&format!("1001 RAW_APP\n1000 source {kind} {handle:X}\n1005 0200")));
    }
    RawRecord::new(kind.into(), out).unwrap()
}

fn raw(kind: &str) -> RawCompound {
    let (parent, followers) = if kind == "INSERT" {
        let parent = record(
            "INSERT",
            0x200,
            0,
            "100 AcDbBlockReference\n66 1\n2 Symbol\n10 001.2500\n20 2.5\n30 3.75\n41 1.0\n42 1.0\n43 1.0\n50 0.0\n210 0.0\n220 3.0\n230 4.0",
            true,
        );
        let attribute = |handle, text, tag| {
            record(
                "ATTRIB",
                handle,
                0x200,
                &format!(
                    "100 AcDbText\n10 001.2500\n20 2.5\n30 3.75\n40 1.5\n1 {text}\n50 0.0\n41 1.0\n7 Standard\n72 0\n100 AcDbAttribute\n280 0\n2 {tag}\n70 0\n73 0\n74 0\n280 0"
                ),
                false,
            )
        };
        (parent, vec![attribute(0x201, "First value", "TAG_A"), attribute(0x202, "Second value", "TAG_B"), record("SEQEND", 0x203, 0x200, "", false)])
    } else {
        let parent = record(
            "POLYLINE",
            0x200,
            0,
            "100 AcDb2dPolyline\n66 1\n10 0.0\n20 0.0\n30 2.0\n70 0\n40 0.125\n41 0.375\n210 0.0\n220 3.0\n230 4.0",
            true,
        );
        let vertex = |handle, x, y| {
            record(
                "VERTEX",
                handle,
                0x200,
                &format!("100 AcDbVertex\n100 AcDb2dVertex\n10 {x}\n20 {y}\n30 0.0\n40 0.125\n41 0.375\n42 0.25\n70 0"),
                false,
            )
        };
        (parent, vec![vertex(0x201, "001.2500", "2.5"), vertex(0x202, "10.0", "4.5"), record("SEQEND", 0x203, 0x200, "", false)])
    };
    RawCompound::new(
        parent,
        followers,
        Common { color: Color::Index(2), linetype: "Continuous".into(), ltscale: 2.5, extrusion: Vec3::new(0.0, 3.0, 4.0), ..Default::default() },
        "The source component cannot activate yet.".into(),
    )
    .unwrap()
}

fn drawing(kind: &str) -> Drawing {
    let mut d = Drawing::new_metric();
    d.registered_apps.push("RAW_APP".into());
    d.blocks.insert("Symbol".into(), Arc::new(Block::new("Symbol")));
    d.model.push(Entity::from_raw(raw(kind)));
    d.model.push(Entity::new(Handle(0x90), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })));
    let foreign = Arc::make_mut(&mut d.foreign);
    foreign.objects.push(ForeignObject {
        handle: Handle(0x500),
        kind: "DICTIONARY".into(),
        tags: tags("5 500\n330 200\n100 AcDbDictionary\n281 1\n3 RAW_DATA\n350 501"),
    });
    foreign.objects.push(ForeignObject {
        handle: Handle(0x501),
        kind: "XRECORD".into(),
        tags: tags("5 501\n330 500\n100 AcDbXrecord\n280 1\n1 preserved metadata\n40 1.125"),
    });
    d
}

fn section(bytes: &[u8], name: &str) -> Vec<(String, Vec<Tag>)> {
    let all = cadcraft_dxf::parse(bytes).unwrap();
    records(&sections(&all).into_iter().find(|s| s.name == name).unwrap().tags)
}

fn value(record: &[Tag], code: i32) -> String {
    record.iter().find(|t| t.code == code).map(Tag::str).unwrap()
}

fn assert_unit(dxf: &[u8], kind: &str, layer: &str, color: i64) {
    let all = section(dxf, "ENTITIES");
    let first = all.iter().position(|(_, tags)| tags.iter().any(|t| t.code == 5 && Handle::parse_hex(&t.str()) == Some(Handle(0x200))));
    assert!(first.is_some(), "raw parent disappeared");
    let unit: Vec<_> = all.iter().skip(first.unwrap()).take(4).collect();
    let follower = if kind == "INSERT" { "ATTRIB" } else { "VERTEX" };
    assert_eq!(unit.iter().map(|(kind, _)| kind.as_str()).collect::<Vec<_>>(), vec![kind, follower, follower, "SEQEND"]);
    for ((_, record), handle) in unit.iter().zip(0x200..=0x203) {
        assert_eq!(Handle::parse_hex(&value(record, 5)), Some(Handle(handle)));
        if handle != 0x200 {
            assert_eq!(Handle::parse_hex(&value(record, 330)), Some(Handle(0x200)));
        }
    }
    let tables = section(dxf, "TABLES");
    let model = tables
        .iter()
        .find(|(kind, rec)| kind == "BLOCK_RECORD" && rec.iter().any(|tag| tag.code == 2 && tag.str().eq_ignore_ascii_case("*Model_Space")))
        .unwrap();
    let model_handle = Handle::parse_hex(&value(&model.1, 5)).unwrap();
    assert_eq!(cadcraft_doc::foreign::object_owner(unit[0].1.iter().map(|tag| (tag.code, tag.str()))), Some(model_handle));
    assert_eq!(value(&unit[0].1, 8), layer);
    assert_eq!(unit[0].1.iter().find(|t| t.code == 62).unwrap().i64(), color);
    assert_eq!(value(&unit[0].1, 1000), format!("source {kind} 200"));
    for (index, (_, child)) in unit.iter().skip(1).take(2).enumerate() {
        assert_eq!(value(child, 8), "0");
        assert_eq!(child.iter().find(|t| t.code == 62).unwrap().i64(), 2);
        assert_eq!(value(child, 1000), format!("source {follower} {:X}", 0x201 + index));
        assert_eq!(Handle::parse_hex(&value(child, 1005)), Some(Handle(0x200)));
    }
    if kind == "INSERT" {
        assert_eq!(value(&unit[1].1, 1), "First value");
        assert_eq!(value(&unit[1].1, 2), "TAG_A");
        assert_eq!(value(&unit[2].1, 1), "Second value");
        assert_eq!(value(&unit[2].1, 2), "TAG_B");
    }
    assert!(unit[0].1.iter().any(|t| t.code == 330 && Handle::parse_hex(&t.str()) == Some(Handle(0x90))));
    let dictionary = Handle::parse_hex(&value(&unit[0].1, 360)).unwrap();
    let objects = section(dxf, "OBJECTS");
    let object =
        |handle| &objects.iter().find(|(_, tags)| tags.iter().any(|t| t.code == 5 && Handle::parse_hex(&t.str()) == Some(handle))).unwrap().1;
    let dict = object(dictionary);
    assert_eq!(Handle::parse_hex(&value(dict, 330)), Some(Handle(0x200)));
    assert_eq!(value(dict, 3), "RAW_DATA");
    let xrecord = object(Handle::parse_hex(&value(dict, 350)).unwrap());
    assert_eq!(Handle::parse_hex(&value(xrecord, 330)), Some(dictionary));
    assert_eq!(value(xrecord, 1), "preserved metadata");
    assert_eq!(xrecord.iter().find(|t| t.code == 40).unwrap().f64(), 1.125);
}

fn literal_record(bytes: &[u8], handle: Handle) -> (String, Vec<RawTag>) {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    let mut kind = String::new();
    let mut body: Vec<RawTag> = Vec::new();
    for pair in lines.chunks_exact(2) {
        let code = pair[0].trim().parse().unwrap();
        if code == 0 {
            if body.iter().any(|tag| tag.code == 5 && Handle::parse_hex(&tag.value) == Some(handle)) {
                return (kind, body);
            }
            kind = pair[1].into();
            body.clear();
        } else {
            body.push(RawTag { code, value: pair[1].into() });
        }
    }
    panic!("no source record {handle}")
}

#[test]
fn an_ascii_save_keeps_the_complete_ordered_source_sequence_and_numeric_spelling() {
    for kind in ["INSERT", "POLYLINE"] {
        let d = drawing(kind);
        let EntityKind::Raw(raw) = &d.entity(Handle(0x200)).unwrap().kind else { panic!("raw") };
        let bytes = cadcraft_io::write(&d, "literal.dxf").unwrap();
        for source in raw.records() {
            let (actual_kind, actual) = literal_record(&bytes, source.handle());
            assert_eq!(actual_kind, source.kind());
            assert_eq!(actual.len(), source.tags().len());
            let mut grouped = false;
            let mut owner = false;
            for (expected, actual) in source.tags().iter().zip(actual) {
                assert_eq!(actual.code, expected.code);
                if expected.code == 102 {
                    grouped = expected.value.starts_with('{');
                }
                let root_owner = expected.code == 330 && !grouped && !owner;
                owner |= root_owner;
                if (root_owner && source.handle() == Handle(0x200)) || expected.code == 360 {
                    assert!(Handle::parse_hex(&actual.value).is_some_and(|h| h.0 != 0));
                } else {
                    assert_eq!(actual, *expected, "{kind}: {} group {}", source.handle(), expected.code);
                }
            }
        }
    }
}

#[test]
fn erasing_a_raw_parent_erases_all_followers_and_owned_metadata() {
    for kind in ["INSERT", "POLYLINE"] {
        let mut d = drawing(kind);
        d.remove_entity(Handle(0x200)).unwrap();
        for name in ["erased.dxf", "erased.dwg"] {
            let bytes = cadcraft_io::write(&d, name).unwrap();
            let dxf = if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes };
            let entities = section(&dxf, "ENTITIES");
            for handle in 0x200..=0x203 {
                assert!(!entities.iter().any(|(_, rec)| rec.iter().any(|t| t.code == 5 && Handle::parse_hex(&t.str()) == Some(Handle(handle)))));
            }
            assert!(!section(&dxf, "OBJECTS").iter().any(|(_, rec)| rec.iter().any(|t| t.code == 1 && t.str() == "preserved metadata")));
            assert!(entities.iter().any(|(kind, _)| kind == "LINE"));
        }
    }
}

#[test]
fn raw_compounds_survive_unrelated_edits_in_dxf_and_dwg() {
    for kind in ["INSERT", "POLYLINE"] {
        let mut d = drawing(kind);
        let source = d.entity(Handle(0x200)).unwrap().clone();
        for edited in [false, true] {
            if edited {
                d.transform_entity(Handle(0x90), &Mat4::from_mat3(&cadcraft_geom::Mat3::translate(cadcraft_geom::Vec2::new(4.0, 5.0)))).unwrap();
            }
            for name in ["raw.dxf", "raw.dwg"] {
                let bytes = cadcraft_io::write(&d, name).unwrap();
                let dxf = if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes };
                assert_unit(&dxf, kind, "0", 2);
                if name.ends_with(".dxf") {
                    assert!(String::from_utf8(dxf).unwrap().lines().any(|line| line == "001.2500"));
                }
            }
            assert_eq!(d.entity(Handle(0x200)).unwrap(), &source);
        }
    }
}

#[test]
fn raw_parent_appearance_edits_do_not_touch_the_followers() {
    for kind in ["INSERT", "POLYLINE"] {
        let mut d = drawing(kind);
        d.ensure_layer("Edited");
        d.model
            .modify_metadata(Handle(0x200), |common| {
                common.layer = "Edited".into();
                common.color = Color::Index(7);
            })
            .unwrap();
        for name in ["props.dxf", "props.dwg"] {
            let bytes = cadcraft_io::write(&d, name).unwrap();
            let dxf = if name.ends_with(".dwg") { cadcraft_dwg::dwg_to_dxf(&bytes).unwrap() } else { bytes };
            assert_unit(&dxf, kind, "Edited", 7);
        }
    }
}

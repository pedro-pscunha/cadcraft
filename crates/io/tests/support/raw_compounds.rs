use cadcraft_color::Color;
use cadcraft_doc::foreign::ForeignObject;
use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Handle, Line, RawCompound, RawRecord, RawTag};
use cadcraft_geom::Vec3;
use std::sync::Arc;

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

pub(crate) fn drawing(kind: &str) -> Drawing {
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

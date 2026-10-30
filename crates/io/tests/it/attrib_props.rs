//! Properties set on one attribute of a block reference (an ATTRIB's layer, colour, linetype and
//! lineweight) survive a DXF save and reopen; untouched attributes stay untouched.

use std::sync::Arc;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::*;
use cadcraft_geom::Vec3;
use cadcraft_io::{read_dxf, write_dxf};

fn text(x: f64, value: &str) -> Text {
    Text {
        insert: Vec3::new(x, 0.0, 0.0),
        align_pt: None,
        height: 1.0,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
        backward: false,
        upside_down: false,
    }
}

fn attrib(tag: &str, x: f64, props: AttribProps) -> Attrib {
    Attrib { props, ..Attrib::new(tag, text(x, tag)) }
}

/// Block TAGS with definitions OWN (layer 0), DEFLAYER (layer TXT) and RED (layer 0, red),
/// inserted on layer INS with the given attributes.
fn drawing(attribs: Vec<Attrib>) -> (Drawing, Handle) {
    let mut d = Drawing::new_metric();
    for l in ["TXT", "INS", "A"] {
        d.ensure_layer(l);
    }
    let mut blk = Block::new("TAGS");
    for (i, (tag, common)) in [
        ("OWN", Common::default()),
        ("DEFLAYER", Common { layer: "TXT".into(), ..Common::default() }),
        ("RED", Common { color: Color::Index(1), ..Common::default() }),
    ]
    .into_iter()
    .enumerate()
    {
        let e = Entity::new(Handle(0x7000 + i as u64), EntityKind::AttDef(attrib(tag, 0.0, AttribProps::default())));
        blk.entities.push(Entity { common, ..e });
    }
    d.blocks.insert(blk.name.clone(), Arc::new(blk));
    let ins = Insert {
        block: "TAGS".into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs,
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    let h = d.add(&Space::Model, Common { layer: "INS".into(), ..Common::default() }, EntityKind::Insert(ins)).expect("add");
    (d, h)
}

fn reopened_attribs(d: &Drawing, h: Handle) -> Vec<Attrib> {
    let r = read_dxf(write_dxf(d).as_bytes()).expect("reopen");
    match &r.model.get(h).expect("insert kept").kind {
        EntityKind::Insert(i) => i.attribs.clone(),
        k => panic!("not an insert: {k:?}"),
    }
}

#[test]
fn attribute_properties_roundtrip() {
    let own = AttribProps {
        layer: Some("A".into()),
        color: Some(Color::True(Rgb(10, 20, 30))),
        linetype: Some("DASHED".into()),
        lineweight: Some(Lineweight::Mm100(50)),
        plot_style: Some("ByBlock".into()),
        visible: Some(false),
    };
    let red = AttribProps { color: Some(Color::ByLayer), ..AttribProps::default() };
    let (d, h) = drawing(vec![attrib("OWN", 0.0, own.clone()), attrib("DEFLAYER", 10.0, AttribProps::default()), attrib("RED", 20.0, red.clone())]);
    let a = reopened_attribs(&d, h);
    assert_eq!(a.len(), 3);
    assert_eq!(a[0].props, own);
    // Without properties of its own an ATTRIB is written with its definition's, so other readers
    // show it the same; reading those back gives the same drawing.
    assert_eq!(a[1].props.layer.as_deref(), Some("TXT"));
    assert_eq!((a[1].props.color, a[1].props.linetype.as_deref(), a[1].props.lineweight), (None, None, None));
    // ByLayer on an attribute whose definition is red stays ByLayer.
    assert_eq!(a[2].props.color, Some(Color::ByLayer));
}

#[test]
fn attributes_without_properties_stay_without() {
    let (d, h) = drawing(vec![attrib("OWN", 0.0, AttribProps::default()), attrib("NODEF", 10.0, AttribProps::default())]);
    let a = reopened_attribs(&d, h);
    assert_eq!(a.len(), 2);
    // The layer is always the attribute's own as the file has it; nothing else is set.
    assert!(a.iter().all(|a| AttribProps { layer: None, ..a.props.clone() }.is_empty()), "{a:?}");
}

#[test]
fn hostile_attrib_groups_do_not_panic() {
    let dxf = "0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\n20\n8\n0\n66\n1\n2\nNOPE\n10\n0\n20\n0\n30\n0\n\
0\nATTRIB\n5\n21\n8\n\n6\n\n62\n99999\n420\n-5\n370\n-99999\n10\n0\n20\n0\n30\n0\n40\n1\n1\nv\n2\nT\n70\n0\n\
0\nSEQEND\n5\n22\n0\nENDSEC\n0\nEOF\n";
    let d = read_dxf(dxf.as_bytes()).expect("read");
    let EntityKind::Insert(i) = &d.model.iter().next().expect("insert").kind else { panic!("not an insert") };
    let p = &i.attribs.first().expect("attrib").props;
    assert_eq!((p.layer.as_deref(), p.linetype.as_deref()), (None, None), "empty names are not properties");
    let _ = write_dxf(&d);
}

#[test]
fn an_attribute_on_layer_zero_and_one_hidden_by_itself_keep_both_in_dxf_and_dwg() {
    // OWN: an ATTRIB of a reference on layer INS that is itself on layer 0 (not the reference's) and hidden (group 60) although
    // its definition is shown; DEFLAYER: untouched.
    let own = AttribProps { layer: Some("0".into()), visible: Some(false), ..AttribProps::default() };
    let (d, h) = drawing(vec![attrib("OWN", 0.0, own.clone()), attrib("DEFLAYER", 10.0, AttribProps::default())]);
    let text = write_dxf(&d);
    let tags = cadcraft_dxf::parse(text.as_bytes()).expect("parse");
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES").expect("entities");
    let records = cadcraft_dxf::records(&section.tags);
    let attribs: Vec<&Vec<cadcraft_dxf::Tag>> = records.iter().filter(|(k, _)| k == "ATTRIB").map(|(_, t)| t).collect();
    let group = |t: &Vec<cadcraft_dxf::Tag>, code: i32| t.iter().find(|g| g.code == code).map(|g| g.str());
    assert_eq!((group(attribs[0], 8).as_deref(), group(attribs[0], 60).as_deref()), (Some("0"), Some("1")));
    assert_eq!((group(attribs[1], 8).as_deref(), group(attribs[1], 60)), (Some("TXT"), None));
    for reopened in [reopened_attribs(&d, h), {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
        match &back.model.iter().next().expect("insert").kind {
            EntityKind::Insert(i) => i.attribs.clone(),
            k => panic!("not an insert: {k:?}"),
        }
    }] {
        assert_eq!((reopened[0].props.layer.as_deref(), reopened[0].props.visible), (Some("0"), Some(false)), "{:?}", reopened[0].props);
        assert_eq!(reopened[1].props.visible, None);
    }
}

/// The layer (group 8) of the first ATTRIB of the DXF text.
fn attrib_layer(text: &str) -> Option<String> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).expect("parse");
    let section = cadcraft_dxf::sections(&tags).into_iter().find(|s| s.name == "ENTITIES")?;
    let records = cadcraft_dxf::records(&section.tags);
    let (_, attrib) = records.iter().find(|(k, _)| k == "ATTRIB")?;
    attrib.iter().find(|g| g.code == 8).map(|g| g.str())
}

#[test]
fn an_attribute_on_its_references_layer_keeps_it_when_its_definition_is_on_another_layer() {
    // The reference and the attribute are on INS; the definition (DEFLAYER) is on TXT. The file's layer is the attribute's own.
    let own = AttribProps { layer: Some("INS".into()), ..AttribProps::default() };
    let (d, h) = drawing(vec![attrib("DEFLAYER", 0.0, own)]);
    assert_eq!(attrib_layer(&write_dxf(&d)).as_deref(), Some("INS"));
    // Read from the file and saved again, in DXF and in DWG: the layer stays INS, not the definition's TXT.
    let from_dxf = read_dxf(write_dxf(&d).as_bytes()).expect("reopen");
    let from_dwg = cadcraft_io::read(&cadcraft_io::write(&d, "x.dwg").unwrap(), "x.dwg").unwrap();
    for (what, back) in [("DXF", from_dxf), ("DWG", from_dwg)] {
        let EntityKind::Insert(i) = &back.model.iter().next().expect("insert").kind else { panic!("not an insert") };
        assert_eq!(i.attribs[0].props.layer.as_deref(), Some("INS"), "{what}: read");
        let again = write_dxf(&back);
        assert_eq!(attrib_layer(&again).as_deref(), Some("INS"), "{what}: saved again");
        let again_dwg = cadcraft_io::read(&cadcraft_io::write(&back, "y.dwg").unwrap(), "y.dwg").unwrap();
        let EntityKind::Insert(i) = &again_dwg.model.iter().next().expect("insert").kind else { panic!("not an insert") };
        assert_eq!(i.attribs[0].props.layer.as_deref(), Some("INS"), "{what}: saved again to a DWG");
    }
    let _ = h;
}

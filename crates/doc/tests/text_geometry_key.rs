//! Every canonical text field is part of the replay key, so changing it alone retires the source snapshot; a property
//! edit (value, style, colour) keeps the key.

use cadcraft_doc::{Attrib, Common, EntityKind, HAlign, MText, MultilineAttrib, Text, VAlign, attrib_geometry_key, geometry_key};
use cadcraft_geom::Vec3;

fn text() -> Text {
    Text {
        insert: Vec3::new(1.0, 2.0, 3.0),
        align_pt: Some(Vec3::new(4.0, 5.0, 6.0)),
        height: 2.5,
        value: "Hello".into(),
        rotation: 0.3,
        width_factor: 0.9,
        oblique: 0.1,
        style: "Standard".into(),
        halign: HAlign::Center,
        valign: VAlign::Middle,
        backward: false,
        upside_down: false,
    }
}

fn mtext() -> MText {
    MText {
        insert: Vec3::new(1.0, 2.0, 3.0),
        height: 0.5,
        width: 4.0,
        attach: 5,
        rotation: 0.4,
        style: "Standard".into(),
        contents: "x".into(),
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
        direction: Some(Vec3::new(0.0, 0.6, 0.8)),
        drawing_direction: None,
    }
}

fn attrib() -> Attrib {
    let mut a = Attrib::new("TAG", text());
    a.plane.normal = Vec3::new(0.0, 3.0, 4.0);
    a.plane.thickness = 0.25;
    a.multiline = Some(MultilineAttrib {
        geometry_revision: Default::default(),
        normal: Vec3::new(1.0, 0.0, 2.0),
        mtext: mtext(),
        read_as: "x".into(),
        embedded_46: None,
        type_72: None,
    });
    a
}

/// Each case changes one field; the key must change, and must differ from every other case's.
#[test]
fn each_text_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 0.6, 0.8), thickness: 1.0, ..Common::default() };
    let key = |t: &Text, c: &Common| geometry_key(&EntityKind::Text(t.clone()), c).unwrap();
    let base = key(&text(), &common);
    let mut keys = vec![base.clone()];
    let mutations: Vec<(&str, Box<dyn Fn(&mut Text, &mut Common)>)> = vec![
        ("insert x", Box::new(|t, _| t.insert.x += 0.5)),
        ("insert z", Box::new(|t, _| t.insert.z += 0.5)),
        ("alignment point", Box::new(|t, _| t.align_pt = Some(Vec3::new(4.0, 5.0, 7.0)))),
        ("height", Box::new(|t, _| t.height = 2.6)),
        ("baseline angle", Box::new(|t, _| t.rotation = 0.31)),
        ("width factor", Box::new(|t, _| t.width_factor = 0.8)),
        ("oblique", Box::new(|t, _| t.oblique = 0.2)),
        ("horizontal justification", Box::new(|t, _| t.halign = HAlign::Right)),
        ("vertical justification", Box::new(|t, _| t.valign = VAlign::Top)),
        ("backward", Box::new(|t, _| t.backward = true)),
        ("upside down", Box::new(|t, _| t.upside_down = true)),
        ("normal", Box::new(|_, c| c.extrusion = Vec3::new(0.0, 0.6, 0.800_000_000_000_001))),
        ("thickness", Box::new(|_, c| c.thickness = -1.0)),
    ];
    for (name, mutate) in mutations {
        let (mut t, mut c) = (text(), common.clone());
        mutate(&mut t, &mut c);
        let k = key(&t, &c);
        assert_ne!(k, base, "{name}");
        assert!(!keys.contains(&k), "{name} has the key of another change");
        keys.push(k);
    }
    // A property edit keeps the key.
    let mut t = text();
    t.value = "Other".into();
    t.style = "Notes".into();
    assert_eq!(key(&t, &common), base);
}

#[test]
fn each_mtext_field_alone_changes_the_key() {
    let common = Common { extrusion: Vec3::new(0.0, 0.0, 2.0), ..Common::default() };
    let key = |m: &MText, c: &Common| geometry_key(&EntityKind::MText(m.clone()), c).unwrap();
    let base = key(&mtext(), &common);
    let mutations: Vec<(&str, Box<dyn Fn(&mut MText, &mut Common)>)> = vec![
        ("insert z", Box::new(|m, _| m.insert.z += 1.0)),
        ("direction x", Box::new(|m, _| m.direction = Some(Vec3::new(0.1, 0.6, 0.8)))),
        ("direction y", Box::new(|m, _| m.direction = Some(Vec3::new(0.0, 0.7, 0.8)))),
        ("direction z", Box::new(|m, _| m.direction = Some(Vec3::new(0.0, 0.6, 0.9)))),
        ("no canonical direction", Box::new(|m, _| m.direction = None)),
        ("attachment", Box::new(|m, _| m.attach = 6)),
        ("height", Box::new(|m, _| m.height = 0.6)),
        ("width", Box::new(|m, _| m.width = 5.0)),
        ("plane", Box::new(|_, c| c.extrusion = Vec3::new(0.0, 0.0, -2.0))),
    ];
    for (name, mutate) in mutations {
        let (mut m, mut c) = (mtext(), common.clone());
        mutate(&mut m, &mut c);
        assert_ne!(key(&m, &c), base, "{name}");
    }
    let mut m = mtext();
    m.contents = "changed".into();
    m.style = "Notes".into();
    assert_eq!(key(&m, &common), base, "contents and style are properties");
}

#[test]
fn an_attribute_has_its_own_plane_and_embedded_plane_in_the_key() {
    let base = attrib_geometry_key(&attrib());
    let mutations: Vec<(&str, Box<dyn Fn(&mut Attrib)>)> = vec![
        ("own normal", Box::new(|a| a.plane.normal = Vec3::new(0.0, 3.0, 4.000_000_000_000_001))),
        ("own thickness", Box::new(|a| a.plane.thickness = -0.25)),
        ("text insert z", Box::new(|a| a.text.insert.z += 1.0)),
        ("text backward", Box::new(|a| a.text.backward = true)),
        ("embedded normal", Box::new(|a| a.multiline.as_mut().unwrap().normal = Vec3::new(1.0, 0.0, 2.5))),
        ("embedded direction", Box::new(|a| a.multiline.as_mut().unwrap().mtext.direction = Some(Vec3::new(0.0, 0.8, 0.6)))),
        ("embedded insert", Box::new(|a| a.multiline.as_mut().unwrap().mtext.insert.x += 1.0)),
    ];
    for (name, mutate) in mutations {
        let mut a = attrib();
        mutate(&mut a);
        assert_ne!(attrib_geometry_key(&a), base, "{name}");
    }
    let mut a = attrib();
    a.text.value = "changed".into();
    a.invisible = true;
    assert_eq!(attrib_geometry_key(&a), base, "value and visibility are properties");
}

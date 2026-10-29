//! An INSERT's attributes keep their own identities through edits, preserving field dictionaries,
//! reactors and other applications' xdata on the right instance.

use cadcraft_doc::EntityKind;
use cadcraft_dxf::{Tag, records, sections};

fn field_dxf() -> String {
    include_str!("../data/attrib_fields.dxf").to_string()
}

fn section(dxf: &str, name: &str) -> Vec<(String, Vec<Tag>)> {
    let tags = cadcraft_dxf::parse(dxf.as_bytes()).unwrap();
    let section = sections(&tags).into_iter().find(|s| s.name == name).unwrap();
    records(&section.tags)
}

fn val(tags: &[Tag], code: i32) -> String {
    tags.iter().find(|t| t.code == code).map(Tag::str).unwrap_or_default()
}

fn assert_fields(dxf: &str) {
    let ents = section(dxf, "ENTITIES");
    let find_attr = |text: &str| &ents.iter().find(|(k, t)| k == "ATTRIB" && val(t, 1) == text).unwrap().1;
    if let Some((_, plain)) = ents.iter().find(|(k, t)| k == "ATTRIB" && val(t, 1) == "plain") {
        assert!(!plain.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "plain attribute stole the live attribute's dictionary");
    }
    let attr = find_attr("live");
    assert!(attr.iter().any(|t| t.code == 102 && t.str() == "{ACAD_XDICTIONARY"), "ATTRIB lost its extension dictionary");
    let objs = section(dxf, "OBJECTS");
    let object = |h: String| &objs.iter().find(|(_, t)| val(t, 5) == h).unwrap().1;
    let xdict = object(val(attr, 360));
    assert_eq!(val(xdict, 330), val(attr, 5));
    assert_eq!(val(xdict, 3), "ACAD_FIELD");
    let fields = object(val(xdict, 360));
    assert_eq!(val(fields, 330), val(xdict, 5));
    assert_eq!(val(fields, 3), "TEXT", "{dxf}");
    assert_eq!(objs.iter().filter(|(k, _)| k == "FIELD").count(), 2);
    let parent = object(val(fields, 360));
    assert_eq!(val(parent, 330), val(fields, 5));
    assert_eq!(val(parent, 1), "_text");
    assert_eq!(val(parent, 2), "c=%<\\_FldIdx 0>%m / i=Y%");
    let child = object(val(parent, 360));
    assert_eq!(val(child, 330), val(parent, 5));
    assert_eq!(val(child, 1), "AcObjProp");
    assert_eq!(val(child, 2), "\\AcObjProp Object(%<\\_ObjIdx 0>%).Length");
    let line = &ents.iter().find(|(k, _)| k == "LINE").unwrap().1;
    assert_eq!(val(child, 331), val(line, 5));
    assert!(attr.iter().any(|t| t.code == 102 && t.str() == "{ACAD_REACTORS"));
    assert!(attr.iter().any(|t| t.code == 330 && t.str() == val(child, 5)));
    assert_eq!(val(attr, 1001), "FOREIGN_FIELDS");
    assert_eq!(val(attr, 1000), "attribute data");
    assert_eq!(val(attr, 1005), val(attr, 5));
}

#[test]
fn attrib_fields_survive_dxf_save() {
    let d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    assert_fields(&saved);
    let back = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_fields(&cadcraft_io::write_dxf(&back));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn attrib_fields_survive_dwg_save() {
    let d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let dwg = cadcraft_io::write(&d, "x.dwg").unwrap();
    let dxf = cadcraft_dwg::dwg_to_dxf(&dwg).unwrap();
    assert_fields(&String::from_utf8_lossy(&dxf));
}

#[test]
fn attribute_extras_use_handle_keys_without_attribute_roles() {
    let d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let extra = d.foreign.entities.get(&cadcraft_doc::Handle(0x33)).expect("attribute extras are keyed by Handle");
    assert!(extra.xdict.is_some(), "extension dictionary belongs to the attribute handle");
    assert!(!extra.xdata.is_empty(), "attribute xdata stays with its handle-keyed extras");
    assert!(
        d.foreign.records.keys().chain(d.foreign.targets.values()).all(|key| key.0 != "ATTRIB"),
        "attributes must not use identity-to-itself roles"
    );
}

#[test]
fn deleting_preceding_duplicate_keeps_live_attrib_fields() {
    edited_attrib_fields(|i| i.attribs.retain(|a| a.text.value != "plain"));
}

#[test]
fn reordering_duplicate_attribs_keeps_fields_on_their_owner() {
    edited_attrib_fields(|i| i.attribs.swap(0, 1));
}

#[test]
fn renaming_attrib_keeps_its_fields() {
    edited_attrib_fields(|i| {
        for a in &mut i.attribs {
            a.tag = "RENAMED".into();
        }
    });
}

fn edited_attrib_fields(edit: impl FnOnce(&mut cadcraft_doc::Insert)) {
    let mut d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
    let h = d.model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
    d.model
        .modify(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                edit(i);
            }
        })
        .unwrap();
    let saved = cadcraft_io::write_dxf(&d);
    assert_fields(&saved);
    let back = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_fields(&cadcraft_io::write_dxf(&back));
}

#[test]
fn removed_attrib_or_insert_drops_its_fields() {
    for remove_insert in [false, true] {
        let mut d = cadcraft_io::read_dxf(field_dxf().as_bytes()).unwrap();
        let h = d.model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
        if remove_insert {
            d.model.remove(h);
        } else {
            d.model
                .modify(h, |e| {
                    if let EntityKind::Insert(i) = &mut e.kind {
                        i.attribs.retain(|a| a.text.value != "live");
                    }
                })
                .unwrap();
        }
        let saved = cadcraft_io::write_dxf(&d);
        assert!(section(&saved, "OBJECTS").iter().all(|(k, _)| k != "FIELD"));
        assert!(section(&saved, "ENTITIES").iter().filter(|(k, _)| k == "ATTRIB").all(|(_, t)| !t.iter().any(|g| g.code == 1001 || g.code == 102)));
    }
}

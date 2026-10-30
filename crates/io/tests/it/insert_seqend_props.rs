//! The SEQEND that closes the ATTRIBs of an INSERT keeps the layer, colour and linetype the file
//! gave it (AutoCAD keeps them; the save wrote the layer of the INSERT and nothing else). A
//! SEQEND the save makes takes the INSERT's layer.

use cadcraft_dxf::{Tag, records, sections};

fn source() -> String {
    "0\nSECTION\n2\nENTITIES\n\
0\nINSERT\n5\n20\n8\nINS\n66\n1\n2\nNOPE\n10\n0\n20\n0\n30\n0\n\
0\nATTRIB\n5\n21\n8\nINS\n10\n0\n20\n0\n30\n0\n40\n1\n1\nv\n2\nT\n70\n0\n\
0\nSEQEND\n5\n22\n330\n20\n100\nAcDbEntity\n8\n_PTOS\n6\nDASHED\n62\n3\n48\n0.5\n\
0\nENDSEC\n0\nEOF\n"
        .to_string()
}

fn seqend(text: &str) -> Vec<(i32, String)> {
    let tags = cadcraft_dxf::parse(text.as_bytes()).unwrap();
    let body: Vec<Tag> = sections(&tags)
        .into_iter()
        .filter(|s| s.name == "ENTITIES")
        .flat_map(|s| records(&s.tags))
        .find(|(k, _)| k == "SEQEND")
        .expect("a SEQEND record")
        .1;
    body.iter().map(|t| (t.code, t.str())).collect()
}

fn value(groups: &[(i32, String)], code: i32) -> Option<&str> {
    groups.iter().find(|(c, _)| *c == code).map(|(_, v)| v.as_str())
}

#[test]
fn the_seqend_of_an_insert_keeps_its_own_layer_colour_and_linetype() {
    let d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let groups = seqend(&cadcraft_io::write_dxf(&d));
    assert_eq!(value(&groups, 8), Some("_PTOS"));
    assert_eq!(value(&groups, 6), Some("DASHED"));
    assert_eq!(value(&groups, 62), Some("3"));
    assert_eq!(value(&groups, 48), Some("0.5"));
}

#[test]
fn it_keeps_them_when_the_insert_changes_layer() {
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let handle = d.model.iter().next().unwrap().handle;
    d.modify_entity(handle, |e| e.common.layer = "OTHER".into()).unwrap();
    assert_eq!(value(&seqend(&cadcraft_io::write_dxf(&d)), 8), Some("_PTOS"));
}

#[test]
fn a_seqend_the_save_makes_takes_the_layer_of_the_insert() {
    // The source SEQEND had the layer of its INSERT: an INSERT made here has no source SEQEND.
    let mut d = cadcraft_io::read_dxf(source().as_bytes()).unwrap();
    let source_handle = d.model.iter().next().unwrap().handle;
    let template = d.model.get(source_handle).unwrap();
    let (common, kind) = (cadcraft_doc::Common { layer: "OTHER".into(), ..template.common.clone() }, template.kind.clone());
    d.remove_entity(source_handle);
    d.add(&cadcraft_doc::Space::Model, common, kind).unwrap();
    let groups = seqend(&cadcraft_io::write_dxf(&d));
    assert_eq!(value(&groups, 8), Some("OTHER"));
    assert_eq!(value(&groups, 62), None);
}

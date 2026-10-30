//! A BLOCK_RECORD lists the INSERTs that reference it (`102 {BLKREFS`, `331` per INSERT), in the handle order they were made in, as AutoCAD
//! writes it: model space, layouts and other blocks all count. A block nothing inserts has no list.

use cadcraft_doc::{Common, EntityKind, Insert, Space};
use cadcraft_geom::Vec3;

use super::foreign_passthrough::{saved_twice, section, val};

fn insert(block: &str) -> EntityKind {
    EntityKind::Insert(Insert {
        block: block.into(),
        insert: Vec3::ZERO,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    })
}

fn drawing() -> (cadcraft_doc::Drawing, Vec<cadcraft_doc::Handle>) {
    let mut d = cadcraft_doc::Drawing::new_metric();
    for name in ["BOLT", "PLATE", "UNUSED"] {
        d.blocks.insert(name.into(), std::sync::Arc::new(cadcraft_doc::Block::new(name)));
    }
    let a = d.add(&Space::Model, Common::default(), insert("BOLT")).unwrap();
    let b = d.add(&Space::Model, Common::default(), insert("BOLT")).unwrap();
    let c = d.add(&Space::Paper("Layout1".into()), Common::default(), insert("BOLT")).unwrap();
    // An INSERT of BOLT inside PLATE.
    let handle = d.new_handle();
    let mut plate = cadcraft_doc::Block::new("PLATE");
    plate.entities.push(cadcraft_doc::Entity::new(handle, insert("BOLT")));
    d.blocks.insert("PLATE".into(), std::sync::Arc::new(plate));
    let p = d.add(&Space::Model, Common::default(), insert("PLATE")).unwrap();
    (d, vec![a, b, c, handle, p])
}

fn refs(text: &str, block: &str) -> Vec<String> {
    let records = section(text, "TABLES");
    let (_, record) = records.iter().find(|(k, t)| k == "BLOCK_RECORD" && val(t, 2) == block).unwrap_or_else(|| panic!("no record {block}"));
    let start = record.iter().position(|t| t.code == 102 && t.str() == "{BLKREFS");
    let Some(start) = start else { return Vec::new() };
    record[start + 1..].iter().take_while(|t| t.code == 331).map(|t| t.str()).collect()
}

#[test]
fn a_block_record_lists_its_inserts_in_dxf_and_dwg() {
    let (d, h) = drawing();
    for (what, text) in ["DXF", "DWG"].into_iter().zip(saved_twice(&d)) {
        let mut bolt: Vec<u64> = refs(&text, "BOLT").iter().map(|x| u64::from_str_radix(x, 16).unwrap()).collect();
        let mut wanted: Vec<u64> = h[..4].iter().map(|x| x.0).collect();
        bolt.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(bolt, wanted, "{what}: the four INSERTs of BOLT");
        assert_eq!(refs(&text, "PLATE"), [format!("{:X}", h[4].0)], "{what}: PLATE");
        assert!(refs(&text, "UNUSED").is_empty(), "{what}: an unused block has no list");
    }
}

#[test]
fn the_order_the_file_listed_is_kept_and_new_inserts_follow() {
    let (d, h) = drawing();
    // The file listed the inserts of BOLT in an order that is not the handle order.
    let listed = vec![h[3], h[1], h[0]];
    let mut source = (*d.source_symbols).clone();
    source.block_refs.insert("BOLT".into(), listed.clone());
    let mut d = d;
    d.source_symbols = std::sync::Arc::new(source);
    let text = cadcraft_io::write_dxf(&d);
    let want: Vec<String> = listed.iter().chain([&h[2]]).map(|x| x.hex()).collect();
    assert_eq!(refs(&text, "BOLT"), want, "the listed ones first, then the new one");
    // Read back, the order is the file's again.
    let again = cadcraft_io::write_dxf(&cadcraft_io::read_dxf(text.as_bytes()).unwrap());
    assert_eq!(refs(&again, "BOLT"), want);
}

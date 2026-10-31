//! The record adapter of the activation graph (plan v3, section 2.1): nodes, edges and the key of every record, from parsed records.

use cadcraft_dxf::{Section, parse, sections};
use cadcraft_io::activation::records::{Container, NodeKey, RecordGraph, family_of_kind};
use cadcraft_io::activation::{Edge, Family, Need, NodeId, NodeKind};

fn rec(kind: &str, groups: &str) -> String {
    format!("0\n{kind}\n{groups}")
}

fn dxf(tables: &str, blocks: &[String], entities: &[String]) -> Vec<Section> {
    let mut text = String::new();
    if !tables.is_empty() {
        text.push_str(&format!("0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n{tables}0\nENDTAB\n0\nENDSEC\n"));
    }
    text.push_str("0\nSECTION\n2\nBLOCKS\n");
    for block in blocks {
        text.push_str(block);
    }
    text.push_str("0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for entity in entities {
        text.push_str(entity);
    }
    text.push_str("0\nENDSEC\n0\nEOF\n");
    sections(&parse(text.as_bytes()).unwrap())
}

fn block(name: &str, members: &[String]) -> String {
    format!("0\nBLOCK\n5\nB\n2\n{name}\n70\n0\n{}0\nENDBLK\n", members.concat())
}

fn line(handle: &str) -> String {
    rec("LINE", &format!("5\n{handle}\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n"))
}

fn insert(handle: &str, name: &str) -> String {
    rec("INSERT", &format!("5\n{handle}\n8\n0\n66\n1\n2\n{name}\n10\n0\n20\n0\n"))
}

fn attrib(handle: &str) -> String {
    rec("ATTRIB", &format!("5\n{handle}\n8\n0\n1\nv\n2\nT\n"))
}

fn plain(_: &cadcraft_io::activation::records::RecordView) -> Need {
    Need::Plain
}

fn key(container: Container, ordinal: u32) -> NodeKey {
    NodeKey::record(container, ordinal, None)
}

fn edges_of(g: &RecordGraph) -> Vec<Edge> {
    g.graph().edges().to_vec()
}

#[test]
fn a_top_level_record_is_named_by_its_container_and_its_place() {
    let secs = dxf("", &[], &[line("A1"), line("A2"), line("A3")]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    assert_eq!(g.graph().nodes().len(), 3);
    for (i, ordinal) in [0u32, 1, 2].into_iter().enumerate() {
        let node = g.node(&key(Container::Entities, ordinal)).unwrap();
        assert_eq!(node, NodeId(i as u32));
        assert_eq!(g.key(node), Some(&key(Container::Entities, ordinal)));
    }
    assert_eq!(g.node(&key(Container::Entities, 3)), None);
    assert_eq!(g.node(&key(Container::block("X"), 0)), None);
}

#[test]
fn a_block_is_a_definition_that_contains_its_records() {
    let secs = dxf("", &[block("Door", &[line("B1"), line("B2")])], &[]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    let definition = g.node(&NodeKey::definition("door")).unwrap();
    assert!(matches!(g.graph().nodes()[definition.0 as usize].kind, NodeKind::Definition));
    let members: Vec<NodeId> = (0..2).map(|i| g.node(&key(Container::block("DOOR"), i)).unwrap()).collect();
    for member in members {
        assert!(edges_of(&g).contains(&Edge::Contains { definition, member }));
    }
}

#[test]
fn an_insert_refers_to_its_definition_and_owns_its_attributes() {
    let secs = dxf("", &[block("Door", &[line("B1")])], &[insert("A1", "door"), attrib("A2"), attrib("A3"), rec("SEQEND", "5\nA4\n"), line("A5")]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    let definition = g.node(&NodeKey::definition("DOOR")).unwrap();
    let user = g.node(&key(Container::Entities, 0)).unwrap();
    assert!(edges_of(&g).contains(&Edge::Refers { user, definition }));
    let attribs: Vec<NodeId> = (0..2).map(|i| g.node(&NodeKey::record(Container::Entities, 0, Some(i))).unwrap()).collect();
    for attrib in attribs {
        assert!(edges_of(&g).contains(&Edge::Owns { insert: user, attrib }));
    }
    // The SEQEND is not a record of its own: the LINE after the INSERT is ordinal 1.
    let after = g.node(&key(Container::Entities, 1)).unwrap();
    assert!(matches!(g.graph().nodes()[after.0 as usize].kind, NodeKind::Record(Family::Line)));
    assert_eq!(g.node(&key(Container::Entities, 2)), None);
}

#[test]
fn polyline_followers_are_part_of_their_parent() {
    let polyline = rec("POLYLINE", "5\nA1\n8\n0\n66\n1\n70\n0\n");
    let vertex = |h: &str| rec("VERTEX", &format!("5\n{h}\n8\n0\n10\n0\n20\n0\n"));
    let secs = dxf("", &[], &[polyline, vertex("A2"), vertex("A3"), rec("SEQEND", "5\nA4\n"), line("A5")]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    assert_eq!(g.graph().nodes().len(), 2);
    assert!(g.node(&key(Container::Entities, 1)).is_some());
}

#[test]
fn a_dimension_and_a_table_refer_to_their_graphics_blocks() {
    let dimension = rec("DIMENSION", "5\nA1\n8\n0\n2\n*D1\n70\n0\n");
    let table = rec("ACAD_TABLE", "5\nA2\n8\n0\n100\nAcDbBlockReference\n2\n*T1\n");
    let secs = dxf("", &[block("*D1", &[line("B1")]), block("*T1", &[line("B2")])], &[dimension, table]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    for (ordinal, name) in [(0, "*D1"), (1, "*T1")] {
        let user = g.node(&key(Container::Entities, ordinal)).unwrap();
        let definition = g.node(&NodeKey::definition(name)).unwrap();
        assert!(edges_of(&g).contains(&Edge::Refers { user, definition }), "{name}");
    }
}

#[test]
fn a_multileader_refers_to_its_block_content_by_either_handle_group() {
    // The block record handle of the content: group 344 of the entity, or group 341 of a context that has block contents (296 = 1).
    let outer = rec("MULTILEADER", "5\nA1\n8\n0\n300\nCONTEXT_DATA{\n290\n0\n296\n1\n341\nBR1\n301\n}\n344\nBR1\n");
    let inner = rec("MULTILEADER", "5\nA2\n8\n0\n300\nCONTEXT_DATA{\n290\n0\n296\n1\n341\nBR1\n301\n}\n");
    let text = rec("MULTILEADER", "5\nA3\n8\n0\n300\nCONTEXT_DATA{\n290\n1\n304\nTilted\n301\n}\n341\nBR1\n");
    let secs = dxf("0\nBLOCK_RECORD\n5\nBR1\n2\nMLB\n", &[block("MLB", &[line("B1")])], &[outer, inner, text]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    let definition = g.node(&NodeKey::definition("MLB")).unwrap();
    let refers = |ordinal| {
        let user = g.node(&key(Container::Entities, ordinal)).unwrap();
        edges_of(&g).contains(&Edge::Refers { user, definition })
    };
    assert!(refers(0) && refers(1), "block content");
    // Group 341 outside a context is the line type, and a context without block contents names none.
    assert!(!refers(2));
}

#[test]
fn a_minsert_is_an_insert() {
    let minsert = rec("INSERT", "5\nA1\n8\n0\n2\nDoor\n70\n3\n71\n2\n44\n5\n45\n4\n");
    let secs = dxf("", &[block("Door", &[line("B1")])], &[minsert]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    let user = g.node(&key(Container::Entities, 0)).unwrap();
    let definition = g.node(&NodeKey::definition("Door")).unwrap();
    assert!(edges_of(&g).contains(&Edge::Refers { user, definition }));
}

#[test]
fn layout_blocks_hold_top_level_records_and_are_no_definition() {
    let secs = dxf("", &[block("*Model_Space", &[]), block("*Paper_Space1", &[line("B1")])], &[]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    assert_eq!(g.node(&NodeKey::definition("*Paper_Space1")), None);
    let record = g.node(&key(Container::block("*PAPER_SPACE1"), 0)).unwrap();
    assert!(!edges_of(&g).iter().any(|e| matches!(e, Edge::Contains { member, .. } if *member == record)));
}

#[test]
fn a_record_of_an_unknown_type_takes_a_place_but_has_no_node() {
    let secs = dxf("", &[], &[line("A1"), rec("3DSOLID", "5\nA2\n8\n0\n"), line("A3")]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    assert_eq!(g.graph().nodes().len(), 2);
    assert!(g.node(&key(Container::Entities, 1)).is_none());
    assert!(g.node(&key(Container::Entities, 2)).is_some());
}

#[test]
fn a_name_that_is_not_a_block_gives_no_edge_and_a_repeated_block_keeps_the_first() {
    let secs = dxf("", &[block("Door", &[line("B1")]), block("DOOR", &[line("B2")])], &[insert("A1", "Missing"), insert("A2", "door")]);
    let g = RecordGraph::build(&secs, &plain).unwrap();
    let definitions = g.graph().nodes().iter().filter(|n| matches!(n.kind, NodeKind::Definition)).count();
    assert_eq!(definitions, 1);
    let refers = edges_of(&g).iter().filter(|e| matches!(e, Edge::Refers { .. })).count();
    assert_eq!(refers, 1);
}

#[test]
fn the_need_function_sees_each_record_with_its_key_and_family() {
    let secs = dxf("", &[block("Door", &[line("B1")])], &[insert("A1", "Door"), attrib("A2")]);
    let seen = std::cell::RefCell::new(Vec::new());
    let need = |view: &cadcraft_io::activation::records::RecordView| {
        seen.borrow_mut().push((view.key.clone(), view.family));
        if view.family == Family::Attrib { Need::Needs { readable: true } } else { Need::Plain }
    };
    let g = RecordGraph::build(&secs, &need).unwrap();
    let seen = seen.into_inner();
    assert!(seen.contains(&(NodeKey::record(Container::Entities, 0, Some(0)), Family::Attrib)));
    assert!(seen.contains(&(key(Container::block("DOOR"), 0), Family::Line)));
    let attrib = g.node(&NodeKey::record(Container::Entities, 0, Some(0))).unwrap();
    assert_eq!(g.graph().nodes()[attrib.0 as usize].need, Need::Needs { readable: true });
}

#[test]
fn the_family_of_every_entity_type_with_a_reader_is_named() {
    assert_eq!(family_of_kind("LWPOLYLINE"), Some(Family::LwPolyline));
    assert_eq!(family_of_kind("ACAD_TABLE"), Some(Family::Table));
    assert_eq!(family_of_kind("MULTILEADER"), Some(Family::MLeader));
    assert_eq!(family_of_kind("3DSOLID"), None);
}

#[test]
fn a_repeated_container_continues_its_count_so_that_two_records_never_share_a_key() {
    // A damaged file with two ENTITIES sections, and two blocks of one name: the second continues the first.
    let secs = {
        let one = dxf("", &[block("Door", &[line("B1")]), block("DOOR", &[line("B2")])], &[line("A1")]);
        let two = dxf("", &[], &[line("A2")]);
        one.into_iter().chain(two.into_iter().filter(|s| s.name == "ENTITIES")).collect::<Vec<_>>()
    };
    let g = RecordGraph::build(&secs, &plain).unwrap();
    assert_eq!(g.graph().nodes().len(), 5);
    for ordinal in 0..2 {
        assert!(g.node(&key(Container::Entities, ordinal)).is_some(), "entities {ordinal}");
        assert!(g.node(&key(Container::block("door"), ordinal)).is_some(), "door {ordinal}");
    }
}

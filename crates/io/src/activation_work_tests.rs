use std::cell::Cell;

use super::{Edge, Failure, Family, FamilySet, Graph, Need, Node, NodeId, Outcome, Policy, activate};

thread_local! {
    pub(super) static REASON_PROBES: Cell<usize> = const { Cell::new(0) };
    pub(super) static OWNERSHIP_PROBES: Cell<usize> = const { Cell::new(0) };
    pub(super) static REASON_FORMATS: Cell<usize> = const { Cell::new(0) };
}

fn off() -> Policy {
    Policy { mask: FamilySet::NONE, readers: FamilySet::ALL, definitions: true }
}

fn unreadable_block(count: u32) -> Graph {
    let mut nodes = vec![Node::definition()];
    nodes.extend(std::iter::repeat_n(Node::record(Family::Circle, Need::Needs { readable: false }), count as usize));
    let edges = (1..=count).map(|index| Edge::Contains { definition: NodeId(0), member: NodeId(index) }).collect();
    Graph::new(nodes, edges).unwrap()
}

#[test]
fn one_component_formats_each_failure_once_for_all_raw_records() {
    let count = 2_000u32;
    REASON_FORMATS.with(|formats| formats.set(0));
    let activation = activate(&unreadable_block(count), &Policy { mask: FamilySet::ALL, ..off() });
    for index in 1..=count {
        assert!(activation.reason_for(NodeId(index)).is_some());
    }
    assert_eq!(REASON_FORMATS.with(Cell::get), count as usize, "a shared reason must not format every failure for every record");
}

#[test]
fn a_shared_reason_is_bounded_but_keeps_the_complete_failure_list() {
    let count = 2_000u32;
    let activation = activate(&unreadable_block(count), &Policy { mask: FamilySet::ALL, ..off() });
    let reason = activation.reason_for(NodeId(1)).unwrap();
    assert_eq!(reason.len(), "its canonical reading fails".len());
    assert_eq!(reason, "its canonical reading fails");
    assert_eq!(activation.components()[0].failures, (1..=count).map(|index| Failure::UnreadableNeed(NodeId(index))).collect::<Vec<_>>());
    let last = activation.reason_for(NodeId(count)).unwrap();
    assert!(std::ptr::eq(reason.as_ptr(), last.as_ptr()), "all records must borrow the same cached reason");
}

#[test]
fn independent_need_records_use_one_component_probe_per_reason() {
    let count = 10_000u32;
    let graph = Graph::new(vec![Node::record(Family::Circle, Need::Needs { readable: true }); count as usize], vec![]).unwrap();
    let activation = activate(&graph, &off());
    REASON_PROBES.with(|probes| probes.set(0));
    for index in 0..count {
        assert!(activation.reason_for(NodeId(index)).is_some());
    }
    let probes = REASON_PROBES.with(Cell::get);
    assert_eq!(probes, count as usize, "{count} independent records required {probes} component probes");
    assert!(activation.reason_for(NodeId(count)).is_none());
}

#[test]
fn failed_components_share_one_ownership_edge_pass() {
    let count = 1_000u32;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for index in 0..count {
        let insert = NodeId(index * 2);
        let attrib = NodeId(index * 2 + 1);
        nodes.push(Node::record(Family::Insert, Need::Needs { readable: true }));
        nodes.push(Node::record(Family::Attrib, Need::Plain));
        edges.push(Edge::Owns { insert, attrib });
    }
    let graph = Graph::new(nodes, edges).unwrap();
    OWNERSHIP_PROBES.with(|probes| probes.set(0));
    let activation = activate(&graph, &off());
    for index in 0..count * 2 {
        assert_eq!(activation.outcome(NodeId(index)), Some(Outcome::Raw));
    }
    let probes = OWNERSHIP_PROBES.with(Cell::get);
    assert_eq!(probes, graph.edges().len(), "{} ownership edges required {probes} probes", graph.edges().len());
}

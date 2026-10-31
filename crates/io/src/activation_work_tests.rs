use std::cell::Cell;

use super::inspections::{COMPONENTS, EDGES, InspectedVec, MEMBERS};
use super::{Edge, Failure, Family, FamilySet, Graph, Need, Node, NodeId, Outcome, Policy, activate};

thread_local! {
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
    COMPONENTS.with(|probes| probes.set(0));
    MEMBERS.with(|probes| probes.set(0));
    for index in 0..count {
        assert!(activation.reason_for(NodeId(index)).is_some());
    }
    let probes = COMPONENTS.with(Cell::get);
    assert_eq!(probes, count as usize, "{count} independent records inspected {probes} components");
    assert_eq!(MEMBERS.with(Cell::get), 0, "indexed reason lookup must not scan component members");
    assert!(activation.reason_for(NodeId(count)).is_none());
}

#[test]
fn one_large_component_does_not_scan_members_for_each_reason() {
    let count = 2_000u32;
    let activation = activate(&unreadable_block(count), &Policy { mask: FamilySet::ALL, ..off() });
    COMPONENTS.with(|probes| probes.set(0));
    MEMBERS.with(|probes| probes.set(0));
    for index in 1..=count {
        assert!(activation.reason_for(NodeId(index)).is_some());
    }
    assert_eq!(COMPONENTS.with(Cell::get), count as usize);
    assert_eq!(MEMBERS.with(Cell::get), 0, "one component must not cause a member scan per Raw record");
}

#[test]
fn inspection_storage_counts_values_not_requests() {
    let values = InspectedVec::<_, 1>::new(vec![NodeId(1), NodeId(2), NodeId(3)]);
    MEMBERS.with(|probes| probes.set(0));
    assert!(values.contains(&NodeId(2)));
    assert_eq!(MEMBERS.with(Cell::get), 2);
    assert_eq!(values.iter().take(2).count(), 2);
    assert_eq!(MEMBERS.with(Cell::get), 4);
    assert_eq!(values.get(1), Some(&NodeId(2)));
    assert_eq!(MEMBERS.with(Cell::get), 5);
    assert_eq!(values[0], NodeId(1));
    assert_eq!(MEMBERS.with(Cell::get), 6);
    assert_eq!((&values).into_iter().count(), 3);
    assert_eq!(MEMBERS.with(Cell::get), 9);
    assert_eq!(values.into_iter().count(), 3);
    assert_eq!(MEMBERS.with(Cell::get), 12);
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
    EDGES.with(|probes| probes.set(0));
    let activation = activate(&graph, &off());
    for index in 0..count * 2 {
        assert_eq!(activation.outcome(NodeId(index)), Some(Outcome::Raw));
    }
    let probes = EDGES.with(Cell::get);
    assert_eq!(probes, 3 * graph.edges().len(), "{} edges required {probes} inspections across union and both nesting passes", graph.edges().len());
}

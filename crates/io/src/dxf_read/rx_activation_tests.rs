use super::*;
use crate::activation::{Family, FamilySet, Graph, Need, Node, NodeId, Policy, ReaderMode, activate};
use cadcraft_doc::GeometryRevision;

fn graph() -> Graph {
    Graph::new(
        vec![
            Node::record(Family::Circle, Need::Needs { readable: true }),
            Node::record(Family::Line, Need::Plain),
            Node::record(Family::Region, Need::Needs { readable: false }),
        ],
        Vec::new(),
    )
    .unwrap()
}

#[test]
fn an_unprepared_rx_has_no_implicit_legacy_decision() {
    let rx = Rx::default();
    assert_eq!(rx.reader_policy().mask(), FamilySet::NONE);
    assert_eq!(rx.revision_for(NodeId(0)), None);
}

#[test]
fn rx_keeps_per_load_policy_and_prepared_component_revisions() {
    let families = FamilySet::of(&[Family::Circle, Family::Line]);
    let safe = ReaderPolicy::new(ReaderMode::Safe, families);
    let off = ReaderPolicy::new(ReaderMode::Off, families);
    let mut a = Rx { reader_policy: safe, ..Default::default() };
    let mut b = Rx { reader_policy: off, ..Default::default() };
    for rx in [&mut a, &mut b] {
        rx.activation = Some(activate(&graph(), &Policy { mask: rx.reader_policy().mask(), readers: families, definitions: false }));
    }
    assert_eq!(a.reader_policy(), safe);
    assert_eq!(b.reader_policy(), off);
    assert_eq!(a.revision_for(NodeId(0)), Some(GeometryRevision::CanonicalV2));
    assert_eq!(b.revision_for(NodeId(0)), Some(GeometryRevision::RawSource));
    assert_eq!(a.revision_for(NodeId(1)), Some(GeometryRevision::LegacyProjectedV1));
    assert_eq!(a.revision_for(NodeId(2)), Some(GeometryRevision::RawSource));
    assert_eq!(a.revision_for(NodeId(99)), None);
}

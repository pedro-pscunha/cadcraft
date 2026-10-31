//! I5 of the activation plan (section 2.6): the read reports how many records each component activated, and which records
//! stayed Raw.

use cadcraft_dxf::{parse, sections};
use cadcraft_io::activation::records::RecordGraph;
use cadcraft_io::activation::{ComponentOutcome, FamilySet, Outcome, ReaderMode, ReaderPolicy, activate};

use crate::activation_fixtures::{cases, dir, stand_in};

#[test]
fn i5_each_fixture_reports_the_records_each_component_activated() {
    for case in cases() {
        let secs = sections(&parse(case.text.as_bytes()).unwrap());
        let graph = RecordGraph::build(&secs, &stand_in).unwrap();
        let report = graph.report(&case.policy);
        let activation = activate(graph.graph(), &case.policy);
        // The components partition the nodes, and a component is activated whole or not at all.
        assert_eq!(report.components.iter().map(|c| c.nodes).sum::<usize>(), graph.graph().nodes().len(), "{}", case.name);
        for component in &report.components {
            match component.outcome {
                ComponentOutcome::Canonical => assert_eq!(component.activated, component.nodes, "{}", case.name),
                _ => assert_eq!(component.activated, 0, "{}", case.name),
            }
        }
        assert_eq!(report.activated(), activation.activated(), "{}", case.name);
        // The Raw records are named by their handles, in file order.
        let raw: Vec<String> = (0..graph.graph().nodes().len() as u32)
            .map(cadcraft_io::activation::NodeId)
            .filter(|n| activation.outcome(*n) == Some(Outcome::Raw))
            .filter_map(|n| graph.handle(n).map(str::to_string))
            .collect();
        let mut reported: Vec<String> = report.components.iter().flat_map(|c| c.raw.clone()).collect();
        reported.sort();
        let mut want = raw;
        want.sort();
        assert_eq!(reported, want, "{}", case.name);
    }
}

#[test]
fn i5_the_counts_of_two_fixtures() {
    let by_name = |name: &str| cases().into_iter().find(|c| c.name == name).unwrap();
    // A1: the tilted INSERT, its definition and the two members are one component of four, all activated.
    let a1 = by_name("a1_tilted_insert_over_plain_definition");
    let report = RecordGraph::build(&sections(&parse(a1.text.as_bytes()).unwrap()), &stand_in).unwrap().report(&a1.policy);
    assert_eq!(report.components.iter().map(|c| (c.nodes, c.activated)).collect::<Vec<_>>(), [(4, 4)]);
    // B1: the definition, its INSERT and the two plain members stay Legacy; the spline B1 is the one Raw record.
    let b1 = by_name("b1_needs_member_in_component_that_cannot_activate");
    let report = RecordGraph::build(&sections(&parse(b1.text.as_bytes()).unwrap()), &stand_in).unwrap().report(&b1.policy);
    assert_eq!(report.components.len(), 1);
    assert_eq!((report.components[0].nodes, report.components[0].activated), (5, 0));
    assert_eq!(report.components[0].raw, ["B1"]);
}

#[test]
fn the_read_report_of_a_file_comes_with_the_drawing() {
    let text = std::fs::read(dir().join("a1_tilted_insert_over_plain_definition.dxf")).unwrap();
    let policy = ReaderPolicy::new(ReaderMode::Safe, FamilySet::NONE);
    let (drawing, report) = cadcraft_io::read_report_with(&text, "a1.dxf", policy, &stand_in).unwrap();
    // The reader is off: the same drawing as the plain read.
    assert_eq!(drawing, cadcraft_io::read_with_policy(&text, "a1.dxf", policy).unwrap());
    // Nothing is in the mask, so the record that needs Canonical is not activated but kept Raw in the report.
    assert_eq!(report.activated(), 0);
    assert_eq!(report.components.len(), 1);
    assert_eq!(report.components[0].outcome, ComponentOutcome::Raw);
    assert_eq!(report.components[0].raw, ["E1"]);
}

#[test]
fn a_report_without_a_need_function_finds_nothing_that_needs_canonical() {
    let text = std::fs::read(dir().join("a1_tilted_insert_over_plain_definition.dxf")).unwrap();
    let (_, report) = cadcraft_io::read_report(&text, "a1.dxf", ReaderPolicy::default()).unwrap();
    assert_eq!(report.activated(), 0);
    assert!(report.components.iter().all(|c| c.outcome == ComponentOutcome::Legacy && c.raw.is_empty()));
}

#[test]
fn the_graph_is_not_needed_while_the_mask_is_empty() {
    // One condition decides whether a plain read builds the graph.
    assert!(!ReaderPolicy::default().needs_graph());
    assert!(!ReaderPolicy::new(ReaderMode::Off, FamilySet::NONE).needs_graph());
}

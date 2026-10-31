//! I5 of the activation plan (section 2.6): the read reports how many records each component activated, and which records
//! stayed Raw.

use cadcraft_dxf::{parse, sections};
use cadcraft_io::activation::records::RecordGraph;
use cadcraft_io::activation::{ComponentOutcome, FamilySet, Outcome, ReaderMode, ReaderPolicy, activate};

use crate::activation_fixtures::{cases, dir, predicate, source_predicate};

#[test]
fn i5_each_fixture_reports_the_records_each_component_activated() {
    for case in cases() {
        let secs = sections(&parse(case.text.as_bytes()).unwrap());
        let graph = RecordGraph::build(&secs, &predicate(&secs)).unwrap();
        let report = graph.report(&case.policy);
        let activation = activate(graph.graph(), &case.policy);
        // Records and definitions partition the graph without conflating their activation counts.
        assert_eq!(
            report.components.iter().map(|component| component.records + component.definitions).sum::<usize>(),
            graph.graph().nodes().len(),
            "{}",
            case.name
        );
        for component in &report.components {
            let counts = (component.activated_records, component.activated_definitions);
            match component.outcome {
                ComponentOutcome::Canonical => assert_eq!(counts, (component.records, component.definitions), "{}", case.name),
                _ => assert_eq!(counts, (0, 0), "{}", case.name),
            }
        }
        let activated_definitions = report.components.iter().map(|component| component.activated_definitions).sum::<usize>();
        assert_eq!(report.activated() + activated_definitions, activation.activated(), "{}", case.name);
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
fn i5_record_definition_and_attribute_counts_are_independent() {
    let by_name = |name: &str| cases().into_iter().find(|c| c.name == name).unwrap();
    // A1: the tilted INSERT, its definition and the two members are one component of four, all activated.
    let a1 = by_name("a1_tilted_insert_over_plain_definition");
    let report =
        RecordGraph::build(&sections(&parse(a1.text.as_bytes()).unwrap()), &source_predicate(a1.text.as_bytes())).unwrap().report(&a1.policy);
    assert_eq!(report.activated(), 3, "a definition must not inflate the activated record count");
    assert_eq!(
        report
            .components
            .iter()
            .map(|component| (component.records, component.definitions, component.activated_records, component.activated_definitions))
            .collect::<Vec<_>>(),
        [(3, 1, 3, 1)]
    );
    let a2 = by_name("a2_needing_attribute_on_plain_insert");
    let report =
        RecordGraph::build(&sections(&parse(a2.text.as_bytes()).unwrap()), &source_predicate(a2.text.as_bytes())).unwrap().report(&a2.policy);
    assert_eq!(report.activated(), 4, "INSERT, two ATTRIBs and the definition's LINE are four records");
    assert_eq!((report.components[0].definitions, report.components[0].activated_definitions), (1, 1));
    // B1: the definition and its two plain members stay Legacy. The SPLINE and unproved INSERT stay Raw.
    let b1 = by_name("b1_needs_member_in_component_that_cannot_activate");
    let report =
        RecordGraph::build(&sections(&parse(b1.text.as_bytes()).unwrap()), &source_predicate(b1.text.as_bytes())).unwrap().report(&b1.policy);
    assert_eq!(report.components.len(), 1);
    assert_eq!(
        (
            report.components[0].records,
            report.components[0].definitions,
            report.components[0].activated_records,
            report.components[0].activated_definitions
        ),
        (4, 1, 0, 0)
    );
    assert_eq!(report.components[0].raw, ["B1", "E1"]);
}

#[test]
fn i5_an_empty_definition_is_not_an_authored_record() {
    let text = b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nEMPTY\n70\n0\n0\nENDBLK\n0\nENDSEC\n0\nEOF\n";
    let graph = RecordGraph::build(&sections(&parse(text).unwrap()), &|_| cadcraft_io::activation::Need::Plain).unwrap();
    let report = graph.report(&cadcraft_io::activation::Policy { mask: FamilySet::ALL, readers: FamilySet::ALL, definitions: true });
    assert_eq!(report.components.len(), 1);
    assert_eq!((report.components[0].records, report.components[0].definitions), (0, 1));
    assert_eq!((report.activated(), report.components[0].activated_definitions), (0, 0));
}

#[test]
fn the_read_report_of_a_file_comes_with_the_drawing() {
    let text = std::fs::read(dir().join("a1_tilted_insert_over_plain_definition.dxf")).unwrap();
    let policy = ReaderPolicy::new(ReaderMode::Safe, FamilySet::NONE);
    let (drawing, report) = cadcraft_io::read_report_with(&text, "a1.dxf", policy, &source_predicate(&text)).unwrap();
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

//! The read fixtures A1 to A11 and B1 to B5 of the activation plan (section 2.6) as real DXF files, in
//! `tests/data/activation/`. Each file is read, its graph is built by the record adapter with a stand-in for the section 3 predicate,
//! and the activation of the graph is asserted: the outcome of every named record, the typed reason of every Raw one, and the
//! invariants I1 (agreement among the non-Raw nodes) and I2 (a node that needs Canonical is never Legacy).
//!
//! The files are generated from the text below (`cargo test -p cadcraft-io --test it write_activation_fixtures -- --ignored`) and a test
//! checks that the files on disk are the generated ones.

use std::path::{Path, PathBuf};

use cadcraft_dxf::{Tag, parse, sections};
use cadcraft_io::activation::records::{Container, NodeKey, RecordGraph, RecordView};
use cadcraft_io::activation::{Edge, Failure, Family, FamilySet, Need, NodeId, NodeKind, Outcome, Policy, activate};

// ---- the DXF text of the fixtures ----

const TILT: &str = "210\n0\n220\n1\n230\n1\n";

fn extrusion(tilted: bool) -> &'static str {
    if tilted { TILT } else { "" }
}

fn line(handle: &str) -> String {
    format!("0\nLINE\n5\n{handle}\n8\n0\n10\n0\n20\n0\n30\n0\n11\n1\n21\n1\n31\n0\n")
}

fn circle(handle: &str, tilted: bool) -> String {
    format!("0\nCIRCLE\n5\n{handle}\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n{}", extrusion(tilted))
}

/// A SPLINE; `controls_declared` is the count of group 73, so a count that is not the points that follow makes it unreadable.
fn spline(handle: &str, tilted: bool, controls_declared: usize) -> String {
    format!("0\nSPLINE\n5\n{handle}\n8\n0\n{}70\n8\n71\n3\n73\n{controls_declared}\n10\n0\n20\n0\n30\n0\n10\n1\n20\n1\n30\n0\n", extrusion(tilted))
}

fn insert(handle: &str, block: &str, tilted: bool) -> String {
    format!("0\nINSERT\n5\n{handle}\n8\n0\n66\n1\n2\n{block}\n10\n0\n20\n0\n30\n0\n{}", extrusion(tilted))
}

fn attrib(handle: &str, tilted: bool) -> String {
    format!("0\nATTRIB\n5\n{handle}\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n1\nv\n2\nT\n{}", extrusion(tilted))
}

fn seqend(handle: &str) -> String {
    format!("0\nSEQEND\n5\n{handle}\n8\n0\n")
}

fn block(name: &str, flags: i32, members: &[String]) -> String {
    format!("0\nBLOCK\n5\nB0\n8\n0\n2\n{name}\n70\n{flags}\n10\n0\n20\n0\n30\n0\n3\n{name}\n{}0\nENDBLK\n5\nB1\n8\n0\n", members.concat())
}

fn file(tables: &str, blocks: &[String], entities: &[String]) -> String {
    let tables =
        if tables.is_empty() { String::new() } else { format!("0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n{tables}0\nENDTAB\n0\nENDSEC\n") };
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1027\n0\nENDSEC\n{tables}0\nSECTION\n2\nBLOCKS\n{}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        blocks.concat(),
        entities.concat()
    )
}

/// One fixture: its name, its text, the policy of the stage it is read at, and what it must come to.
struct Case {
    name: &'static str,
    text: String,
    policy: Policy,
    /// (what the record is called, its key, the expected outcome)
    expect: Vec<(&'static str, NodeKey, Outcome)>,
    /// Typed reasons that the component of the first record must carry.
    failures: Vec<FailureKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureKind {
    Cycle,
    UnreadableNeed,
    DefinitionBeforeStage6,
    NoReader(Family),
    OutsideMask(Family),
}

fn matches_failure(kind: FailureKind, f: &Failure) -> bool {
    match (kind, f) {
        (FailureKind::Cycle, Failure::Cycle(_)) | (FailureKind::UnreadableNeed, Failure::UnreadableNeed(_)) => true,
        (FailureKind::DefinitionBeforeStage6, Failure::DefinitionBeforeStage6(_)) => true,
        (FailureKind::NoReader(a), Failure::NoReader(b)) | (FailureKind::OutsideMask(a), Failure::FamilyOutsideMask(b)) => a == *b,
        _ => false,
    }
}

/// Every family has a reader and is in the mask, and definitions can activate (stage 6).
fn open() -> Policy {
    Policy { mask: FamilySet::ALL, readers: FamilySet::ALL, definitions: true }
}

fn before_stage_6() -> Policy {
    Policy { definitions: false, ..open() }
}

fn entity(ordinal: u32) -> NodeKey {
    NodeKey::record(Container::Entities, ordinal, None)
}

fn member(block: &str, ordinal: u32) -> NodeKey {
    NodeKey::record(Container::block(block), ordinal, None)
}

fn def(name: &str) -> NodeKey {
    NodeKey::definition(name)
}

fn cases() -> Vec<Case> {
    use Outcome::{Canonical, Legacy, Raw};
    let nothing = Vec::new;
    let all_canonical = |keys: Vec<(&'static str, NodeKey)>| keys.into_iter().map(|(n, k)| (n, k, Canonical)).collect::<Vec<_>>();
    let cycle = |seed_tilted: bool| {
        file(
            "",
            &[block("A", 0, &[spline("A1", seed_tilted, 2), insert("A2", "B", false)]), block("B", 0, &[insert("B1", "A", false)])],
            &[insert("E1", "A", false)],
        )
    };
    vec![
        Case {
            name: "a1_tilted_insert_over_plain_definition",
            text: file("", &[block("DEF", 0, &[line("B1"), circle("B2", false)])], &[insert("E1", "DEF", true)]),
            policy: open(),
            expect: all_canonical(vec![("insert", entity(0)), ("def", def("DEF")), ("line", member("DEF", 0)), ("circle", member("DEF", 1))]),
            failures: nothing(),
        },
        Case {
            name: "a2_needing_attribute_on_plain_insert",
            text: file("", &[block("DEF", 0, &[line("B1")])], &[insert("E1", "DEF", false), attrib("E2", true), attrib("E3", false), seqend("E4")]),
            policy: open(),
            expect: all_canonical(vec![
                ("insert", entity(0)),
                ("needing attrib", NodeKey::record(Container::Entities, 0, Some(0))),
                ("plain attrib", NodeKey::record(Container::Entities, 0, Some(1))),
                ("def", def("DEF")),
                ("line", member("DEF", 0)),
            ]),
            failures: nothing(),
        },
        Case {
            name: "a3_canonical_container_reaches_nested_target",
            text: file(
                "",
                &[block("OUTER", 0, &[spline("B1", true, 2), insert("B2", "INNER", false)]), block("INNER", 0, &[line("C1")])],
                &[insert("E1", "OUTER", false)],
            ),
            policy: open(),
            expect: all_canonical(vec![
                ("user", entity(0)),
                ("outer", def("OUTER")),
                ("needing", member("OUTER", 0)),
                ("nested insert", member("OUTER", 1)),
                ("inner", def("INNER")),
                ("leaf", member("INNER", 0)),
            ]),
            failures: nothing(),
        },
        Case {
            name: "a4_plain_insert_over_definition_with_tilted_child",
            text: file("", &[block("DEF", 0, &[circle("B1", true), line("B2")])], &[insert("E1", "DEF", false)]),
            policy: open(),
            expect: all_canonical(vec![("insert", entity(0)), ("def", def("DEF")), ("tilted", member("DEF", 0)), ("flat", member("DEF", 1))]),
            failures: nothing(),
        },
        Case {
            name: "a5_nested_chain",
            text: file(
                "",
                &[block("D1", 0, &[insert("B1", "D2", false)]), block("D2", 0, &[insert("C1", "D3", false)]), block("D3", 0, &[circle("D31", true)])],
                &[insert("E1", "D1", false)],
            ),
            policy: open(),
            expect: all_canonical(vec![
                ("top", entity(0)),
                ("d1", def("D1")),
                ("i1", member("D1", 0)),
                ("d2", def("D2")),
                ("i2", member("D2", 0)),
                ("d3", def("D3")),
                ("deep", member("D3", 0)),
            ]),
            failures: nothing(),
        },
        Case {
            name: "a6_shared_definition_in_model_and_in_another_definition",
            text: file(
                "",
                &[block("OTHER", 0, &[insert("B1", "SHARED", false)]), block("SHARED", 0, &[circle("C1", true)])],
                &[insert("E1", "SHARED", false), line("E2")],
            ),
            policy: open(),
            expect: vec![
                ("model user", entity(0), Canonical),
                ("other", def("OTHER"), Canonical),
                ("block user", member("OTHER", 0), Canonical),
                ("shared", def("SHARED"), Canonical),
                ("tilted", member("SHARED", 0), Canonical),
                ("unrelated", entity(1), Legacy),
            ],
            failures: nothing(),
        },
        Case {
            name: "a7_mleader_block_content_tilted",
            text: file(
                "0\nBLOCK_RECORD\n5\nBR1\n2\nMLB\n",
                &[block("MLB", 0, &[circle("B1", true)])],
                &["0\nMULTILEADER\n5\nE1\n8\n0\n300\nCONTEXT_DATA{\n290\n0\n296\n1\n341\nBR1\n301\n}\n344\nBR1\n".to_string()],
            ),
            policy: open(),
            expect: all_canonical(vec![("mleader", entity(0)), ("content", def("MLB")), ("tilted", member("MLB", 0))]),
            failures: nothing(),
        },
        Case {
            name: "a8_dimension_graphics_block_tilted",
            text: file(
                "",
                &[block("*D1", 1, &[line("B1"), circle("B2", true)])],
                &["0\nDIMENSION\n5\nE1\n8\n0\n2\n*D1\n70\n0\n10\n0\n20\n0\n30\n0\n".to_string()],
            ),
            policy: open(),
            expect: all_canonical(vec![("dim", entity(0)), ("star_d", def("*D1")), ("line", member("*D1", 0)), ("tilted", member("*D1", 1))]),
            failures: nothing(),
        },
        Case {
            name: "a9_xref_with_tilted_child",
            // The xref block (flag 4 and its path) holds the loaded content: a tilted CIRCLE.
            text: file("", &[block("XR", 4, &["1\nxref.dwg\n".to_string(), circle("B1", true)])], &[insert("E1", "XR", false)]),
            policy: open(),
            expect: all_canonical(vec![("xref insert", entity(0)), ("xref", def("XR")), ("tilted", member("XR", 0))]),
            failures: nothing(),
        },
        Case {
            name: "a10_cycle_all_plain",
            text: cycle(false),
            policy: open(),
            expect: vec![
                ("insert", entity(0), Legacy),
                ("a", def("A"), Legacy),
                ("member", member("A", 0), Legacy),
                ("a insert", member("A", 1), Legacy),
                ("b", def("B"), Legacy),
                ("b insert", member("B", 0), Legacy),
            ],
            failures: nothing(),
        },
        Case {
            name: "a11_cycle_with_needs_seed",
            text: cycle(true),
            policy: open(),
            expect: vec![
                ("insert", entity(0), Legacy),
                ("a", def("A"), Legacy),
                ("seed", member("A", 0), Raw),
                ("a insert", member("A", 1), Legacy),
                ("b", def("B"), Legacy),
                ("b insert", member("B", 0), Legacy),
            ],
            failures: vec![FailureKind::Cycle],
        },
        Case {
            name: "b1_needs_member_in_component_that_cannot_activate",
            // A SPLINE with a control count that does not match its points, on a tilted plane: its canonical read fails.
            text: file("", &[block("DEF", 0, &[spline("B1", true, 5), line("B2"), circle("B3", false)])], &[insert("E1", "DEF", false)]),
            policy: open(),
            expect: vec![
                ("insert", entity(0), Legacy),
                ("def", def("DEF"), Legacy),
                ("bad spline", member("DEF", 0), Raw),
                ("line", member("DEF", 1), Legacy),
                ("circle", member("DEF", 2), Legacy),
            ],
            failures: vec![FailureKind::UnreadableNeed],
        },
        Case {
            name: "b2_needs_insert_with_attributes_raw",
            // INSERT has no canonical reader: the tilted INSERT and its ATTRIBs are one Raw unit, the definition stays Legacy.
            text: file("", &[block("DEF", 0, &[line("B1")])], &[insert("E1", "DEF", true), attrib("E2", false), attrib("E3", false), seqend("E4")]),
            policy: Policy { readers: FamilySet::ALL.without(Family::Insert), ..open() },
            expect: vec![
                ("insert", entity(0), Raw),
                ("attrib 1", NodeKey::record(Container::Entities, 0, Some(0)), Raw),
                ("attrib 2", NodeKey::record(Container::Entities, 0, Some(1)), Raw),
                ("def", def("DEF"), Legacy),
                ("line", member("DEF", 0), Legacy),
            ],
            failures: vec![FailureKind::NoReader(Family::Insert)],
        },
        Case {
            name: "b3_component_with_unmasked_family",
            text: file("", &[block("DEF", 0, &[spline("B1", true, 2), line("B2")])], &[insert("E1", "DEF", false)]),
            policy: Policy { mask: FamilySet::ALL.without(Family::Spline), ..open() },
            expect: vec![
                ("insert", entity(0), Legacy),
                ("def", def("DEF"), Legacy),
                ("spline", member("DEF", 0), Raw),
                ("line", member("DEF", 1), Legacy),
            ],
            failures: vec![FailureKind::OutsideMask(Family::Spline)],
        },
        Case {
            name: "b4_unreferenced_definition_with_needing_member",
            text: file("", &[block("DEF", 0, &[circle("B1", true), line("B2")])], &[]),
            policy: before_stage_6(),
            expect: vec![("def", def("DEF"), Legacy), ("needing", member("DEF", 0), Raw), ("plain", member("DEF", 1), Legacy)],
            failures: vec![FailureKind::DefinitionBeforeStage6],
        },
        Case {
            name: "b5_dimension_with_star_d_block",
            text: file(
                "",
                &[block("*D1", 1, &[line("B1")])],
                &["0\nDIMENSION\n5\nE1\n8\n0\n2\n*D1\n70\n0\n10\n0\n20\n0\n30\n0\n210\n0\n220\n1\n230\n1\n".to_string()],
            ),
            policy: before_stage_6(),
            expect: vec![("dim", entity(0), Raw), ("star_d", def("*D1"), Legacy), ("line", member("*D1", 0), Legacy)],
            failures: vec![FailureKind::DefinitionBeforeStage6],
        },
    ]
}

// ---- the stand-in for the section 3 predicate ----

fn group(groups: &[Tag], code: i32) -> Option<f64> {
    groups.iter().find(|t| t.code == code).map(Tag::f64)
}

/// A record needs Canonical when its extrusion is not +Z. A SPLINE whose declared control count is not the number of its points
/// cannot be read.
pub(crate) fn stand_in(view: &RecordView) -> Need {
    let tilted = [group(view.groups, 210).unwrap_or(0.0), group(view.groups, 220).unwrap_or(0.0)].iter().any(|v| *v != 0.0);
    if !tilted {
        return Need::Plain;
    }
    let readable = view.kind != "SPLINE" || group(view.groups, 73).is_some_and(|n| n as usize == view.groups.iter().filter(|t| t.code == 10).count());
    Need::Needs { readable }
}

// ---- the files ----

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/activation")
}

fn read_case(case: &Case) -> RecordGraph {
    let text = std::fs::read_to_string(dir().join(format!("{}.dxf", case.name))).unwrap_or_else(|e| panic!("{}: {e}", case.name));
    RecordGraph::build(&sections(&parse(text.as_bytes()).unwrap()), &stand_in).unwrap()
}

/// I1 and I2 over an activated graph.
fn check_invariants(case: &Case, graph: &RecordGraph, outcomes: &[Outcome]) {
    for edge in graph.graph().edges() {
        let (a, b) = match *edge {
            Edge::Contains { definition, member } => (definition, member),
            Edge::Refers { user, definition } => (user, definition),
            Edge::Owns { insert, attrib } => (insert, attrib),
        };
        let pair = (outcomes[a.0 as usize], outcomes[b.0 as usize]);
        assert!(
            pair.0 == Outcome::Raw || pair.1 == Outcome::Raw || pair.0 == pair.1,
            "{} I1: {:?} is {:?} and {:?} is {:?}",
            case.name,
            graph.key(a),
            pair.0,
            graph.key(b),
            pair.1
        );
    }
    for (i, node) in graph.graph().nodes().iter().enumerate() {
        if matches!(node.kind, NodeKind::Record(_)) && matches!(node.need, Need::Needs { .. }) {
            assert_ne!(outcomes[i], Outcome::Legacy, "{} I2: {:?} needs Canonical", case.name, graph.key(NodeId(i as u32)));
        }
    }
}

fn check(name: &str) {
    let case = cases().into_iter().find(|c| c.name == name).expect("a named fixture");
    let graph = read_case(&case);
    let activation = activate(graph.graph(), &case.policy);
    let outcomes: Vec<Outcome> = (0..graph.graph().nodes().len()).map(|i| activation.outcome(NodeId(i as u32)).expect("an outcome")).collect();
    for (label, key, want) in &case.expect {
        let node = graph.node(key).unwrap_or_else(|| panic!("{name}: no node for {label} ({key:?})"));
        assert_eq!(outcomes[node.0 as usize], *want, "{name}: {label}");
    }
    assert_eq!(graph.graph().nodes().len(), case.expect.len(), "{name}: every node of the file is asserted");
    check_invariants(&case, &graph, &outcomes);
    if !case.failures.is_empty() {
        let first = graph.node(&case.expect[0].1).unwrap();
        let component = activation.components.iter().find(|c| c.nodes.contains(&first)).expect("its component");
        for kind in &case.failures {
            assert!(component.failures.iter().any(|f| matches_failure(*kind, f)), "{name}: {kind:?} in {:?}", component.failures);
        }
    }
}

#[test]
fn the_files_on_disk_are_the_generated_ones() {
    for case in cases() {
        let on_disk = std::fs::read_to_string(dir().join(format!("{}.dxf", case.name))).unwrap_or_default();
        assert_eq!(on_disk, case.text, "{}: run write_activation_fixtures", case.name);
    }
}

#[test]
#[ignore = "rewrites tests/data/activation/*.dxf"]
fn write_activation_fixtures() {
    std::fs::create_dir_all(dir()).unwrap();
    for case in cases() {
        std::fs::write(dir().join(format!("{}.dxf", case.name)), &case.text).unwrap();
    }
}

#[test]
fn a1_tilted_insert_over_plain_definition() {
    check("a1_tilted_insert_over_plain_definition");
}

#[test]
fn a2_needing_attribute_on_plain_insert() {
    check("a2_needing_attribute_on_plain_insert");
}

#[test]
fn a3_canonical_container_reaches_nested_target() {
    check("a3_canonical_container_reaches_nested_target");
}

#[test]
fn a4_plain_insert_over_definition_with_tilted_child() {
    check("a4_plain_insert_over_definition_with_tilted_child");
}

#[test]
fn a5_nested_chain() {
    check("a5_nested_chain");
}

#[test]
fn a6_shared_definition_in_model_and_in_another_definition() {
    check("a6_shared_definition_in_model_and_in_another_definition");
}

#[test]
fn a7_mleader_block_content_tilted() {
    check("a7_mleader_block_content_tilted");
}

#[test]
fn a8_dimension_graphics_block_tilted() {
    check("a8_dimension_graphics_block_tilted");
}

#[test]
fn a9_xref_with_tilted_child() {
    check("a9_xref_with_tilted_child");
}

#[test]
fn a10_cycle_all_plain() {
    check("a10_cycle_all_plain");
}

#[test]
fn a11_cycle_with_needs_seed() {
    check("a11_cycle_with_needs_seed");
}

#[test]
fn b1_needs_member_in_component_that_cannot_activate() {
    check("b1_needs_member_in_component_that_cannot_activate");
}

#[test]
fn b2_needs_insert_with_attributes_raw() {
    check("b2_needs_insert_with_attributes_raw");
}

#[test]
fn b3_component_with_unmasked_family() {
    check("b3_component_with_unmasked_family");
}

#[test]
fn b4_unreferenced_definition_with_needing_member() {
    check("b4_unreferenced_definition_with_needing_member");
}

#[test]
fn b5_dimension_with_star_d_block() {
    check("b5_dimension_with_star_d_block");
}

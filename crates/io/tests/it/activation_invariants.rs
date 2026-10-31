//! I3 and I4 of the activation plan (section 2.6), on the fixtures and on the repository drawings:
//! (I3) a drawing with no record that needs Canonical activates nothing and reads exactly as the Legacy read does;
//! (I4) the outcome of every record is the same under a shuffled record order and under a repeated read.

use std::collections::BTreeMap;

use cadcraft_dxf::{Section, Tag, parse, records, sections};
use cadcraft_io::activation::records::RecordGraph;
use cadcraft_io::activation::{ACTIVE_FAMILIES, ComponentOutcome, Failure, Need, NodeId, Outcome, Policy, ReaderMode, ReaderPolicy, activate};

use crate::activation_fixtures::{cases, predicate, source_predicate};

/// Every record of the file by its name (the handle, or `*NAME` for a definition) and its outcome, and the outcome and reasons of
/// each component without node ids (the ids follow the record order).
#[derive(Debug, PartialEq, Eq)]
struct Outcomes {
    records: BTreeMap<String, Outcome>,
    components: Vec<String>,
}

fn kinds(failures: &[Failure]) -> Vec<String> {
    let mut v: Vec<String> = failures
        .iter()
        .map(|f| match f {
            Failure::DefinitionBeforeStage6(_) => "definition before stage 6".into(),
            Failure::UnreadableNeed(_) => "unreadable need".into(),
            Failure::Cycle(_) => "cycle".into(),
            Failure::TooDeep(_) => "too deep".into(),
            other => format!("{other:?}"),
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

fn outcomes(secs: &[Section], policy: &Policy) -> Outcomes {
    let graph = RecordGraph::build(secs, &predicate(secs)).unwrap();
    let activation = activate(graph.graph(), policy);
    let name = |node: NodeId| match graph.key(node) {
        Some(cadcraft_io::activation::records::NodeKey::Definition(name)) => format!("*{name}"),
        _ => graph.handle(node).map_or_else(|| format!("{:?}", graph.key(node)), str::to_string),
    };
    let records = (0..graph.graph().nodes().len() as u32).map(NodeId).map(|n| (name(n), activation.outcome(n).expect("an outcome"))).collect();
    let mut components: Vec<String> =
        activation.components.iter().map(|c| format!("{:?} {} {:?}", c.outcome, c.nodes.len(), kinds(&c.failures))).collect();
    components.sort();
    Outcomes { records, components }
}

// ---- shuffling ----

/// A small deterministic generator (the test needs a permutation, not randomness).
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n.max(1)
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

type Record = (String, Vec<Tag>);

/// The records of a container as units: a record with the followers that belong to it.
fn units(recs: Vec<Record>) -> Vec<Vec<Record>> {
    let mut out: Vec<Vec<Record>> = Vec::new();
    for rec in recs {
        let follower = matches!(rec.0.as_str(), "ATTRIB" | "VERTEX" | "SEQEND") && !out.is_empty();
        if follower {
            out.last_mut().unwrap().push(rec);
        } else {
            out.push(vec![rec]);
        }
    }
    out
}

fn text_of(recs: &[Record]) -> String {
    let mut out = String::new();
    for (kind, groups) in recs {
        out.push_str(&format!("0\n{kind}\n"));
        for t in groups {
            out.push_str(&format!("{}\n{}\n", t.code, t.str()));
        }
    }
    out
}

/// The sections with the order of the blocks, of the records of each block and of the entities shuffled.
fn shuffled(secs: &[Section], rng: &mut Lcg) -> Vec<Section> {
    let mut out = Vec::new();
    for section in secs {
        let recs = records(&section.tags);
        let text = match section.name.as_str() {
            "ENTITIES" => {
                let mut u = units(recs);
                rng.shuffle(&mut u);
                u.concat()
            }
            "BLOCKS" => {
                // Blocks as units (BLOCK .. ENDBLK), each with its members shuffled.
                let mut blocks: Vec<Vec<Record>> = Vec::new();
                for rec in recs {
                    if rec.0 == "BLOCK" || blocks.is_empty() {
                        blocks.push(vec![rec]);
                    } else {
                        blocks.last_mut().unwrap().push(rec);
                    }
                }
                let mut shuffled_blocks: Vec<Vec<Record>> = blocks
                    .into_iter()
                    .map(|mut block| {
                        let end = block.iter().position(|r| r.0 == "ENDBLK").map(|i| block.split_off(i));
                        let head = block.remove(0);
                        let mut members = units(block);
                        rng.shuffle(&mut members);
                        let mut rebuilt = vec![head];
                        rebuilt.extend(members.concat());
                        rebuilt.extend(end.unwrap_or_default());
                        rebuilt
                    })
                    .collect();
                rng.shuffle(&mut shuffled_blocks);
                shuffled_blocks.concat()
            }
            _ => recs,
        };
        let mut section_text = format!("0\nSECTION\n2\n{}\n{}0\nENDSEC\n", section.name, text_of(&text));
        section_text.push_str("0\nEOF\n");
        out.extend(sections(&parse(section_text.as_bytes()).unwrap()));
    }
    out
}

fn sections_of(text: &str) -> Vec<Section> {
    sections(&parse(text.as_bytes()).unwrap())
}

fn flat(secs: &[Section]) -> String {
    secs.iter().flat_map(|s| s.tags.iter()).map(|t| format!("{} {}\n", t.code, t.str())).collect()
}

#[test]
fn i4_every_fixture_has_the_same_outcome_in_a_shuffled_record_order() {
    let mut reordered = 0;
    for case in cases() {
        let secs = sections_of(&case.text);
        let first = outcomes(&secs, &case.policy);
        let mut rng = Lcg(0x5EED);
        for round in 0..12 {
            let other = shuffled(&secs, &mut rng);
            reordered += usize::from(flat(&other) != flat(&secs));
            assert_eq!(outcomes(&other, &case.policy), first, "{} round {round}", case.name);
        }
    }
    assert!(reordered > 50, "the shuffle must move records: {reordered} of 192 reads were in another order");
}

#[test]
fn i4_a_repeated_read_gives_the_same_outcome() {
    for case in cases() {
        let secs = sections_of(&case.text);
        assert_eq!(outcomes(&secs, &case.policy), outcomes(&sections_of(&case.text), &case.policy), "{}", case.name);
    }
}

fn serialized_entities(drawing: &cadcraft_doc::Drawing) -> BTreeMap<(String, u64), Vec<u8>> {
    let mut entities = BTreeMap::new();
    let mut collect = |container: String, store: &cadcraft_doc::EntityStore| {
        for entity in store.iter() {
            entities.insert((container.clone(), entity.handle.0), serde_json::to_vec(entity.as_ref()).unwrap());
        }
    };
    collect("ENTITIES".into(), &drawing.model);
    for layout in &drawing.layouts {
        collect(format!("PAPER/{}", layout.name), &layout.entities);
    }
    for (name, block) in &drawing.blocks {
        collect(name.to_ascii_uppercase(), &block.entities);
    }
    entities
}

#[derive(Debug, PartialEq, Eq)]
struct ReadOutcomes {
    revisions: BTreeMap<(String, u64), cadcraft_doc::GeometryRevision>,
    raw_units: BTreeMap<(String, u64), Vec<u8>>,
    notes: Vec<String>,
}

fn read_outcomes(drawing: &cadcraft_doc::Drawing) -> ReadOutcomes {
    let mut out = ReadOutcomes { revisions: BTreeMap::new(), raw_units: BTreeMap::new(), notes: drawing.read_notes.clone() };
    let mut collect = |container: String, store: &cadcraft_doc::EntityStore| {
        for entity in store.iter() {
            let key = (container.clone(), entity.handle.0);
            out.revisions.insert(key.clone(), entity.geometry_revision);
            if let cadcraft_doc::EntityKind::Raw(raw) = &entity.kind {
                out.raw_units.insert(key, serde_json::to_vec(raw).unwrap());
                for record in raw.records() {
                    out.revisions.insert((container.clone(), record.handle().0), cadcraft_doc::GeometryRevision::RawSource);
                }
            } else {
                for attribute in entity.kind.attributes() {
                    if let Some(handle) = attribute.handle {
                        out.revisions.insert((container.clone(), handle.0), attribute.geometry_revision);
                    }
                }
            }
        }
    };
    collect("ENTITIES".into(), &drawing.model);
    for layout in &drawing.layouts {
        collect(format!("PAPER/{}", layout.name), &layout.entities);
    }
    for (name, block) in &drawing.blocks {
        collect(name.to_ascii_uppercase(), &block.entities);
    }
    out.notes.sort();
    out
}

#[test]
fn i4_repeated_and_shuffled_raw_reads_match_the_graph_and_keep_source_units() {
    use cadcraft_io::activation::records::{Container, NodeKey};
    let policy = ReaderPolicy::new(ReaderMode::Off, ACTIVE_FAMILIES).with_raw_path();
    let mut moved = 0;
    // These source units are complete. Other graph fixtures deliberately omit INSERT SEQENDs;
    // their malformed-read refusal belongs to M1, not a silent Legacy outcome in this proof.
    let mut fixtures: Vec<_> = cases()
        .into_iter()
        .filter(|case| {
            matches!(
                case.name,
                "b2_needs_insert_with_attributes_raw" | "b4_unreferenced_definition_with_needing_member" | "b5_dimension_with_star_d_block"
            )
        })
        .map(|case| (case.name, case.text))
        .collect();
    fixtures.push(("a10_plain_cycle_control", String::from_utf8(include_bytes!("../data/activation/a10_plain_cycle_control.dxf").to_vec()).unwrap()));
    for (name, text) in fixtures {
        let bytes = text.as_bytes();
        let drawing = cadcraft_io::read_with_need(bytes, "fixture.dxf", policy, &source_predicate(bytes)).unwrap();
        let first = read_outcomes(&drawing);
        assert!(!first.raw_units.is_empty(), "{name} must exercise an actual Raw read");
        let secs = sections_of(&text);
        let graph = RecordGraph::build(&secs, &predicate(&secs)).unwrap();
        let activation = activate(graph.graph(), &policy.graph_policy());
        for index in 0..graph.graph().nodes().len() as u32 {
            let node = NodeId(index);
            let Some(NodeKey::Record { container, .. }) = graph.key(node) else { continue };
            let Some(handle) = graph.handle(node).and_then(cadcraft_doc::Handle::parse_hex) else { continue };
            let container = match container {
                Container::Entities => "ENTITIES".into(),
                Container::Block(name) => name.clone(),
            };
            let expected = match activation.outcome(node).unwrap() {
                Outcome::Legacy => cadcraft_doc::GeometryRevision::LegacyProjectedV1,
                Outcome::Canonical => cadcraft_doc::GeometryRevision::CanonicalV2,
                Outcome::Raw => cadcraft_doc::GeometryRevision::RawSource,
            };
            assert_eq!(first.revisions.get(&(container, handle.0)), Some(&expected), "{name}: actual read must consume the graph decision");
        }
        let repeated = cadcraft_io::read_with_need(bytes, "fixture.dxf", policy, &source_predicate(bytes)).unwrap();
        assert_eq!(read_outcomes(&repeated), first, "{name}: repeated read");
        let mut rng = Lcg(0x5EED);
        for round in 0..8 {
            let other = shuffled(&secs, &mut rng);
            moved += usize::from(flat(&other) != flat(&secs));
            let bytes = format!(
                "{}0\nEOF\n",
                other
                    .iter()
                    .map(|section| format!("0\nSECTION\n2\n{}\n{}0\nENDSEC\n", section.name, text_of(&records(&section.tags))))
                    .collect::<String>()
            )
            .into_bytes();
            let drawing = cadcraft_io::read_with_need(&bytes, "fixture.dxf", policy, &source_predicate(&bytes)).unwrap();
            assert_eq!(read_outcomes(&drawing), first, "{name}: shuffled actual read {round}");
        }
    }
    assert!(moved > 4, "the fixture shuffles must change source order");
}

#[test]
fn a10_parsed_plain_cycle_stays_legacy_when_no_record_needs_activation() {
    let bytes = include_bytes!("../data/activation/a10_plain_cycle_control.dxf");
    let secs = sections(&parse(bytes).unwrap());
    // This is the graph's all-Plain control. Conservative classification of unproved INSERTs remains separate.
    let graph = RecordGraph::build(&secs, &|_| Need::Plain).unwrap();
    let activation = activate(graph.graph(), &open());
    assert!(activation.outcomes.iter().all(|outcome| *outcome == Outcome::Legacy));
    let calls = std::cell::Cell::new(0);
    let plain = |_: &cadcraft_io::activation::records::RecordView<'_>| {
        calls.set(calls.get() + 1);
        Need::Plain
    };
    let graph_read = cadcraft_io::read_with_need(bytes, "a10.dxf", ReaderPolicy::default().with_raw_path(), &plain).unwrap();
    assert_eq!(calls.get(), 4);
    assert!(read_outcomes(&graph_read).raw_units.is_empty());
    assert!(read_outcomes(&graph_read).revisions.values().all(|revision| *revision == cadcraft_doc::GeometryRevision::LegacyProjectedV1));
    assert_eq!(graph_read, cadcraft_io::read_with_policy(bytes, "a10.dxf", ReaderPolicy::default()).unwrap());
}

// ---- I3 ----

fn open() -> Policy {
    Policy { mask: cadcraft_io::activation::FamilySet::ALL, readers: cadcraft_io::activation::FamilySet::ALL, definitions: true }
}

#[test]
fn i3_a_drawing_with_no_needing_record_activates_nothing_and_reads_as_the_legacy_read() {
    for (name, bytes) in [
        ("apartment.dxf", include_bytes!("../../../../examples/apartment.dxf").as_slice()),
        ("attrib_fields.dxf", include_bytes!("../data/attrib_fields.dxf").as_slice()),
        ("named_shape_style.dxf", include_bytes!("../data/named_shape_style.dxf").as_slice()),
    ] {
        let secs = sections_of(&String::from_utf8_lossy(bytes));
        let graph = RecordGraph::build(&secs, &|_| Need::Plain).unwrap();
        let activation = activate(graph.graph(), &open());
        assert_eq!(activation.activated(), 0, "{name}");
        assert!((0..graph.graph().nodes().len() as u32).all(|i| activation.outcome(NodeId(i)) == Some(Outcome::Legacy)), "{name}");
        assert!(activation.components.iter().all(|c| c.outcome == ComponentOutcome::Legacy), "{name}");
        // The read in every mode is the Legacy read, field for field.
        let legacy = cadcraft_io::read_with_policy(bytes, name, ReaderPolicy::new(ReaderMode::Off, ACTIVE_FAMILIES)).unwrap();
        for mode in [ReaderMode::Safe, ReaderMode::All] {
            let calls = std::cell::Cell::new(0);
            let plain = |_: &cadcraft_io::activation::records::RecordView<'_>| {
                calls.set(calls.get() + 1);
                Need::Plain
            };
            let policy = ReaderPolicy::new(mode, ACTIVE_FAMILIES).with_raw_path();
            assert!(policy.needs_graph());
            let read = cadcraft_io::read_with_need(bytes, name, policy, &plain).unwrap();
            let records = graph.graph().nodes().iter().filter(|node| matches!(node.kind, cadcraft_io::activation::NodeKind::Record(_))).count();
            assert_eq!(calls.get(), records, "{name} {mode:?}: the read must classify every record through the graph");
            assert_eq!(read, legacy, "{name} {mode:?}");
            assert_eq!(serialized_entities(&read), serialized_entities(&legacy), "{name} {mode:?}: serialized finite fields must keep signed zero");
            assert_eq!(cadcraft_io::write_dxf(&read), cadcraft_io::write_dxf(&legacy), "{name} {mode:?}: DXF bytes");
            assert_eq!(
                cadcraft_io::write(&read, "plain.dwg").unwrap(),
                cadcraft_io::write(&legacy, "plain.dwg").unwrap(),
                "{name} {mode:?}: DWG bytes"
            );
        }
    }
}

#[test]
fn i3_a_fixture_of_proved_plain_records_has_no_needing_node_and_stays_legacy() {
    let text = crate::activation_fixtures::file("", &[], &[crate::activation_fixtures::line("E1"), crate::activation_fixtures::circle("E2", false)]);
    let graph = RecordGraph::build(&sections_of(&text), &source_predicate(text.as_bytes())).unwrap();
    assert!(!graph.graph().nodes().iter().any(|n| matches!(n.need, Need::Needs { .. })));
    let activation = activate(graph.graph(), &open());
    assert_eq!(activation.activated(), 0);
}

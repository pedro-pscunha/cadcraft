//! The activation graph (stage 0, plan section 2): named fixtures A1 to A11 and B1 to B5, the assertions I1 (agreement among
//! non-Raw nodes), I2 (a `needs` node is never Legacy), I3 (no `needs`: all Legacy), I4 (order independence) and I5 (the
//! activated count).

use cadcraft_io::activation::{
    Component, ComponentOutcome, Edge, Failure, Family, FamilySet, Graph, GraphError, Need, Node, NodeId, NodeKind, Outcome, Policy, activate,
};

const PLAIN: Need = Need::Plain;
const NEEDS: Need = Need::Needs { readable: true };
const UNREADABLE: Need = Need::Needs { readable: false };

/// A graph described by names: the same fixture can be built in any node and edge order.
#[derive(Clone)]
struct Fixture {
    nodes: Vec<(&'static str, Node)>,
    edges: Vec<(&'static str, &'static str, &'static str)>,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture { nodes: Vec::new(), edges: Vec::new() }
    }

    fn record(mut self, name: &'static str, family: Family, need: Need) -> Fixture {
        self.nodes.push((name, Node::record(family, need)));
        self
    }

    fn definition(mut self, name: &'static str) -> Fixture {
        self.nodes.push((name, Node::definition()));
        self
    }

    fn contains(mut self, definition: &'static str, member: &'static str) -> Fixture {
        self.edges.push(("contains", definition, member));
        self
    }

    fn refers(mut self, user: &'static str, definition: &'static str) -> Fixture {
        self.edges.push(("refers", user, definition));
        self
    }

    fn owns(mut self, insert: &'static str, attrib: &'static str) -> Fixture {
        self.edges.push(("owns", insert, attrib));
        self
    }

    /// The graph with the nodes and edges in the given order (`rotate` positions to the left, `reverse` flips both lists).
    fn build(&self, rotate: usize, reverse: bool) -> (Graph, Vec<&'static str>) {
        let mut nodes = self.nodes.clone();
        let mut edges = self.edges.clone();
        if !nodes.is_empty() {
            let by = rotate % nodes.len();
            nodes.rotate_left(by);
        }
        if !edges.is_empty() {
            let by = rotate % edges.len();
            edges.rotate_left(by);
        }
        if reverse {
            nodes.reverse();
            edges.reverse();
        }
        let names: Vec<&'static str> = nodes.iter().map(|(name, _)| *name).collect();
        let id = |name: &str| NodeId(names.iter().position(|n| *n == name).expect("a named node") as u32);
        let edges = edges
            .iter()
            .map(|(kind, a, b)| match *kind {
                "contains" => Edge::Contains { definition: id(a), member: id(b) },
                "refers" => Edge::Refers { user: id(a), definition: id(b) },
                _ => Edge::Owns { insert: id(a), attrib: id(b) },
            })
            .collect();
        (Graph::new(nodes.into_iter().map(|(_, node)| node).collect(), edges).expect("a valid fixture"), names)
    }
}

/// Everything allowed: stage 6 with a reader for every family.
fn open() -> Policy {
    Policy { mask: FamilySet::ALL, readers: FamilySet::ALL, definitions: true }
}

/// Stage 1 to 5: definitions cannot activate yet.
fn before_stage_6() -> Policy {
    Policy { definitions: false, ..open() }
}

struct Result {
    outcomes: Vec<(&'static str, Outcome)>,
    components: Vec<Component>,
    graph: Graph,
    names: Vec<&'static str>,
    activated: usize,
}

impl Result {
    fn outcome(&self, name: &str) -> Outcome {
        self.outcomes.iter().find(|(n, _)| *n == name).map(|(_, o)| *o).expect("a named node")
    }

    fn all(&self, names: &[&str], want: Outcome) {
        for name in names {
            assert_eq!(self.outcome(name), want, "{name}");
        }
    }

    fn component_of(&self, name: &str) -> &Component {
        let at = self.names.iter().position(|n| *n == name).expect("a named node");
        self.components.iter().find(|c| c.nodes.contains(&NodeId(at as u32))).expect("its component")
    }
}

/// Run the fixture in its own order and check I1 to I4 on it, with the order flipped and rotated.
fn run(fixture: &Fixture, policy: &Policy) -> Result {
    let first = run_once(fixture, policy, 0, false);
    check_agreement(&first);
    check_needs_preserved(&first);
    for rotate in 1..=7 {
        for reverse in [false, true] {
            let other = run_once(fixture, policy, rotate, reverse);
            let mut a = first.outcomes.clone();
            let mut b = other.outcomes.clone();
            a.sort_by_key(|(name, _)| *name);
            b.sort_by_key(|(name, _)| *name);
            assert_eq!(a, b, "I4: the outcome follows the record order (rotate {rotate}, reverse {reverse})");
            let kinds = |r: &Result| {
                let mut v: Vec<(ComponentOutcome, Vec<Failure>)> = r.components.iter().map(|c| (c.outcome, failure_kinds(&c.failures))).collect();
                v.sort_by(|x, y| format!("{x:?}").cmp(&format!("{y:?}")));
                v
            };
            assert_eq!(kinds(&first), kinds(&other), "I4: components (rotate {rotate}, reverse {reverse})");
        }
    }
    first
}

/// The failures without node ids (the ids change with the order).
fn failure_kinds(failures: &[Failure]) -> Vec<Failure> {
    failures
        .iter()
        .map(|f| match f {
            Failure::DefinitionBeforeStage6(_) => Failure::DefinitionBeforeStage6(NodeId(0)),
            Failure::UnreadableNeed(_) => Failure::UnreadableNeed(NodeId(0)),
            Failure::Cycle(_) => Failure::Cycle(NodeId(0)),
            Failure::TooDeep(_) => Failure::TooDeep(NodeId(0)),
            other => *other,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn run_once(fixture: &Fixture, policy: &Policy, rotate: usize, reverse: bool) -> Result {
    let (graph, names) = fixture.build(rotate, reverse);
    let activation = activate(&graph, policy);
    let outcomes = names.iter().enumerate().map(|(i, name)| (*name, activation.outcome(NodeId(i as u32)).expect("an outcome"))).collect();
    Result { outcomes, components: activation.components.clone(), graph, names, activated: activation.activated() }
}

/// I1: no Canonical node shares an agreement edge with a Legacy one. A Raw node is the one allowed exception.
fn check_agreement(r: &Result) {
    let outcome = |id: NodeId| r.outcomes[id.0 as usize].1;
    for edge in r.graph.edges() {
        let (a, b) = match *edge {
            Edge::Contains { definition, member } => (definition, member),
            Edge::Refers { user, definition } => (user, definition),
            Edge::Owns { insert, attrib } => (insert, attrib),
        };
        let pair = (outcome(a), outcome(b));
        assert!(
            pair.0 == Outcome::Raw || pair.1 == Outcome::Raw || pair.0 == pair.1,
            "I1: {} is {:?} and {} is {:?}",
            r.names[a.0 as usize],
            pair.0,
            r.names[b.0 as usize],
            pair.1
        );
    }
}

/// I2: a node that needs Canonical is Canonical or Raw, never Legacy.
fn check_needs_preserved(r: &Result) {
    for (i, node) in r.graph.nodes().iter().enumerate() {
        if matches!(node.kind, NodeKind::Record(_)) && matches!(node.need, Need::Needs { .. }) {
            assert_ne!(r.outcomes[i].1, Outcome::Legacy, "I2: {} needs Canonical", r.names[i]);
        }
    }
}

use Family::{Attrib, Circle, Dimension, Insert, Line, MLeader, Spline};

#[test]
fn a1_a_tilted_insert_over_a_plain_definition_activates_the_definition() {
    let f = Fixture::new()
        .record("insert", Insert, NEEDS)
        .definition("def")
        .record("a", Line, PLAIN)
        .record("b", Circle, PLAIN)
        .refers("insert", "def")
        .contains("def", "a")
        .contains("def", "b");
    let r = run(&f, &open());
    r.all(&["insert", "def", "a", "b"], Outcome::Canonical);
    assert_eq!(r.activated, 4, "I5: the closure moves 4 nodes");
}

#[test]
fn a2_a_needing_attribute_on_a_plain_insert_activates_the_insert_and_its_definition() {
    let f = Fixture::new()
        .record("insert", Insert, PLAIN)
        .record("attrib", Attrib, NEEDS)
        .definition("def")
        .record("m", Line, PLAIN)
        .refers("insert", "def")
        .owns("insert", "attrib")
        .contains("def", "m");
    let r = run(&f, &open());
    r.all(&["insert", "attrib", "def", "m"], Outcome::Canonical);
}

#[test]
fn a3_a_canonical_container_reaches_the_definition_its_nested_insert_names() {
    let f = Fixture::new()
        .definition("outer")
        .record("needing", Spline, NEEDS)
        .record("nested", Insert, PLAIN)
        .definition("inner")
        .record("leaf", Line, PLAIN)
        .contains("outer", "needing")
        .contains("outer", "nested")
        .refers("nested", "inner")
        .contains("inner", "leaf");
    let r = run(&f, &open());
    r.all(&["outer", "needing", "nested", "inner", "leaf"], Outcome::Canonical);
}

#[test]
fn a4_a_plain_insert_over_a_definition_with_a_tilted_child_is_canonical() {
    let f = Fixture::new()
        .record("insert", Insert, PLAIN)
        .definition("def")
        .record("tilted", Circle, NEEDS)
        .record("flat", Line, PLAIN)
        .refers("insert", "def")
        .contains("def", "tilted")
        .contains("def", "flat");
    run(&f, &open()).all(&["insert", "def", "tilted", "flat"], Outcome::Canonical);
}

#[test]
fn a5_a_nested_chain_is_one_component() {
    let f = Fixture::new()
        .record("top", Insert, PLAIN)
        .definition("d1")
        .record("i1", Insert, PLAIN)
        .definition("d2")
        .record("i2", Insert, PLAIN)
        .definition("d3")
        .record("deep", Circle, NEEDS)
        .refers("top", "d1")
        .contains("d1", "i1")
        .refers("i1", "d2")
        .contains("d2", "i2")
        .refers("i2", "d3")
        .contains("d3", "deep");
    run(&f, &open()).all(&["top", "d1", "i1", "d2", "i2", "d3", "deep"], Outcome::Canonical);
}

#[test]
fn a6_a_shared_definition_pulls_every_user_with_it() {
    let f = Fixture::new()
        .record("model_user", Insert, PLAIN)
        .definition("other")
        .record("block_user", Insert, PLAIN)
        .definition("shared")
        .record("tilted", Circle, NEEDS)
        .refers("model_user", "shared")
        .contains("other", "block_user")
        .refers("block_user", "shared")
        .contains("shared", "tilted")
        .record("unrelated", Line, PLAIN);
    let r = run(&f, &open());
    r.all(&["model_user", "other", "block_user", "shared", "tilted"], Outcome::Canonical);
    r.all(&["unrelated"], Outcome::Legacy);
}

#[test]
fn a7_a_multileader_with_tilted_block_content_is_canonical() {
    let f = Fixture::new()
        .record("mleader", MLeader, PLAIN)
        .definition("content")
        .record("tilted", Circle, NEEDS)
        .refers("mleader", "content")
        .contains("content", "tilted");
    run(&f, &open()).all(&["mleader", "content", "tilted"], Outcome::Canonical);
}

#[test]
fn a8_a_dimension_with_a_tilted_graphics_block_is_canonical() {
    let f = Fixture::new()
        .record("dim", Dimension, PLAIN)
        .definition("star_d")
        .record("line", Line, NEEDS)
        .record("text", Family::MText, PLAIN)
        .refers("dim", "star_d")
        .contains("star_d", "line")
        .contains("star_d", "text");
    run(&f, &open()).all(&["dim", "star_d", "line", "text"], Outcome::Canonical);
}

#[test]
fn a9_an_xref_with_a_tilted_child_is_canonical_with_its_host_insert() {
    let f = Fixture::new()
        .record("xref_insert", Insert, PLAIN)
        .definition("xref")
        .record("tilted", Spline, NEEDS)
        .refers("xref_insert", "xref")
        .contains("xref", "tilted");
    run(&f, &open()).all(&["xref_insert", "xref", "tilted"], Outcome::Canonical);
}

fn cycle(seed: Need) -> Fixture {
    Fixture::new()
        .definition("a")
        .record("a_member", Spline, seed)
        .record("a_insert", Insert, PLAIN)
        .definition("b")
        .record("b_insert", Insert, PLAIN)
        .contains("a", "a_member")
        .contains("a", "a_insert")
        .refers("a_insert", "b")
        .contains("b", "b_insert")
        .refers("b_insert", "a")
}

#[test]
fn a10_a_plain_cycle_stays_legacy_with_no_raw_node() {
    let r = run(&cycle(PLAIN), &open());
    r.all(&["a", "a_member", "a_insert", "b", "b_insert"], Outcome::Legacy);
    assert_eq!(r.component_of("a").outcome, ComponentOutcome::Legacy);
}

#[test]
fn a11_a_cycle_with_a_needs_seed_keeps_the_needing_member_raw_and_the_rest_legacy() {
    let r = run(&cycle(NEEDS), &open());
    assert_eq!(r.outcome("a_member"), Outcome::Raw);
    r.all(&["a", "a_insert", "b", "b_insert"], Outcome::Legacy);
    let component = r.component_of("a");
    assert_eq!(component.outcome, ComponentOutcome::Raw);
    assert!(component.failures.iter().any(|f| matches!(f, Failure::Cycle(_))), "{:?}", component.failures);
}

#[test]
fn a_self_referencing_definition_is_a_cycle() {
    let f =
        Fixture::new().definition("d").record("i", Insert, PLAIN).record("m", Circle, NEEDS).contains("d", "i").contains("d", "m").refers("i", "d");
    let r = run(&f, &open());
    assert_eq!(r.outcome("m"), Outcome::Raw);
    assert!(r.component_of("d").failures.iter().any(|f| matches!(f, Failure::Cycle(_))));
}

#[test]
fn b1_a_needing_member_that_cannot_be_read_is_raw_beside_legacy_siblings() {
    let f = Fixture::new()
        .definition("def")
        .record("bad", Spline, UNREADABLE)
        .record("x", Line, PLAIN)
        .record("y", Circle, PLAIN)
        .contains("def", "bad")
        .contains("def", "x")
        .contains("def", "y");
    let r = run(&f, &open());
    assert_eq!(r.outcome("bad"), Outcome::Raw);
    r.all(&["def", "x", "y"], Outcome::Legacy);
    assert!(r.component_of("def").failures.iter().any(|f| matches!(f, Failure::UnreadableNeed(_))));
}

#[test]
fn b2_a_needing_insert_with_attributes_is_one_raw_unit() {
    // The INSERT needs Canonical and INSERT has no canonical reader yet: the INSERT and every ATTRIB are Raw, the definition stays Legacy.
    let f = Fixture::new()
        .record("insert", Insert, NEEDS)
        .record("a1", Attrib, PLAIN)
        .record("a2", Attrib, PLAIN)
        .definition("def")
        .record("m", Line, PLAIN)
        .owns("insert", "a1")
        .owns("insert", "a2")
        .refers("insert", "def")
        .contains("def", "m");
    let policy = Policy { readers: FamilySet::ALL.without(Insert), ..open() };
    let r = run(&f, &policy);
    r.all(&["insert", "a1", "a2"], Outcome::Raw);
    r.all(&["def", "m"], Outcome::Legacy);
    assert!(r.component_of("def").failures.contains(&Failure::NoReader(Insert)));
}

#[test]
fn b2_a_needing_attribute_makes_its_insert_and_all_attributes_raw() {
    let f = Fixture::new()
        .record("insert", Insert, PLAIN)
        .record("a1", Attrib, NEEDS)
        .record("a2", Attrib, PLAIN)
        .definition("def")
        .refers("insert", "def")
        .owns("insert", "a1")
        .owns("insert", "a2");
    let policy = Policy { readers: FamilySet::ALL.without(Attrib), ..open() };
    let r = run(&f, &policy);
    r.all(&["insert", "a1", "a2"], Outcome::Raw);
    r.all(&["def"], Outcome::Legacy);
}

#[test]
fn b3_a_family_outside_the_mask_keeps_the_needing_member_raw() {
    let f = Fixture::new().definition("def").record("s", Spline, NEEDS).record("x", Line, PLAIN).contains("def", "s").contains("def", "x");
    let policy = Policy { mask: FamilySet::ALL.without(Spline), ..open() };
    let r = run(&f, &policy);
    assert_eq!(r.outcome("s"), Outcome::Raw);
    r.all(&["def", "x"], Outcome::Legacy);
    assert!(r.component_of("def").failures.contains(&Failure::FamilyOutsideMask(Spline)));
}

#[test]
fn b3_a_plain_member_outside_the_mask_also_blocks_the_component() {
    // Canonical makes every member Canonical, so a plain Line outside the mask blocks the needing Circle.
    let f = Fixture::new().definition("def").record("c", Circle, NEEDS).record("x", Line, PLAIN).contains("def", "c").contains("def", "x");
    let policy = Policy { mask: FamilySet::ALL.without(Line), ..open() };
    let r = run(&f, &policy);
    assert_eq!(r.outcome("c"), Outcome::Raw);
    assert_eq!(r.outcome("x"), Outcome::Legacy);
}

#[test]
fn b4_an_unreferenced_definition_with_a_needing_member_is_raw_before_stage_6() {
    let f = Fixture::new().definition("def").record("c", Circle, NEEDS).record("x", Line, PLAIN).contains("def", "c").contains("def", "x");
    let r = run(&f, &before_stage_6());
    assert_eq!(r.outcome("c"), Outcome::Raw);
    r.all(&["def", "x"], Outcome::Legacy);
    assert!(r.component_of("def").failures.iter().any(|f| matches!(f, Failure::DefinitionBeforeStage6(_))));
    // At stage 6 the same definition activates.
    run(&f, &open()).all(&["def", "c", "x"], Outcome::Canonical);
}

#[test]
fn b5_a_dimension_with_its_star_d_block_is_raw_before_stage_6() {
    let f =
        Fixture::new().record("dim", Dimension, NEEDS).definition("star_d").record("l", Line, PLAIN).refers("dim", "star_d").contains("star_d", "l");
    let r = run(&f, &before_stage_6());
    assert_eq!(r.outcome("dim"), Outcome::Raw);
    r.all(&["star_d", "l"], Outcome::Legacy);
}

#[test]
fn a_top_level_record_activates_before_stage_6_when_it_needs_it() {
    let f = Fixture::new().record("tilted", Circle, NEEDS).record("flat", Line, PLAIN);
    let r = run(&f, &before_stage_6());
    assert_eq!(r.outcome("tilted"), Outcome::Canonical);
    assert_eq!(r.outcome("flat"), Outcome::Legacy);
}

#[test]
fn i3_a_graph_with_no_needing_record_is_all_legacy() {
    let f = Fixture::new()
        .record("top", Insert, PLAIN)
        .definition("def")
        .record("m", Circle, PLAIN)
        .record("lone", Line, PLAIN)
        .refers("top", "def")
        .contains("def", "m");
    for policy in [open(), before_stage_6(), Policy { mask: FamilySet::NONE, ..open() }] {
        let r = run(&f, &policy);
        assert!(r.outcomes.iter().all(|(_, o)| *o == Outcome::Legacy));
        assert!(r.components.iter().all(|c| c.outcome == ComponentOutcome::Legacy));
        assert_eq!(r.activated, 0);
    }
}

#[test]
fn a_chain_of_nested_definitions_is_deep_at_seventeen_and_not_at_sixteen() {
    fn chain(depth: usize) -> (Vec<Node>, Vec<Edge>) {
        // definition k holds an INSERT that names definition k+1; the last one holds the needing circle.
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for k in 0..depth {
            let def = NodeId(nodes.len() as u32);
            nodes.push(Node::definition());
            if k + 1 < depth {
                let insert = NodeId(nodes.len() as u32);
                nodes.push(Node::record(Insert, PLAIN));
                edges.push(Edge::Contains { definition: def, member: insert });
                edges.push(Edge::Refers { user: insert, definition: NodeId(insert.0 + 1) });
            } else {
                let seed = NodeId(nodes.len() as u32);
                nodes.push(Node::record(Circle, NEEDS));
                edges.push(Edge::Contains { definition: def, member: seed });
            }
        }
        (nodes, edges)
    }
    let outcome = |depth: usize| {
        let (nodes, edges) = chain(depth);
        let seed = NodeId(nodes.len() as u32 - 1);
        let activation = activate(&Graph::new(nodes, edges).expect("a chain"), &open());
        (activation.outcome(seed).expect("seed"), activation.components.iter().flat_map(|c| c.failures.clone()).collect::<Vec<_>>())
    };
    assert_eq!(outcome(cadcraft_doc::MAX_BLOCK_DEPTH).0, Outcome::Canonical);
    let (deep, failures) = outcome(cadcraft_doc::MAX_BLOCK_DEPTH + 1);
    assert_eq!(deep, Outcome::Raw);
    assert!(failures.iter().any(|f| matches!(f, Failure::TooDeep(_))), "{failures:?}");
}

#[test]
fn a_hundred_thousand_nested_definitions_do_not_overflow_the_stack() {
    let n = 100_000u32;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for k in 0..n {
        let def = NodeId(nodes.len() as u32);
        nodes.push(Node::definition());
        let insert = NodeId(nodes.len() as u32);
        nodes.push(Node::record(Insert, if k == 0 { NEEDS } else { PLAIN }));
        edges.push(Edge::Contains { definition: def, member: insert });
        if k + 1 < n {
            edges.push(Edge::Refers { user: insert, definition: NodeId(insert.0 + 1) });
        }
    }
    let activation = activate(&Graph::new(nodes, edges).expect("a long chain"), &open());
    assert_eq!(activation.outcome(NodeId(1)), Some(Outcome::Raw));
    assert!(activation.components.iter().flat_map(|c| c.failures.iter()).any(|f| matches!(f, Failure::TooDeep(_))));
}

#[test]
fn a_graph_with_a_bad_edge_is_refused() {
    let nodes = vec![Node::definition(), Node::record(Line, PLAIN)];
    assert_eq!(
        Graph::new(nodes.clone(), vec![Edge::Contains { definition: NodeId(0), member: NodeId(5) }]).unwrap_err(),
        GraphError::UnknownNode(NodeId(5))
    );
    assert_eq!(
        Graph::new(nodes.clone(), vec![Edge::Contains { definition: NodeId(1), member: NodeId(1) }]).unwrap_err(),
        GraphError::WrongKind(NodeId(1))
    );
    assert_eq!(
        Graph::new(nodes.clone(), vec![Edge::Refers { user: NodeId(0), definition: NodeId(0) }]).unwrap_err(),
        GraphError::WrongKind(NodeId(0))
    );
    let two = vec![Node::definition(), Node::definition(), Node::record(Line, PLAIN)];
    let both = vec![Edge::Contains { definition: NodeId(0), member: NodeId(2) }, Edge::Contains { definition: NodeId(1), member: NodeId(2) }];
    assert_eq!(Graph::new(two, both).unwrap_err(), GraphError::TwoOwners(NodeId(2)));
}

#[test]
fn the_closure_size_is_reported_per_component() {
    let f = Fixture::new()
        .record("insert", Insert, NEEDS)
        .definition("def")
        .record("a", Line, PLAIN)
        .refers("insert", "def")
        .contains("def", "a")
        .record("alone", Line, PLAIN);
    let r = run(&f, &open());
    assert_eq!(r.component_of("insert").nodes.len(), 3);
    assert_eq!(r.component_of("alone").nodes.len(), 1);
    assert_eq!(r.activated, 3);
}

//! The activation graph of the canonical readers: which records of a drawing are read `CanonicalV2`, which stay `LegacyProjectedV1`
//! and which are kept raw (stage 0 of the reader activation plan, section 2).
//!
//! The graph is a pure description: records and block definitions are nodes, and every revision-agreement relation is an edge.
//! A Canonical parent cannot hold a Legacy child and a Legacy parent cannot hold a Canonical one, so every edge constrains both
//! ends. The edges are therefore undirected for the revision: the nodes that are joined by edges form a component, and a component
//! has one revision. No reader calls [`activate`] yet.
//!
//! A component with no `needs` node (a record that Legacy would lose data from) is Legacy. A component with a `needs` node is
//! Canonical when it can activate ([`Failure`] lists why it cannot), else each `needs` unit is Raw and everything else stays
//! Legacy. A Raw unit is an exception to revision agreement: it is neither Legacy nor Canonical.

pub mod records;

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_doc::MAX_BLOCK_DEPTH;

/// A node of a [`Graph`]: its index in the node list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

pub use cadcraft_doc::{Family, FamilySet};

/// The environment variable that overrides [`ACTIVATION_DEFAULT`] (`off`, `safe` or `all`).
pub const READER_ENV: &str = "CADCRAFT_READER";

/// The mode of this build when the environment does not say: `safe`, the steady state of the plan.
pub const ACTIVATION_DEFAULT: ReaderMode = ReaderMode::Safe;

/// The families this build activates: each stage adds its own. Stage 0 activates none, so every mode reads as the reader did before.
pub const ACTIVE_FAMILIES: FamilySet = FamilySet::NONE;

/// The families whose canonical reader the reader calls. No reader calls one yet.
pub const WIRED_FAMILIES: FamilySet = FamilySet::NONE;

/// The rollback switch of the activation (plan v3, section 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderMode {
    /// Pre-activation behavior for the records Legacy loses nothing from. A record that needs Canonical is read Canonical if its
    /// component can activate under the mask, else kept Raw: `off` never drops Z, a plane or a frame silently.
    Off,
    /// The minimum set: only the components that need Canonical are Canonical, the rest is Legacy.
    Safe,
    /// Every record of the active families is Canonical (tests and measurements).
    All,
}

/// A setting that is not one of the modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderModeError(String);

impl std::fmt::Display for ReaderModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{READER_ENV}={:?} is not a reader mode: use off, safe or all", self.0)
    }
}

impl std::error::Error for ReaderModeError {}

impl ReaderMode {
    /// A mode from its setting, in any case.
    pub fn parse(text: &str) -> Result<ReaderMode, ReaderModeError> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(ReaderMode::Off),
            "safe" => Ok(ReaderMode::Safe),
            "all" => Ok(ReaderMode::All),
            _ => Err(ReaderModeError(text.to_string())),
        }
    }
}

/// What a read does about the activation: the mode of the switch and the families the build has active. The caller's own, so
/// concurrent reads with different policies share nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderPolicy {
    mode: ReaderMode,
    active: FamilySet,
}

impl Default for ReaderPolicy {
    fn default() -> Self {
        Self::new(ACTIVATION_DEFAULT, ACTIVE_FAMILIES)
    }
}

impl ReaderPolicy {
    pub fn new(mode: ReaderMode, active: FamilySet) -> ReaderPolicy {
        ReaderPolicy { mode, active }
    }

    /// The policy of a setting (the value of [`READER_ENV`]) over this build: [`ACTIVATION_DEFAULT`] when there is none.
    pub fn resolve(setting: Option<&str>) -> Result<ReaderPolicy, ReaderModeError> {
        let mode = setting.map_or(Ok(ACTIVATION_DEFAULT), ReaderMode::parse)?;
        Ok(ReaderPolicy::new(mode, ACTIVE_FAMILIES))
    }

    /// The policy of the environment this process runs in.
    pub fn current() -> Result<ReaderPolicy, ReaderModeError> {
        ReaderPolicy::resolve(std::env::var(READER_ENV).ok().as_deref())
    }

    pub fn mode(&self) -> ReaderMode {
        self.mode
    }

    /// The families the reader may read Canonical: none when off.
    pub fn mask(&self) -> FamilySet {
        if self.mode == ReaderMode::Off { FamilySet::NONE } else { self.active }
    }

    /// Whether a record that Legacy loses nothing from is read Canonical too: only in mode `all`.
    pub fn reads_plain_canonical(&self) -> bool {
        self.mode == ReaderMode::All
    }
}

/// Whether Legacy would lose data from a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Plain,
    /// The record needs Canonical. `readable`: its canonical reader accepts it.
    Needs {
        readable: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A record: top level in model or paper space, a member of a definition, an INSERT, an ATTRIB.
    Record(Family),
    /// A block definition: a named block, a `*D` dimension graphics block, a MULTILEADER content block or an xref block.
    Definition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// A definition has no payload of its own: its need is ignored.
    pub need: Need,
}

impl Node {
    pub fn record(family: Family, need: Need) -> Node {
        Node { kind: NodeKind::Record(family), need }
    }

    pub fn definition() -> Node {
        Node { kind: NodeKind::Definition, need: Need::Plain }
    }
}

/// A revision-agreement edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// `member` is one of the records of `definition`.
    Contains { definition: NodeId, member: NodeId },
    /// `user` names `definition`: an INSERT or MINSERT, a MULTILEADER and its content block, a DIMENSION and its `*D` block, an
    /// xref INSERT and the loaded xref.
    Refers { user: NodeId, definition: NodeId },
    /// `attrib` is an ATTRIB of the INSERT `insert`.
    Owns { insert: NodeId, attrib: NodeId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// An edge names a node that does not exist.
    UnknownNode(NodeId),
    /// An edge end has the wrong kind (a record where a definition is needed, or the reverse).
    WrongKind(NodeId),
    /// A record is contained in two definitions.
    TwoOwners(NodeId),
    /// More nodes than a [`NodeId`] holds.
    TooManyNodes,
}

#[derive(Clone, Debug)]
pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl Graph {
    /// A graph of `nodes` and `edges`, validated: every edge end exists and has the kind its edge needs, and a record belongs to at
    /// most one definition.
    pub fn new(nodes: Vec<Node>, edges: Vec<Edge>) -> Result<Graph, GraphError> {
        if u32::try_from(nodes.len()).is_err() {
            return Err(GraphError::TooManyNodes);
        }
        let kind = |id: NodeId| nodes.get(id.index()).map(|n| n.kind).ok_or(GraphError::UnknownNode(id));
        let want_record = |id: NodeId| match kind(id)? {
            NodeKind::Record(_) => Ok(()),
            NodeKind::Definition => Err(GraphError::WrongKind(id)),
        };
        let want_definition = |id: NodeId| match kind(id)? {
            NodeKind::Definition => Ok(()),
            NodeKind::Record(_) => Err(GraphError::WrongKind(id)),
        };
        let mut owned = BTreeSet::new();
        for edge in &edges {
            match *edge {
                Edge::Contains { definition, member } => {
                    want_definition(definition)?;
                    want_record(member)?;
                    if !owned.insert(member) {
                        return Err(GraphError::TwoOwners(member));
                    }
                }
                Edge::Refers { user, definition } => {
                    want_record(user)?;
                    want_definition(definition)?;
                }
                Edge::Owns { insert, attrib } => {
                    want_record(insert)?;
                    want_record(attrib)?;
                }
            }
        }
        Ok(Graph { nodes, edges })
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }
}

/// What the current stage allows.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// The families the reader mask turns on.
    pub mask: FamilySet,
    /// The families that have a canonical reader.
    pub readers: FamilySet,
    /// Stage 6 or later: a component with a definition node can activate. Before it, none can.
    pub definitions: bool,
}

/// The revision a node ends with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Legacy,
    Canonical,
    /// Kept as source records and read-only; an exception to revision agreement.
    Raw,
}

/// Why a component with a `needs` node cannot activate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Failure {
    /// The component has a definition node and the stage is before stage 6.
    DefinitionBeforeStage6(NodeId),
    /// A family in the component is outside the reader mask.
    FamilyOutsideMask(Family),
    /// A family in the component has no canonical reader.
    NoReader(Family),
    /// The canonical read of this `needs` node fails.
    UnreadableNeed(NodeId),
    /// This definition is on a reference cycle.
    Cycle(NodeId),
    /// This definition starts a chain of nested definitions deeper than [`MAX_BLOCK_DEPTH`].
    TooDeep(NodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentOutcome {
    /// No `needs` node.
    Legacy,
    /// A `needs` node, and the component activates.
    Canonical,
    /// A `needs` node, and the component cannot activate: its `needs` units are Raw.
    Raw,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// The nodes of the component, ascending.
    pub nodes: Vec<NodeId>,
    pub outcome: ComponentOutcome,
    /// Why it cannot activate (empty unless `outcome` is [`ComponentOutcome::Raw`]), ascending and without repeats.
    pub failures: Vec<Failure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    /// The outcome of every node, by node index.
    pub outcomes: Vec<Outcome>,
    /// The components, by their lowest node.
    pub components: Vec<Component>,
}

impl Activation {
    pub fn outcome(&self, id: NodeId) -> Option<Outcome> {
        self.outcomes.get(id.index()).copied()
    }

    /// The number of nodes the closure moves to Canonical, over all components (the size effect of a stage).
    pub fn activated(&self) -> usize {
        self.outcomes.iter().filter(|o| **o == Outcome::Canonical).count()
    }
}

/// The revision of every node of `graph` under `policy`. The result depends on the graph's structure only, not on the order of its
/// nodes or edges.
pub fn activate(graph: &Graph, policy: &Policy) -> Activation {
    let n = graph.nodes.len();
    let mut sets = Sets::new(n);
    for edge in &graph.edges {
        let (a, b) = match *edge {
            Edge::Contains { definition, member } => (definition, member),
            Edge::Refers { user, definition } => (user, definition),
            Edge::Owns { insert, attrib } => (insert, attrib),
        };
        sets.union(a.index(), b.index());
    }
    let nesting = Nesting::of(graph);
    // The members of each component, ascending.
    let mut members: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
    for index in 0..n {
        members.entry(sets.find(index)).or_default().push(NodeId(index as u32));
    }
    let mut outcomes = vec![Outcome::Legacy; n];
    let mut components: Vec<Component> = Vec::with_capacity(members.len());
    for nodes in members.into_values() {
        let needs = nodes.iter().any(|id| matches!(graph.nodes[id.index()].need, Need::Needs { .. }) && is_record(graph, *id));
        if !needs {
            components.push(Component { nodes, outcome: ComponentOutcome::Legacy, failures: Vec::new() });
            continue;
        }
        let failures = failures(graph, policy, &nesting, &nodes);
        if failures.is_empty() {
            for id in &nodes {
                outcomes[id.index()] = Outcome::Canonical;
            }
            components.push(Component { nodes, outcome: ComponentOutcome::Canonical, failures });
        } else {
            for id in raw_units(graph, &nodes) {
                outcomes[id.index()] = Outcome::Raw;
            }
            components.push(Component { nodes, outcome: ComponentOutcome::Raw, failures });
        }
    }
    components.sort_by_key(|c| c.nodes.first().copied());
    Activation { outcomes, components }
}

fn is_record(graph: &Graph, id: NodeId) -> bool {
    matches!(graph.nodes[id.index()].kind, NodeKind::Record(_))
}

/// Why `nodes` (one component with a `needs` node) cannot activate.
fn failures(graph: &Graph, policy: &Policy, nesting: &Nesting, nodes: &[NodeId]) -> Vec<Failure> {
    let mut out = BTreeSet::new();
    for id in nodes {
        let node = graph.nodes[id.index()];
        match node.kind {
            NodeKind::Definition => {
                if !policy.definitions {
                    out.insert(Failure::DefinitionBeforeStage6(*id));
                }
                if nesting.on_cycle.get(id.index()).copied().unwrap_or(false) {
                    out.insert(Failure::Cycle(*id));
                }
                if nesting.depth.get(id.index()).copied().unwrap_or(0) > MAX_BLOCK_DEPTH {
                    out.insert(Failure::TooDeep(*id));
                }
            }
            NodeKind::Record(family) => {
                if !policy.mask.contains(family) {
                    out.insert(Failure::FamilyOutsideMask(family));
                }
                if !policy.readers.contains(family) {
                    out.insert(Failure::NoReader(family));
                }
                if node.need == (Need::Needs { readable: false }) {
                    out.insert(Failure::UnreadableNeed(*id));
                }
            }
        }
    }
    out.into_iter().collect()
}

/// The nodes of a component that cannot activate and that become Raw: each `needs` record, and an INSERT with all its ATTRIBs when the
/// INSERT or one of its ATTRIBs needs Canonical (an ATTRIB cannot exist without its INSERT, and the unit keeps the record sequence).
fn raw_units(graph: &Graph, nodes: &[NodeId]) -> BTreeSet<NodeId> {
    let in_component: BTreeSet<NodeId> = nodes.iter().copied().collect();
    let needs = |id: NodeId| is_record(graph, id) && matches!(graph.nodes[id.index()].need, Need::Needs { .. });
    let mut raw: BTreeSet<NodeId> = nodes.iter().copied().filter(|id| needs(*id)).collect();
    let mut owned: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for edge in &graph.edges {
        if let Edge::Owns { insert, attrib } = *edge
            && in_component.contains(&insert)
        {
            owned.entry(insert).or_default().push(attrib);
        }
    }
    for (insert, attribs) in owned {
        if raw.contains(&insert) || attribs.iter().any(|a| raw.contains(a)) {
            raw.insert(insert);
            raw.extend(attribs);
        }
    }
    raw
}

/// Union-find with path halving and union by size.
struct Sets {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl Sets {
    fn new(n: usize) -> Sets {
        Sets { parent: (0..n).collect(), size: vec![1; n] }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        let (big, small) = if self.size[a] >= self.size[b] { (a, b) } else { (b, a) };
        self.parent[small] = big;
        self.size[big] += self.size[small];
    }
}

/// The nesting of definitions: a definition refers to another when one of its records names it.
struct Nesting {
    /// By node index (records: false).
    on_cycle: Vec<bool>,
    /// The number of definitions in the longest chain of nested definitions that starts at this one, itself included; 0 for a record.
    depth: Vec<usize>,
}

impl Nesting {
    fn of(graph: &Graph) -> Nesting {
        let n = graph.nodes.len();
        let mut owner: Vec<Option<usize>> = vec![None; n];
        for edge in &graph.edges {
            if let Edge::Contains { definition, member } = *edge {
                owner[member.index()] = Some(definition.index());
            }
        }
        let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
        for edge in &graph.edges {
            if let Edge::Refers { user, definition } = *edge
                && let Some(parent) = owner[user.index()]
            {
                next[parent].push(definition.index());
            }
        }
        for successors in &mut next {
            successors.sort_unstable();
            successors.dedup();
        }
        let (scc, count) = components_of(&next);
        let mut size = vec![0usize; count];
        for (node, c) in scc.iter().enumerate() {
            if graph.nodes[node].kind == NodeKind::Definition {
                size[*c] += 1;
            }
        }
        let mut on_cycle = vec![false; n];
        for (node, c) in scc.iter().enumerate() {
            on_cycle[node] = graph.nodes[node].kind == NodeKind::Definition && (size[*c] > 1 || next[node].contains(&node));
        }
        // Tarjan numbers a component after every component it reaches, so the chain depths follow in that order.
        let mut by_component: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (node, c) in scc.iter().enumerate() {
            if graph.nodes[node].kind == NodeKind::Definition {
                by_component[*c].push(node);
            }
        }
        let mut depth_of_component = vec![0usize; count];
        for c in 0..count {
            let reached = by_component[c]
                .iter()
                .flat_map(|node| next[*node].iter())
                .map(|succ| scc[*succ])
                .filter(|sc| *sc != c)
                .map(|sc| depth_of_component[sc])
                .max()
                .unwrap_or(0);
            if !by_component[c].is_empty() {
                depth_of_component[c] = reached + 1;
            }
        }
        let depth = (0..n).map(|node| if graph.nodes[node].kind == NodeKind::Definition { depth_of_component[scc[node]] } else { 0 }).collect();
        Nesting { on_cycle, depth }
    }
}

/// The strongly connected components of a directed graph by Tarjan's algorithm without recursion: each node's component number, and the
/// count. A component is numbered after every component it reaches.
fn components_of(next: &[Vec<usize>]) -> (Vec<usize>, usize) {
    let n = next.len();
    const UNSEEN: usize = usize::MAX;
    let mut index = vec![UNSEEN; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut component = vec![UNSEEN; n];
    let mut count = 0usize;
    let mut counter = 0usize;
    for root in 0..n {
        if index[root] != UNSEEN {
            continue;
        }
        // (node, next successor to visit)
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&(node, at)) = work.last() {
            if let Some(&succ) = next[node].get(at) {
                if let Some(top) = work.last_mut() {
                    top.1 += 1;
                }
                if index[succ] == UNSEEN {
                    index[succ] = counter;
                    low[succ] = counter;
                    counter += 1;
                    stack.push(succ);
                    on_stack[succ] = true;
                    work.push((succ, 0));
                } else if on_stack[succ] {
                    low[node] = low[node].min(index[succ]);
                }
            } else {
                work.pop();
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[node]);
                }
                if low[node] == index[node] {
                    while let Some(member) = stack.pop() {
                        on_stack[member] = false;
                        component[member] = count;
                        if member == node {
                            break;
                        }
                    }
                    count += 1;
                }
            }
        }
    }
    (component, count)
}

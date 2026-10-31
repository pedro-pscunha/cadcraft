//! The record adapter of the activation graph (plan v3, section 2.1 and 2.2): nodes and edges built from the parsed records of a
//! DXF, before any entity is built.
//!
//! This adapter is the one owner of the mapping from a parsed record to its [`NodeId`]. A record is named by a [`NodeKey`]: the
//! container it sits in (the ENTITIES section, or a BLOCK) and its position among the top-level records of that container in file
//! order. A reader that walks the records in file order builds the key as it goes and asks [`RecordGraph::node`] for the node;
//! nothing else maps a record to a node. The handle is not part of the key: a hostile file repeats or omits handles, and the reader
//! gives some records new ones.
//!
//! Followers are not nodes of their own, except the ATTRIBs of an INSERT: a VERTEX or a SEQEND is read with its POLYLINE (or INSERT),
//! so the parent's decision covers it. A record of a kind without a family (a proxy or an unknown entity) takes a place in the
//! ordinals but has no node.

use std::collections::HashMap;

use cadcraft_dxf::{Section, Tag, records};

use super::{ComponentOutcome, Edge, Failure, Family, Graph, GraphError, Need, Node, NodeId, Outcome, Policy, activate};

/// Where a top-level record sits.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Container {
    /// The ENTITIES section: model space and the active paper space.
    Entities,
    /// A BLOCK of the BLOCKS section, by its upper-case name: a definition, or a layout block (`*Paper_Space1`).
    Block(String),
}

impl Container {
    pub fn block(name: &str) -> Container {
        Container::Block(name.trim().to_ascii_uppercase())
    }
}

/// The identity of a node, whatever the reader later does with the record.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeKey {
    /// A block definition, by its upper-case name.
    Definition(String),
    /// A record. `ordinal` counts the top-level records of the container in file order (ATTRIB, VERTEX and SEQEND followers are
    /// not counted; a record without a node is). `attrib` is the place of an ATTRIB under its INSERT.
    Record { container: Container, ordinal: u32, attrib: Option<u32> },
}

impl NodeKey {
    pub fn definition(name: &str) -> NodeKey {
        NodeKey::Definition(name.trim().to_ascii_uppercase())
    }

    pub fn record(container: Container, ordinal: u32, attrib: Option<u32>) -> NodeKey {
        NodeKey::Record { container, ordinal, attrib }
    }
}

/// The place of the next top-level record of each container, shared by the adapter and the reader so that both count alike. A block
/// repeated by name, and an ENTITIES section repeated in a damaged file, continue the count of their container.
#[derive(Debug, Default)]
pub struct Ordinals(HashMap<Container, u32>);

impl Ordinals {
    /// The place of the next top-level record of `container`.
    pub fn next(&mut self, container: &Container) -> u32 {
        let counter = self.0.entry(container.clone()).or_insert(0);
        let place = *counter;
        *counter = counter.saturating_add(1);
        place
    }
}

/// A record as the need function sees it.
pub struct RecordView<'a> {
    pub key: &'a NodeKey,
    pub kind: &'a str,
    pub family: Family,
    /// The groups of the record after its type.
    pub groups: &'a [Tag],
}

/// The activation graph of a file, with the key of every node.
pub struct RecordGraph {
    graph: Graph,
    keys: Vec<NodeKey>,
    /// The handle (group 5) of each node's record, for naming it in a report; a definition has none.
    handles: Vec<Option<String>>,
    by_key: HashMap<NodeKey, NodeId>,
}

/// The activation family of a DXF entity type, `None` for a type without a canonical reader (proxy and unknown entities).
pub fn family_of_kind(kind: &str) -> Option<Family> {
    Some(match kind.to_ascii_uppercase().as_str() {
        "CIRCLE" => Family::Circle,
        "ARC" => Family::Arc,
        "ELLIPSE" => Family::Ellipse,
        "LINE" => Family::Line,
        "POINT" => Family::Point,
        "RAY" => Family::Ray,
        "XLINE" => Family::XLine,
        "LWPOLYLINE" => Family::LwPolyline,
        "POLYLINE" => Family::Polyline,
        "SOLID" => Family::Solid,
        "TRACE" => Family::Trace,
        "3DFACE" => Family::Face3d,
        "SPLINE" => Family::Spline,
        "LEADER" => Family::Leader,
        "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => Family::Dimension,
        "TEXT" => Family::Text,
        "MTEXT" => Family::MText,
        "ATTDEF" => Family::AttDef,
        "ATTRIB" => Family::Attrib,
        "TOLERANCE" => Family::Tolerance,
        "HATCH" => Family::Hatch,
        "WIPEOUT" => Family::Wipeout,
        "MLINE" => Family::MLine,
        "HELIX" => Family::Helix,
        "IMAGE" => Family::Image,
        "ACAD_TABLE" => Family::Table,
        "INSERT" => Family::Insert,
        "MULTILEADER" | "MLEADER" => Family::MLeader,
        "REGION" => Family::Region,
        "VIEWPORT" => Family::Viewport,
        _ => return None,
    })
}

/// The `*Model_Space` and `*Paper_Space` blocks hold the top-level records of a layout, not a definition.
fn is_layout_block(name: &str) -> bool {
    name.starts_with("*MODEL_SPACE") || name.starts_with("*PAPER_SPACE")
}

fn first(groups: &[Tag], code: i32) -> Option<String> {
    groups.iter().find(|t| t.code == code).map(|t| t.str().trim().to_string())
}

/// The block record handle of a MULTILEADER's block content: group 344 of the entity, else group 341 of a context that has block
/// contents (group 296 = 1). Measured on an AutoCAD file: both name the same handle.
fn mleader_block_handle(groups: &[Tag]) -> Option<String> {
    let mut depth = 0usize;
    let mut has_block = false;
    let mut in_context = None;
    for t in groups {
        let value = t.str();
        let value = value.trim();
        if (300..=305).contains(&t.code) && value.ends_with('{') {
            depth += 1;
            if value == "CONTEXT_DATA{" {
                in_context = Some(depth);
                has_block = false;
            }
        } else if (300..=305).contains(&t.code) && value == "}" {
            if in_context == Some(depth) {
                in_context = None;
            }
            depth = depth.saturating_sub(1);
        } else if depth == 0 && t.code == 344 {
            return Some(value.to_ascii_uppercase());
        } else if in_context == Some(depth) && t.code == 296 {
            has_block = t.i64() == 1;
        } else if in_context == Some(depth) && has_block && t.code == 341 {
            return Some(value.to_ascii_uppercase());
        }
    }
    None
}

/// Which block a record names, by the upper-case name or the block record handle.
enum Names {
    Block(String),
    Handle(String),
}

struct Builder<'a> {
    need: &'a dyn Fn(&RecordView) -> Need,
    nodes: Vec<Node>,
    keys: Vec<NodeKey>,
    handles: Vec<Option<String>>,
    by_key: HashMap<NodeKey, NodeId>,
    edges: Vec<Edge>,
    /// Upper-case block name → its definition node (the first of a repeated name).
    definitions: HashMap<String, NodeId>,
    references: Vec<(NodeId, Names)>,
    ordinals: Ordinals,
}

impl Builder<'_> {
    fn add(&mut self, key: NodeKey, node: Node, handle: Option<String>) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(node);
        self.handles.push(handle);
        self.keys.push(key.clone());
        self.by_key.insert(key, id);
        id
    }

    /// The top-level records of one container, in file order.
    fn container(&mut self, container: &Container, definition: Option<NodeId>, recs: &[(String, Vec<Tag>)]) {
        // The INSERT whose ATTRIBs follow, with its node, its place and the number of ATTRIBs so far.
        let mut insert: Option<(NodeId, u32, u32)> = None;
        for (kind, groups) in recs {
            if kind == "ATTRIB" {
                // An ATTRIB follows its INSERT; one with no INSERT before it is no record.
                if let Some((parent, parent_place, count)) = insert.as_mut() {
                    let (parent, parent_place, place) = (*parent, *parent_place, *count);
                    *count += 1;
                    let key = NodeKey::record(container.clone(), parent_place, Some(place));
                    let need = (self.need)(&RecordView { key: &key, kind, family: Family::Attrib, groups });
                    let attrib = self.add(key, Node::record(Family::Attrib, need), first(groups, 5));
                    self.edges.push(Edge::Owns { insert: parent, attrib });
                }
                continue;
            }
            if kind == "SEQEND" {
                insert = None;
                continue;
            }
            if kind == "VERTEX" {
                continue;
            }
            insert = None;
            let place = self.ordinals.next(container);
            let Some(family) = family_of_kind(kind) else { continue };
            let key = NodeKey::record(container.clone(), place, None);
            let need = (self.need)(&RecordView { key: &key, kind, family, groups });
            let id = self.add(key, Node::record(family, need), first(groups, 5));
            if let Some(definition) = definition {
                self.edges.push(Edge::Contains { definition, member: id });
            }
            match kind.as_str() {
                "INSERT" => {
                    insert = Some((id, place, 0));
                    if let Some(name) = first(groups, 2) {
                        self.references.push((id, Names::Block(name.to_ascii_uppercase())));
                    }
                }
                // A dimension draws a `*D` block and a table a `*T` block: group 2.
                "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" | "ACAD_TABLE" => {
                    if let Some(name) = first(groups, 2) {
                        self.references.push((id, Names::Block(name.to_ascii_uppercase())));
                    }
                }
                "MULTILEADER" | "MLEADER" => {
                    if let Some(handle) = mleader_block_handle(groups) {
                        self.references.push((id, Names::Handle(handle)));
                    }
                }
                _ => {}
            }
        }
    }
}

/// What a read decided for one component of the graph (I5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentReport {
    /// Entity records, including owned ATTRIB records, excluding definition nodes.
    pub records: usize,
    /// Block-definition nodes, separate from the records they contain.
    pub definitions: usize,
    pub activated_records: usize,
    pub activated_definitions: usize,
    pub outcome: ComponentOutcome,
    /// Why the component could not activate, when it needed to.
    pub failures: Vec<Failure>,
    /// The handles of the records kept Raw, in file order.
    pub raw: Vec<String>,
}

/// The activation decided for a file: one entry per component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadReport {
    pub components: Vec<ComponentReport>,
}

impl ReadReport {
    /// The records read Canonical, in all.
    pub fn activated(&self) -> usize {
        self.components.iter().map(|component| component.activated_records).sum()
    }
}

impl RecordGraph {
    /// What the activation of the graph under `policy` comes to, per component.
    pub fn report(&self, policy: &Policy) -> ReadReport {
        let activation = activate(&self.graph, policy);
        let components = activation
            .components
            .iter()
            .map(|component| {
                let outcome_of = |n: &NodeId| activation.outcome(*n);
                let mut records = 0;
                let mut definitions = 0;
                let mut activated_records = 0;
                let mut activated_definitions = 0;
                for node in &component.nodes {
                    let canonical = usize::from(outcome_of(node) == Some(Outcome::Canonical));
                    match self.graph.nodes()[node.0 as usize].kind {
                        super::NodeKind::Record(_) => {
                            records += 1;
                            activated_records += canonical;
                        }
                        super::NodeKind::Definition => {
                            definitions += 1;
                            activated_definitions += canonical;
                        }
                    }
                }
                ComponentReport {
                    records,
                    definitions,
                    activated_records,
                    activated_definitions,
                    outcome: component.outcome,
                    failures: component.failures.clone(),
                    raw: component
                        .nodes
                        .iter()
                        .filter(|n| outcome_of(n) == Some(Outcome::Raw))
                        .filter_map(|n| self.handle(*n).map(str::to_string))
                        .collect(),
                }
            })
            .collect();
        ReadReport { components }
    }

    /// The graph of the records of `sections`; `need` says whether Legacy would lose data from a record (the section 3 predicate;
    /// until it lands the caller passes a stand-in).
    pub fn build(sections: &[Section], need: &dyn Fn(&RecordView) -> Need) -> Result<RecordGraph, GraphError> {
        #[cfg(test)]
        crate::work_done::GRAPHS.with(|n| n.set(n.get() + 1));
        let mut b = Builder {
            need,
            nodes: Vec::new(),
            keys: Vec::new(),
            handles: Vec::new(),
            by_key: HashMap::new(),
            edges: Vec::new(),
            definitions: HashMap::new(),
            references: Vec::new(),
            ordinals: Ordinals::default(),
        };
        // Block record handle (upper case) → block name (upper case), across every TABLES section.
        let block_names: HashMap<String, String> = sections
            .iter()
            .filter(|section| section.name.eq_ignore_ascii_case("TABLES"))
            .flat_map(|section| records(&section.tags))
            .filter(|(kind, _)| kind == "BLOCK_RECORD")
            .filter_map(|(_, groups)| Some((first(&groups, 5)?.to_ascii_uppercase(), first(&groups, 2)?.to_ascii_uppercase())))
            .collect();
        // Every BLOCKS section first; repeated definitions continue their existing container ordinals.
        for blocks in sections.iter().filter(|section| section.name.eq_ignore_ascii_case("BLOCKS")) {
            let all = records(&blocks.tags);
            let mut at = 0;
            while at < all.len() {
                if all[at].0 != "BLOCK" {
                    at += 1;
                    continue;
                }
                let name = first(&all[at].1, 2).unwrap_or_default().to_ascii_uppercase();
                let end = (at + 1..all.len()).find(|&i| all[i].0 == "ENDBLK" || all[i].0 == "BLOCK").unwrap_or(all.len());
                let definition = (!is_layout_block(&name)).then(|| match b.definitions.get(&name) {
                    Some(id) => *id,
                    None => {
                        let id = b.add(NodeKey::definition(&name), Node::definition(), None);
                        b.definitions.insert(name.clone(), id);
                        id
                    }
                });
                b.container(&Container::Block(name), definition, &all[at + 1..end]);
                at = end;
            }
        }
        // A damaged file can split its entities over several ENTITIES sections: one container.
        for entities in sections.iter().filter(|s| s.name.eq_ignore_ascii_case("ENTITIES")) {
            b.container(&Container::Entities, None, &records(&entities.tags));
        }
        for (user, names) in std::mem::take(&mut b.references) {
            let name = match names {
                Names::Block(name) => Some(name),
                Names::Handle(handle) => block_names.get(&handle).cloned(),
            };
            if let Some(definition) = name.and_then(|n| b.definitions.get(&n).copied()) {
                b.edges.push(Edge::Refers { user, definition });
            }
        }
        let graph = Graph::new(b.nodes, b.edges)?;
        Ok(RecordGraph { graph, keys: b.keys, handles: b.handles, by_key: b.by_key })
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The node of a record: the one answer to "which node is this record".
    pub fn node(&self, key: &NodeKey) -> Option<NodeId> {
        self.by_key.get(key).copied()
    }

    /// The handle of a node's record, as the file wrote it (`None` for a definition, or a record without one).
    pub fn handle(&self, node: NodeId) -> Option<&str> {
        self.handles.get(node.0 as usize)?.as_deref()
    }

    /// The key of a node.
    pub fn key(&self, node: NodeId) -> Option<&NodeKey> {
        self.keys.get(node.0 as usize)
    }
}

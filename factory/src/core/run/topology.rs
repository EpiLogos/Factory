use super::{RunMapAddress, RunRef};
use crate::core::identity::{Ref, Revision};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, TopologyError> {
        let value = value.into();
        let mut characters = value.chars();
        let valid = characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
            && characters.all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            });
        if !valid {
            return Err(TopologyError::InvalidNodeId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for NodeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Destination,
    Position,
    Work,
    Decision,
    Candidate,
    Gate,
    Authority,
    NestedRun,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    Planned,
    Ready,
    Active,
    Blocked,
    Waiting,
    Satisfied,
    Returned,
    Superseded,
    Abandoned,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Requires,
    BranchesTo,
    ReturnsTo,
    Nests,
    ConvergesTo,
    Realises,
    Supersedes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub state: Option<NodeState>,
    pub semantic_ref: Option<Ref>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: EdgeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMap {
    run_ref: RunRef,
    topology_revision: Revision,
    nodes: BTreeMap<NodeId, TopologyNode>,
    edges: BTreeSet<TopologyEdge>,
}

impl RunMap {
    pub(crate) fn new(run_ref: RunRef, destination: String) -> Result<Self, TopologyError> {
        if destination.trim().is_empty() {
            return Err(TopologyError::EmptyDestination);
        }
        let destination_id = NodeId::new("destination")?;
        let destination_node = TopologyNode {
            id: destination_id.clone(),
            kind: NodeKind::Destination,
            label: destination,
            state: None,
            semantic_ref: None,
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(destination_id, destination_node);
        let map = Self {
            run_ref,
            topology_revision: Revision::INITIAL,
            nodes,
            edges: BTreeSet::new(),
        };
        map.validate()?;
        Ok(map)
    }

    pub fn run_ref(&self) -> &RunRef {
        &self.run_ref
    }

    pub fn address(&self) -> RunMapAddress {
        RunMapAddress::for_run(self.run_ref.clone())
    }

    pub fn topology_revision(&self) -> Revision {
        self.topology_revision
    }

    pub fn nodes(&self) -> &BTreeMap<NodeId, TopologyNode> {
        &self.nodes
    }

    pub fn edges(&self) -> &BTreeSet<TopologyEdge> {
        &self.edges
    }

    pub(crate) fn apply(&self, mutation: TopologyMutation) -> Result<Self, TopologyError> {
        let mut next = self.clone();
        next.apply_unvalidated(mutation)?;
        next.validate()?;
        next.topology_revision = next
            .topology_revision
            .next()
            .ok_or(TopologyError::RevisionOverflow)?;
        Ok(next)
    }

    fn apply_unvalidated(&mut self, mutation: TopologyMutation) -> Result<(), TopologyError> {
        match mutation {
            TopologyMutation::AddNode { node } => {
                if self.nodes.contains_key(&node.id) {
                    return Err(TopologyError::DuplicateNode(node.id));
                }
                self.nodes.insert(node.id.clone(), node);
            }
            TopologyMutation::AddEdge { edge } => {
                if !self.edges.insert(edge.clone()) {
                    return Err(TopologyError::DuplicateEdge(edge));
                }
            }
            TopologyMutation::SetNodeState { node_id, state } => {
                let node = self
                    .nodes
                    .get_mut(&node_id)
                    .ok_or_else(|| TopologyError::MissingNode(node_id.clone()))?;
                node.state = Some(state);
            }
            TopologyMutation::Batch { mutations } => {
                if mutations.is_empty() {
                    return Err(TopologyError::EmptyBatch);
                }
                for mutation in mutations {
                    self.apply_unvalidated(mutation)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), TopologyError> {
        let destinations = self
            .nodes
            .values()
            .filter(|node| node.kind == NodeKind::Destination)
            .collect::<Vec<_>>();
        if destinations.len() != 1 {
            return Err(TopologyError::DestinationCount(destinations.len()));
        }
        let destination = destinations[0];

        for (key, node) in &self.nodes {
            if key != &node.id {
                return Err(TopologyError::NodeKeyMismatch(key.clone()));
            }
            validate_node(&self.run_ref, node)?;
        }

        for edge in &self.edges {
            if edge.from == edge.to {
                return Err(TopologyError::SelfEdge(edge.from.clone()));
            }
            if !self.nodes.contains_key(&edge.from) {
                return Err(TopologyError::MissingNode(edge.from.clone()));
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(TopologyError::MissingNode(edge.to.clone()));
            }
        }

        let mut reachable = BTreeSet::new();
        let mut queue = VecDeque::from([destination.id.clone()]);
        while let Some(current) = queue.pop_front() {
            if !reachable.insert(current.clone()) {
                continue;
            }
            for edge in self
                .edges
                .iter()
                .filter(|edge| edge.from == current && edge.relation != EdgeKind::ReturnsTo)
            {
                queue.push_back(edge.to.clone());
            }
        }
        if let Some(unreachable) = self
            .nodes
            .keys()
            .find(|node_id| !reachable.contains(*node_id))
        {
            return Err(TopologyError::UnreachableNode(unreachable.clone()));
        }

        validate_requires_acyclic(&self.nodes, &self.edges)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TopologyMutation {
    AddNode { node: TopologyNode },
    AddEdge { edge: TopologyEdge },
    SetNodeState { node_id: NodeId, state: NodeState },
    Batch { mutations: Vec<TopologyMutation> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopologyError {
    InvalidNodeId(String),
    EmptyDestination,
    EmptyBatch,
    DestinationCount(usize),
    DuplicateNode(NodeId),
    DuplicateEdge(TopologyEdge),
    MissingNode(NodeId),
    SelfEdge(NodeId),
    UnreachableNode(NodeId),
    RequiresCycle,
    InvalidNodeState {
        node: NodeId,
        kind: NodeKind,
    },
    MissingSemanticRef {
        node: NodeId,
        expected_kind: &'static str,
    },
    WrongSemanticRef {
        node: NodeId,
        expected_kind: &'static str,
        actual_kind: String,
    },
    SelfNestedRun(NodeId),
    NodeKeyMismatch(NodeId),
    RevisionOverflow,
}

impl Display for TopologyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Run Map topology: {self:?}")
    }
}

impl Error for TopologyError {}

fn validate_node(run_ref: &RunRef, node: &TopologyNode) -> Result<(), TopologyError> {
    let state_allowed = matches!(node.kind, NodeKind::Position | NodeKind::Work);
    if state_allowed != node.state.is_some() {
        return Err(TopologyError::InvalidNodeState {
            node: node.id.clone(),
            kind: node.kind,
        });
    }

    match node.kind {
        NodeKind::Decision => validate_semantic_ref(node, "decision"),
        NodeKind::Candidate => validate_semantic_ref(node, "candidate"),
        NodeKind::NestedRun => {
            validate_semantic_ref(node, "run")?;
            if node.semantic_ref.as_ref() == Some(run_ref.as_ref()) {
                return Err(TopologyError::SelfNestedRun(node.id.clone()));
            }
            Ok(())
        }
        NodeKind::Destination | NodeKind::Gate | NodeKind::Authority => {
            if let Some(reference) = &node.semantic_ref {
                return Err(TopologyError::WrongSemanticRef {
                    node: node.id.clone(),
                    expected_kind: "none",
                    actual_kind: reference.kind().to_owned(),
                });
            }
            Ok(())
        }
        NodeKind::Work => {
            if let Some(reference) = &node.semantic_ref {
                if reference.kind() != "workflow-unit" {
                    return Err(TopologyError::WrongSemanticRef {
                        node: node.id.clone(),
                        expected_kind: "workflow-unit",
                        actual_kind: reference.kind().to_owned(),
                    });
                }
            }
            Ok(())
        }
        NodeKind::Position => Ok(()),
    }
}

fn validate_semantic_ref(
    node: &TopologyNode,
    expected_kind: &'static str,
) -> Result<(), TopologyError> {
    let reference =
        node.semantic_ref
            .as_ref()
            .ok_or_else(|| TopologyError::MissingSemanticRef {
                node: node.id.clone(),
                expected_kind,
            })?;
    if reference.kind() != expected_kind {
        return Err(TopologyError::WrongSemanticRef {
            node: node.id.clone(),
            expected_kind,
            actual_kind: reference.kind().to_owned(),
        });
    }
    Ok(())
}

fn validate_requires_acyclic(
    nodes: &BTreeMap<NodeId, TopologyNode>,
    edges: &BTreeSet<TopologyEdge>,
) -> Result<(), TopologyError> {
    let mut indegree = nodes
        .keys()
        .cloned()
        .map(|node| (node, 0usize))
        .collect::<BTreeMap<_, _>>();
    for edge in edges
        .iter()
        .filter(|edge| edge.relation == EdgeKind::Requires)
    {
        *indegree
            .get_mut(&edge.to)
            .expect("edge endpoints validated") += 1;
    }
    let mut queue = indegree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(node, _)| node.clone())
        .collect::<VecDeque<_>>();
    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        for edge in edges
            .iter()
            .filter(|edge| edge.relation == EdgeKind::Requires && edge.from == node)
        {
            let count = indegree
                .get_mut(&edge.to)
                .expect("edge endpoints validated");
            *count -= 1;
            if *count == 0 {
                queue.push_back(edge.to.clone());
            }
        }
    }
    if visited != nodes.len() {
        return Err(TopologyError::RequiresCycle);
    }
    Ok(())
}

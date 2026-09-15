//! Pure Rust model for choosing a route through a contract.
//!
//! The graph and route-locking rules live here so they can be tested without
//! Godot. "Map" is only the domain name for this node graph: this module does
//! not define a visual map, UI layout, or final UX. Presentation and scene
//! changes are handled separately by the Godot adapter.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterType {
    Entry,
    Combat,
    Event,
    Shop,
    Elite,
}

impl EncounterType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Combat => "combat",
            Self::Event => "event",
            Self::Shop => "shop",
            Self::Elite => "elite",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncounterNode {
    id: NodeId,
    encounter_type: EncounterType,
    next_nodes: Vec<NodeId>,
}

impl EncounterNode {
    pub fn new(
        id: NodeId,
        encounter_type: EncounterType,
        next_nodes: impl Into<Vec<NodeId>>,
    ) -> Self {
        Self {
            id,
            encounter_type,
            next_nodes: next_nodes.into(),
        }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn encounter_type(&self) -> EncounterType {
        self.encounter_type
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncounterSelection {
    pub node_id: NodeId,
    pub encounter_type: EncounterType,
}

/// Where a node stands from the player's point of view, for drawing the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    /// Finished, and the route moved on from it.
    Completed,
    /// Where the player is standing right now.
    Current,
    /// Reachable from the current node and not locked out.
    Available,
    /// Ruled out by choosing a different branch.
    Locked,
    /// Further along the contract, not reachable yet.
    Upcoming,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Current => "current",
            Self::Available => "available",
            Self::Locked => "locked",
            Self::Upcoming => "upcoming",
        }
    }
}

/// A node plus its status, so the map screen can draw the whole contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeProgress {
    pub node_id: NodeId,
    pub encounter_type: EncounterType,
    pub status: NodeStatus,
}

/// How far through the contract the player is.
///
/// The entry node is excluded: it is where a run starts, not an encounter the
/// player completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractProgress {
    pub completed: usize,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractMapError {
    EmptyMap,
    DuplicateNode(NodeId),
    DuplicateConnection { from: NodeId, to: NodeId },
    UnknownStartNode(NodeId),
    UnknownConnection { from: NodeId, to: NodeId },
    CyclicGraph,
    EncounterAlreadyInProgress(NodeId),
    NoEncounterInProgress,
    EncounterNotReachable { from: NodeId, requested: NodeId },
}

impl fmt::Display for ContractMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMap => formatter.write_str("the contract map contains no encounters"),
            Self::DuplicateNode(node_id) => {
                write!(formatter, "encounter {node_id} appears more than once")
            }
            Self::DuplicateConnection { from, to } => {
                write!(
                    formatter,
                    "encounter {from} links to encounter {to} more than once"
                )
            }
            Self::UnknownStartNode(node_id) => {
                write!(formatter, "starting encounter {node_id} does not exist")
            }
            Self::UnknownConnection { from, to } => {
                write!(
                    formatter,
                    "encounter {from} leads to missing encounter {to}"
                )
            }
            Self::CyclicGraph => formatter.write_str("the contract map contains a cycle"),
            Self::EncounterAlreadyInProgress(node_id) => write!(
                formatter,
                "encounter {node_id} is already in progress; another route cannot be chosen"
            ),
            Self::NoEncounterInProgress => formatter.write_str("there is no active map encounter"),
            Self::EncounterNotReachable { from, requested } => write!(
                formatter,
                "encounter {requested} is unavailable from encounter {from}"
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractMap {
    nodes: BTreeMap<NodeId, EncounterNode>,
    current_node: NodeId,
    locked_nodes: BTreeSet<NodeId>,
    active_node: Option<NodeId>,
    completed_nodes: BTreeSet<NodeId>,
}

impl ContractMap {
    pub fn new(
        nodes: impl IntoIterator<Item = EncounterNode>,
        start_node: NodeId,
    ) -> Result<Self, ContractMapError> {
        let mut node_map = BTreeMap::new();

        for node in nodes {
            let node_id = node.id;
            if node_map.insert(node_id, node).is_some() {
                return Err(ContractMapError::DuplicateNode(node_id));
            }
        }

        if node_map.is_empty() {
            return Err(ContractMapError::EmptyMap);
        }
        if !node_map.contains_key(&start_node) {
            return Err(ContractMapError::UnknownStartNode(start_node));
        }
        for node in node_map.values() {
            let mut connections = BTreeSet::new();
            for next_node in &node.next_nodes {
                if !connections.insert(*next_node) {
                    return Err(ContractMapError::DuplicateConnection {
                        from: node.id,
                        to: *next_node,
                    });
                }
                if !node_map.contains_key(next_node) {
                    return Err(ContractMapError::UnknownConnection {
                        from: node.id,
                        to: *next_node,
                    });
                }
            }
        }
        if graph_contains_cycle(&node_map) {
            return Err(ContractMapError::CyclicGraph);
        }

        Ok(Self {
            nodes: node_map,
            current_node: start_node,
            locked_nodes: BTreeSet::new(),
            active_node: None,
            completed_nodes: BTreeSet::new(),
        })
    }

    /// Authored data used until contract generation is implemented. The two
    /// opening routes rejoin before the final elite encounter.
    pub fn demo() -> Self {
        Self::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![4]),
                EncounterNode::new(3, EncounterType::Shop, vec![5]),
                EncounterNode::new(4, EncounterType::Combat, vec![5]),
                EncounterNode::new(5, EncounterType::Elite, vec![]),
            ],
            0,
        )
        .expect("the built-in contract graph must be valid")
    }

    pub fn current_encounter(&self) -> &EncounterNode {
        self.nodes
            .get(&self.current_node)
            .expect("a validated contract contains its current node")
    }

    pub fn selectable_encounters(&self) -> Vec<&EncounterNode> {
        if self.active_node.is_some() {
            return Vec::new();
        }

        self.current_encounter()
            .next_nodes
            .iter()
            .filter(|node_id| !self.locked_nodes.contains(node_id))
            .map(|node_id| {
                self.nodes
                    .get(node_id)
                    .expect("a validated contract only links to existing nodes")
            })
            .collect()
    }

    pub fn encounter_option(
        &self,
        requested_node: NodeId,
    ) -> Result<EncounterSelection, ContractMapError> {
        if let Some(active_node) = self.active_node {
            return Err(ContractMapError::EncounterAlreadyInProgress(active_node));
        }

        if self.locked_nodes.contains(&requested_node)
            || !self
                .current_encounter()
                .next_nodes
                .contains(&requested_node)
        {
            return Err(ContractMapError::EncounterNotReachable {
                from: self.current_node,
                requested: requested_node,
            });
        }

        let encounter = self
            .nodes
            .get(&requested_node)
            .expect("a validated contract only links to existing nodes");
        Ok(EncounterSelection {
            node_id: encounter.id,
            encounter_type: encounter.encounter_type,
        })
    }

    pub fn select_encounter(
        &mut self,
        requested_node: NodeId,
    ) -> Result<EncounterSelection, ContractMapError> {
        let selection = self.encounter_option(requested_node)?;
        let unchosen_branches: Vec<_> = self
            .current_encounter()
            .next_nodes
            .iter()
            .copied()
            .filter(|node_id| *node_id != requested_node)
            .collect();

        self.locked_nodes.extend(unchosen_branches);
        self.active_node = Some(requested_node);
        Ok(selection)
    }

    pub fn complete_current_encounter(&mut self) -> Result<(), ContractMapError> {
        let completed_node = self
            .active_node
            .take()
            .ok_or(ContractMapError::NoEncounterInProgress)?;

        // The node being left is behind the player now, which matters for the
        // entry node: it is never an encounter that gets completed, but it
        // should not keep reading as somewhere still ahead.
        let departed_node = self.current_node;
        self.current_node = completed_node;
        self.completed_nodes.insert(completed_node);
        self.completed_nodes.insert(departed_node);
        Ok(())
    }

    /// Every node with its current status, in id order, for drawing the map.
    pub fn node_progress(&self) -> Vec<NodeProgress> {
        let selectable: BTreeSet<NodeId> = self
            .selectable_encounters()
            .iter()
            .map(|node| node.id)
            .collect();

        self.nodes
            .values()
            .map(|node| {
                let status = if self.completed_nodes.contains(&node.id) {
                    NodeStatus::Completed
                } else if node.id == self.current_node {
                    NodeStatus::Current
                } else if self.locked_nodes.contains(&node.id) {
                    NodeStatus::Locked
                } else if selectable.contains(&node.id) {
                    NodeStatus::Available
                } else {
                    NodeStatus::Upcoming
                };

                NodeProgress {
                    node_id: node.id,
                    encounter_type: node.encounter_type,
                    status,
                }
            })
            .collect()
    }

    /// Completed encounters out of the total, excluding the entry node.
    pub fn progress(&self) -> ContractProgress {
        let total = self
            .nodes
            .values()
            .filter(|node| node.encounter_type != EncounterType::Entry)
            .count();

        let completed = self
            .completed_nodes
            .iter()
            .filter(|node_id| {
                self.nodes
                    .get(node_id)
                    .is_some_and(|node| node.encounter_type != EncounterType::Entry)
            })
            .count();

        ContractProgress { completed, total }
    }

    /// Ends a failed encounter without advancing past it. The chosen route
    /// remains committed, so the same encounter is the only retry offered.
    pub fn fail_current_encounter(&mut self) -> Result<(), ContractMapError> {
        self.active_node
            .take()
            .ok_or(ContractMapError::NoEncounterInProgress)?;
        Ok(())
    }
}

fn graph_contains_cycle(nodes: &BTreeMap<NodeId, EncounterNode>) -> bool {
    let mut incoming_edges: BTreeMap<NodeId, usize> =
        nodes.keys().copied().map(|node_id| (node_id, 0)).collect();

    for node in nodes.values() {
        for next_node in &node.next_nodes {
            *incoming_edges
                .get_mut(next_node)
                .expect("connections were validated before cycle detection") += 1;
        }
    }

    let mut ready: VecDeque<_> = incoming_edges
        .iter()
        .filter_map(|(node_id, count)| (*count == 0).then_some(*node_id))
        .collect();
    let mut visited = 0;

    while let Some(node_id) = ready.pop_front() {
        visited += 1;
        let node = nodes
            .get(&node_id)
            .expect("cycle traversal only queues existing nodes");
        for next_node in &node.next_nodes {
            let count = incoming_edges
                .get_mut(next_node)
                .expect("connections were validated before cycle detection");
            *count -= 1;
            if *count == 0 {
                ready.push_back(*next_node);
            }
        }
    }

    visited != nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branching_map() -> ContractMap {
        ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![]),
                EncounterNode::new(3, EncounterType::Elite, vec![2]),
            ],
            0,
        )
        .unwrap()
    }

    #[test]
    fn every_reachable_next_encounter_is_selectable_with_its_type() {
        let contract = branching_map();

        let selectable: Vec<_> = contract
            .selectable_encounters()
            .into_iter()
            .map(|node| (node.id(), node.encounter_type()))
            .collect();

        assert_eq!(
            selectable,
            vec![(1, EncounterType::Combat), (2, EncounterType::Event)]
        );
    }

    #[test]
    fn selecting_starts_an_encounter_and_completion_advances_to_it() {
        let mut contract = branching_map();

        let selected = contract.select_encounter(1).unwrap();

        assert_eq!(selected.node_id, 1);
        assert_eq!(selected.encounter_type, EncounterType::Combat);
        assert_eq!(contract.current_encounter().id(), 0);
        assert!(contract.selectable_encounters().is_empty());

        contract.complete_current_encounter().unwrap();
        assert_eq!(contract.current_encounter().id(), 1);
    }

    #[test]
    fn unchosen_branch_stays_locked_even_if_a_later_node_links_to_it() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();
        contract.complete_current_encounter().unwrap();
        contract.select_encounter(3).unwrap();
        contract.complete_current_encounter().unwrap();

        assert!(contract.selectable_encounters().is_empty());
        assert_eq!(
            contract.select_encounter(2),
            Err(ContractMapError::EncounterNotReachable {
                from: 3,
                requested: 2,
            })
        );
    }

    #[test]
    fn locked_or_non_adjacent_selection_does_not_mutate_the_contract() {
        let mut contract = branching_map();
        let before = contract.clone();

        assert!(matches!(
            contract.select_encounter(3),
            Err(ContractMapError::EncounterNotReachable { .. })
        ));
        assert_eq!(contract, before);
    }

    #[test]
    fn another_route_cannot_be_chosen_while_an_encounter_is_active() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();

        assert_eq!(
            contract.select_encounter(2),
            Err(ContractMapError::EncounterAlreadyInProgress(1))
        );
    }

    #[test]
    fn invalid_graph_connections_are_rejected() {
        let result = ContractMap::new([EncounterNode::new(0, EncounterType::Entry, vec![99])], 0);

        assert_eq!(
            result,
            Err(ContractMapError::UnknownConnection { from: 0, to: 99 })
        );
    }

    #[test]
    fn duplicate_outgoing_connections_are_rejected() {
        let result = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 1]),
                EncounterNode::new(1, EncounterType::Combat, vec![]),
            ],
            0,
        );

        assert_eq!(
            result,
            Err(ContractMapError::DuplicateConnection { from: 0, to: 1 })
        );
    }

    #[test]
    fn cyclic_contract_graphs_are_rejected() {
        let result = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1]),
                EncounterNode::new(1, EncounterType::Combat, vec![2]),
                EncounterNode::new(2, EncounterType::Event, vec![1]),
            ],
            0,
        );

        assert_eq!(result, Err(ContractMapError::CyclicGraph));
    }

    #[test]
    fn failed_encounter_can_be_retried_without_unlocking_its_sibling() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();

        contract.fail_current_encounter().unwrap();

        let selectable: Vec<_> = contract
            .selectable_encounters()
            .into_iter()
            .map(|node| node.id())
            .collect();
        assert_eq!(contract.current_encounter().id(), 0);
        assert_eq!(selectable, vec![1]);
    }
    fn status_of(contract: &ContractMap, node_id: NodeId) -> NodeStatus {
        contract
            .node_progress()
            .into_iter()
            .find(|entry| entry.node_id == node_id)
            .expect("node exists on the contract")
            .status
    }

    /// Acceptance scenario 3: a brand-new run has nothing completed.
    #[test]
    fn a_new_contract_has_no_completed_encounters_and_zero_progress() {
        let contract = ContractMap::demo();

        let progress = contract.progress();
        assert_eq!(progress.completed, 0);
        assert_eq!(
            progress.total, 5,
            "the demo contract has 5 encounters plus the entry node"
        );

        assert!(
            !contract
                .node_progress()
                .iter()
                .any(|entry| entry.status == NodeStatus::Completed),
            "nothing should be marked completed before anything is played"
        );
        assert_eq!(status_of(&contract, 0), NodeStatus::Current);
    }

    /// Acceptance scenario 1: completed encounters are marked, and the rest
    /// are visibly in a different state.
    #[test]
    fn completing_an_encounter_marks_it_and_leaves_others_distinct() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(status_of(&contract, 1), NodeStatus::Completed);
        assert_eq!(
            status_of(&contract, 2),
            NodeStatus::Locked,
            "the branch not taken is locked out"
        );
        assert_eq!(
            status_of(&contract, 3),
            NodeStatus::Available,
            "the next encounter on the chosen route can be selected"
        );
        assert_eq!(
            status_of(&contract, 5),
            NodeStatus::Upcoming,
            "later encounters are not reachable yet"
        );
    }

    /// Acceptance scenario 2: overall progress is countable, e.g. "2 of 5".
    #[test]
    fn progress_counts_completed_encounters_and_excludes_the_entry_node() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");
        assert_eq!(contract.progress().completed, 1);

        contract.select_encounter(3).expect("node 3 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        let progress = contract.progress();
        assert_eq!(progress.completed, 2);
        assert_eq!(progress.total, 5);
    }

    /// A failed encounter is not progress, so it must not be counted.
    #[test]
    fn a_failed_encounter_does_not_count_as_completed() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .fail_current_encounter()
            .expect("an encounter is active");

        assert_eq!(contract.progress().completed, 0);
        assert_ne!(status_of(&contract, 1), NodeStatus::Completed);
    }

    #[test]
    fn the_entry_node_reads_as_behind_you_once_the_run_moves_on() {
        let mut contract = ContractMap::demo();
        assert_eq!(status_of(&contract, 0), NodeStatus::Current);

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(
            status_of(&contract, 0),
            NodeStatus::Completed,
            "the entry node is behind the player, not still ahead"
        );
        assert_eq!(
            contract.progress().completed,
            1,
            "the entry node still must not count towards encounters completed"
        );
    }

    #[test]
    fn every_node_appears_exactly_once_in_the_progress_view() {
        let contract = ContractMap::demo();

        let entries = contract.node_progress();
        let mut ids: Vec<NodeId> = entries.iter().map(|entry| entry.node_id).collect();
        ids.sort_unstable();

        assert_eq!(ids, vec![0, 1, 2, 3, 4, 5]);
    }
}

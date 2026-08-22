//! Pure Rust model for choosing a route through a contract.
//!
//! This module deliberately has no Godot dependencies. The scene can ask for
//! the currently selectable encounters and submit a choice, while all rules
//! about reachability and branch lock-out remain unit-testable here.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Self::Entry => "Entry",
            Self::Combat => "Combat",
            Self::Event => "Event",
            Self::Shop => "Shop",
            Self::Elite => "Elite",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncounterNode {
    pub id: NodeId,
    pub encounter_type: EncounterType,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncounterSelection {
    pub node_id: NodeId,
    pub encounter_type: EncounterType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractMapError {
    EmptyMap,
    DuplicateNode(NodeId),
    UnknownStartNode(NodeId),
    UnknownConnection { from: NodeId, to: NodeId },
    EncounterAlreadyInProgress(NodeId),
    NoEncounterInProgress,
    EncounterNotReachable { from: NodeId, requested: NodeId },
}

impl fmt::Display for ContractMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMap => write!(formatter, "The contract map contains no encounters."),
            Self::DuplicateNode(node_id) => {
                write!(
                    formatter,
                    "Encounter {node_id} appears more than once in the map."
                )
            }
            Self::UnknownStartNode(node_id) => {
                write!(
                    formatter,
                    "The starting encounter {node_id} does not exist."
                )
            }
            Self::UnknownConnection { from, to } => write!(
                formatter,
                "Encounter {from} leads to missing encounter {to}."
            ),
            Self::EncounterAlreadyInProgress(node_id) => write!(
                formatter,
                "Encounter {node_id} is already in progress; another route cannot be chosen."
            ),
            Self::NoEncounterInProgress => {
                write!(formatter, "There is no active encounter to complete.")
            }
            Self::EncounterNotReachable { from, requested } => write!(
                formatter,
                "Encounter {requested} is locked because it is not reachable from encounter {from}."
            ),
        }
    }
}

impl Error for ContractMapError {}

#[derive(Debug, Clone)]
pub struct ContractMap {
    nodes: BTreeMap<NodeId, EncounterNode>,
    current_node: NodeId,
    chosen_path: Vec<NodeId>,
    encounter_in_progress: bool,
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
            for next_node in &node.next_nodes {
                if !node_map.contains_key(next_node) {
                    return Err(ContractMapError::UnknownConnection {
                        from: node.id,
                        to: *next_node,
                    });
                }
            }
        }

        Ok(Self {
            nodes: node_map,
            current_node: start_node,
            chosen_path: vec![start_node],
            encounter_in_progress: false,
        })
    }

    /// A small authored graph used by the Godot scene until contract generation
    /// is implemented. Its two routes rejoin before the final elite encounter.
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
        .expect("the built-in demo contract must be a valid graph")
    }

    pub fn current_encounter(&self) -> &EncounterNode {
        self.nodes
            .get(&self.current_node)
            .expect("a validated contract always contains its current node")
    }

    pub fn selectable_encounters(&self) -> Vec<&EncounterNode> {
        if self.encounter_in_progress {
            return Vec::new();
        }

        self.current_encounter()
            .next_nodes
            .iter()
            .map(|node_id| {
                self.nodes
                    .get(node_id)
                    .expect("a validated contract only links to existing nodes")
            })
            .collect()
    }

    pub fn select_encounter(
        &mut self,
        requested_node: NodeId,
    ) -> Result<EncounterSelection, ContractMapError> {
        if self.encounter_in_progress {
            return Err(ContractMapError::EncounterAlreadyInProgress(
                self.current_node,
            ));
        }

        if !self
            .current_encounter()
            .next_nodes
            .contains(&requested_node)
        {
            return Err(ContractMapError::EncounterNotReachable {
                from: self.current_node,
                requested: requested_node,
            });
        }

        self.current_node = requested_node;
        self.chosen_path.push(requested_node);
        self.encounter_in_progress = true;

        let encounter = self.current_encounter();
        Ok(EncounterSelection {
            node_id: encounter.id,
            encounter_type: encounter.encounter_type,
        })
    }

    /// Marks the active placeholder encounter complete so the next map choice
    /// can be demonstrated. A real encounter system can call the same method.
    pub fn complete_current_encounter(&mut self) -> Result<(), ContractMapError> {
        if !self.encounter_in_progress {
            return Err(ContractMapError::NoEncounterInProgress);
        }

        self.encounter_in_progress = false;
        Ok(())
    }

    #[cfg(test)]
    pub fn chosen_path(&self) -> &[NodeId] {
        &self.chosen_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branching_map() -> ContractMap {
        ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![4]),
                EncounterNode::new(3, EncounterType::Shop, vec![]),
                EncounterNode::new(4, EncounterType::Elite, vec![]),
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
            .map(|node| (node.id, node.encounter_type))
            .collect();

        assert_eq!(
            selectable,
            vec![(1, EncounterType::Combat), (2, EncounterType::Event)]
        );
    }

    #[test]
    fn selecting_an_encounter_advances_to_it() {
        let mut contract = branching_map();

        let selected = contract.select_encounter(2).unwrap();

        assert_eq!(selected.node_id, 2);
        assert_eq!(selected.encounter_type, EncounterType::Event);
        assert_eq!(contract.current_encounter().id, 2);
        assert_eq!(contract.chosen_path(), &[0, 2]);
    }

    #[test]
    fn selecting_an_encounter_locks_out_the_unchosen_branch() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();
        contract.complete_current_encounter().unwrap();

        let error = contract.select_encounter(2).unwrap_err();

        assert_eq!(
            error,
            ContractMapError::EncounterNotReachable {
                from: 1,
                requested: 2,
            }
        );
        assert_eq!(contract.current_encounter().id, 1);
        assert_eq!(contract.chosen_path(), &[0, 1]);
    }

    #[test]
    fn selecting_a_locked_encounter_does_not_change_the_contract() {
        let mut contract = branching_map();

        let error = contract.select_encounter(4).unwrap_err();

        assert_eq!(
            error,
            ContractMapError::EncounterNotReachable {
                from: 0,
                requested: 4,
            }
        );
        assert_eq!(contract.current_encounter().id, 0);
        assert_eq!(contract.chosen_path(), &[0]);
    }

    #[test]
    fn another_route_cannot_be_chosen_while_an_encounter_is_active() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();

        let error = contract.select_encounter(2).unwrap_err();

        assert_eq!(error, ContractMapError::EncounterAlreadyInProgress(1));
        assert_eq!(contract.chosen_path(), &[0, 1]);
    }

    #[test]
    fn invalid_graph_connections_are_rejected() {
        let error = ContractMap::new([EncounterNode::new(0, EncounterType::Entry, vec![99])], 0)
            .unwrap_err();

        assert_eq!(
            error,
            ContractMapError::UnknownConnection { from: 0, to: 99 }
        );
    }
}

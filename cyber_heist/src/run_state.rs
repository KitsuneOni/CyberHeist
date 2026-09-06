//! Contract lifecycle state and transitions.
//!
//! This module is plain Rust: it owns the run invariants without depending on
//! Godot, while `run_state_node.rs` adapts it for scenes.

use std::fmt;

use crate::contract_map::{
    ContractMap, ContractMapError, EncounterNode, EncounterSelection, EncounterType, NodeId,
};

impl EncounterType {
    fn parse(value: &str) -> Result<Self, RunStateError> {
        match value {
            "combat" => Ok(Self::Combat),
            "event" => Ok(Self::Event),
            "shop" => Ok(Self::Shop),
            "elite" => Ok(Self::Elite),
            unknown => Err(RunStateError::UnknownEncounterType(unknown.into())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractPhase {
    Hub,
    EncounterActive,
    Caught,
}

impl ContractPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hub => "hub",
            Self::EncounterActive => "encounter_active",
            Self::Caught => "caught",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveEncounter {
    id: String,
    encounter_type: EncounterType,
}

impl ActiveEncounter {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn encounter_type(&self) -> EncounterType {
        self.encounter_type
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunState {
    phase: ContractPhase,
    active_encounter: Option<ActiveEncounter>,
    contract_map: ContractMap,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            phase: ContractPhase::Hub,
            active_encounter: None,
            contract_map: ContractMap::demo(),
        }
    }
}

impl RunState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> ContractPhase {
        self.phase
    }

    pub fn active_encounter(&self) -> Option<&ActiveEncounter> {
        self.active_encounter.as_ref()
    }

    pub fn current_contract_node(&self) -> &EncounterNode {
        self.contract_map.current_encounter()
    }

    pub fn selectable_encounters(&self) -> Vec<&EncounterNode> {
        if self.phase != ContractPhase::Hub {
            return Vec::new();
        }
        self.contract_map.selectable_encounters()
    }

    pub fn encounter_option(&self, node_id: NodeId) -> Result<EncounterSelection, RunStateError> {
        self.require_phase("select encounter", ContractPhase::Hub)?;
        self.contract_map
            .encounter_option(node_id)
            .map_err(RunStateError::ContractMap)
    }

    pub fn select_encounter(
        &mut self,
        node_id: NodeId,
    ) -> Result<EncounterSelection, RunStateError> {
        self.require_phase("select encounter", ContractPhase::Hub)?;
        let selection = self
            .contract_map
            .select_encounter(node_id)
            .map_err(RunStateError::ContractMap)?;

        self.phase = ContractPhase::EncounterActive;
        self.active_encounter = Some(ActiveEncounter {
            id: selection.node_id.to_string(),
            encounter_type: selection.encounter_type,
        });
        Ok(selection)
    }

    pub fn start_encounter(
        &mut self,
        encounter_id: &str,
        encounter_type: &str,
    ) -> Result<(), RunStateError> {
        self.require_phase("start encounter", ContractPhase::Hub)?;

        let encounter_id = encounter_id.trim();
        if encounter_id.is_empty() {
            return Err(RunStateError::EmptyEncounterId);
        }

        let encounter_type = EncounterType::parse(encounter_type)?;
        self.phase = ContractPhase::EncounterActive;
        self.active_encounter = Some(ActiveEncounter {
            id: encounter_id.into(),
            encounter_type,
        });
        Ok(())
    }

    pub fn complete_encounter(&mut self) -> Result<(), RunStateError> {
        self.require_phase("complete encounter", ContractPhase::EncounterActive)?;
        if self.contract_map.encounter_in_progress() {
            self.contract_map
                .complete_current_encounter()
                .map_err(RunStateError::ContractMap)?;
        }
        self.phase = ContractPhase::Hub;
        self.active_encounter = None;
        Ok(())
    }

    pub fn report_caught(&mut self) -> Result<(), RunStateError> {
        self.require_phase("report caught", ContractPhase::EncounterActive)?;
        self.phase = ContractPhase::Caught;
        Ok(())
    }

    pub fn finish_caught(&mut self) -> Result<(), RunStateError> {
        self.require_phase("finish caught", ContractPhase::Caught)?;
        if self.contract_map.encounter_in_progress() {
            self.contract_map
                .complete_current_encounter()
                .map_err(RunStateError::ContractMap)?;
        }
        self.phase = ContractPhase::Hub;
        self.active_encounter = None;
        Ok(())
    }

    fn require_phase(
        &self,
        action: &'static str,
        required: ContractPhase,
    ) -> Result<(), RunStateError> {
        if self.phase == required {
            Ok(())
        } else {
            Err(RunStateError::InvalidTransition {
                action,
                phase: self.phase,
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStateError {
    InvalidTransition {
        action: &'static str,
        phase: ContractPhase,
    },
    EmptyEncounterId,
    UnknownEncounterType(String),
    ContractMap(ContractMapError),
}

impl fmt::Display for RunStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { action, phase } => {
                write!(
                    formatter,
                    "cannot {action} while phase is {}",
                    phase.as_str()
                )
            }
            Self::EmptyEncounterId => formatter.write_str("encounter id must not be empty"),
            Self::UnknownEncounterType(encounter_type) => {
                write!(formatter, "unknown encounter type: {encounter_type}")
            }
            Self::ContractMap(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_combat() -> RunState {
        let mut state = RunState::new();
        state.start_encounter("combat-demo-1", "combat").unwrap();
        state
    }

    #[test]
    fn initial_state_is_hub_without_an_encounter() {
        let state = RunState::new();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
    }

    #[test]
    fn starting_combat_records_identity_and_type() {
        let mut state = RunState::new();

        state.start_encounter("combat-demo-1", "combat").unwrap();

        assert_eq!(state.phase(), ContractPhase::EncounterActive);
        let encounter = state.active_encounter().unwrap();
        assert_eq!(encounter.id(), "combat-demo-1");
        assert_eq!(encounter.encounter_type(), EncounterType::Combat);
    }

    #[test]
    fn completed_encounter_exposes_every_reachable_choice_and_type() {
        let mut state = RunState::new();
        state.contract_map = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1]),
                EncounterNode::new(1, EncounterType::Combat, vec![2, 3]),
                EncounterNode::new(2, EncounterType::Event, vec![]),
                EncounterNode::new(3, EncounterType::Shop, vec![]),
            ],
            0,
        )
        .unwrap();
        state.select_encounter(1).unwrap();
        state.complete_encounter().unwrap();

        let choices: Vec<_> = state
            .selectable_encounters()
            .into_iter()
            .map(|node| (node.id(), node.encounter_type()))
            .collect();

        assert_eq!(
            choices,
            vec![(2, EncounterType::Event), (3, EncounterType::Shop)]
        );
    }

    #[test]
    fn choosing_a_route_loads_it_and_permanently_locks_its_siblings() {
        let mut state = RunState::new();

        let selected = state.select_encounter(1).unwrap();

        assert_eq!(selected.node_id, 1);
        assert_eq!(state.phase(), ContractPhase::EncounterActive);
        assert_eq!(state.active_encounter().unwrap().id(), "1");

        state.complete_encounter().unwrap();
        let before = state.clone();
        assert!(matches!(
            state.select_encounter(2),
            Err(RunStateError::ContractMap(
                ContractMapError::EncounterNotReachable { .. }
            ))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn completing_encounter_returns_to_hub_and_clears_it() {
        let mut state = active_combat();

        state.complete_encounter().unwrap();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
    }

    #[test]
    fn reporting_caught_retains_the_active_encounter() {
        let mut state = active_combat();

        state.report_caught().unwrap();

        assert_eq!(state.phase(), ContractPhase::Caught);
        assert_eq!(state.active_encounter().unwrap().id(), "combat-demo-1");
    }

    #[test]
    fn finishing_caught_returns_to_hub_and_clears_the_encounter() {
        let mut state = active_combat();
        state.report_caught().unwrap();

        state.finish_caught().unwrap();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
    }

    #[test]
    fn starting_a_second_encounter_is_rejected_without_mutation() {
        let mut state = active_combat();
        let before = state.clone();

        let result = state.start_encounter("event-2", "event");

        assert!(matches!(
            result,
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn completing_or_reporting_caught_from_hub_is_rejected() {
        let mut complete_state = RunState::new();
        let complete_before = complete_state.clone();
        let mut caught_state = RunState::new();
        let caught_before = caught_state.clone();

        assert!(matches!(
            complete_state.complete_encounter(),
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert!(matches!(
            caught_state.report_caught(),
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert_eq!(complete_state, complete_before);
        assert_eq!(caught_state, caught_before);
    }

    #[test]
    fn finishing_caught_outside_caught_is_rejected_without_mutation() {
        let mut state = active_combat();
        let before = state.clone();

        let result = state.finish_caught();

        assert!(matches!(
            result,
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn invalid_encounter_identity_and_type_are_rejected_without_mutation() {
        let mut empty_id_state = RunState::new();
        let empty_id_before = empty_id_state.clone();
        let mut unknown_type_state = RunState::new();
        let unknown_type_before = unknown_type_state.clone();

        assert_eq!(
            empty_id_state.start_encounter("   ", "combat"),
            Err(RunStateError::EmptyEncounterId)
        );
        assert_eq!(
            unknown_type_state.start_encounter("encounter-1", "mystery"),
            Err(RunStateError::UnknownEncounterType("mystery".into()))
        );
        assert_eq!(empty_id_state, empty_id_before);
        assert_eq!(unknown_type_state, unknown_type_before);
    }
}

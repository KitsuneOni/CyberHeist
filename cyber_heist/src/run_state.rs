//! Contract lifecycle state and transitions.
//!
//! This module is plain Rust: it owns the run invariants without depending on
//! Godot, while `run_state_node.rs` adapts it for scenes.

use std::fmt;

use rand::Rng;

use crate::contract_map::{
    ContractMap, ContractMapError, ContractProgress, ContractShape, EncounterNode,
    EncounterSelection, EncounterType, NodeId, NodeProgress, ZoneProgress,
};
use crate::contract_offer::{ContractOffer, generate_offers};

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
    zone: usize,
}

impl ActiveEncounter {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn encounter_type(&self) -> EncounterType {
        self.encounter_type
    }

    /// Which zone this encounter sits in, counting from 0.
    pub fn zone(&self) -> usize {
        self.zone
    }

    /// Whether this is the boss closing its zone, rather than one of the
    /// encounters on the way to it.
    pub fn is_boss(&self) -> bool {
        self.encounter_type.is_boss()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunState {
    phase: ContractPhase,
    active_encounter: Option<ActiveEncounter>,
    contract_map: ContractMap,
    /// The contracts this run can choose between. Fixed for the whole run, so
    /// an index into it stays meaningful. Empty for an authored run.
    contract_offers: Vec<ContractOffer>,
    /// Which of `contract_offers` the run took on. `None` means it is still
    /// on the contract it was created with, which pays no contract reward.
    accepted_offer_index: Option<usize>,
    /// Whether an encounter on the current contract has been entered. From
    /// then on the run is committed to it and cannot swap to another offer,
    /// whether that encounter was completed or the player was caught.
    contract_started: bool,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            phase: ContractPhase::Hub,
            active_encounter: None,
            contract_map: ContractMap::demo(),
            contract_offers: Vec::new(),
            accepted_offer_index: None,
            contract_started: false,
        }
    }
}

impl RunState {
    /// A run on the authored contract. The game generates its contracts, so
    /// this is what the tests use to work against a known graph.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// A run on a freshly generated contract, so no two runs follow the same
    /// route, with a fresh set of contract offers to choose between. `new`
    /// keeps the authored contract, which is what the tests run against.
    ///
    /// The run starts on a default-shaped contract that is playable straight
    /// away, so the hub works before any offer is accepted. Accepting an offer
    /// replaces that contract with the one the offer describes.
    pub fn generated(rng: &mut impl Rng) -> Self {
        // The map is rolled before the offers, so a given rng still lays out
        // the same default contract it did before offers existed.
        let contract_map = ContractMap::generate(ContractShape::default(), rng);
        let contract_offers = generate_offers(rng);

        Self {
            phase: ContractPhase::Hub,
            active_encounter: None,
            contract_map,
            contract_offers,
            accepted_offer_index: None,
            contract_started: false,
        }
    }

    pub fn phase(&self) -> ContractPhase {
        self.phase
    }

    pub fn active_encounter(&self) -> Option<&ActiveEncounter> {
        self.active_encounter.as_ref()
    }

    /// The contracts this run can choose between, shortest first.
    pub fn contract_offers(&self) -> &[ContractOffer] {
        &self.contract_offers
    }

    /// Position in `contract_offers` of the contract the run took on, if any.
    pub fn accepted_offer_index(&self) -> Option<usize> {
        self.accepted_offer_index
    }

    /// The contract the run took on, if any. Its `credit_reward` is what
    /// finishing the contract pays.
    pub fn accepted_offer(&self) -> Option<&ContractOffer> {
        self.accepted_offer_index
            .and_then(|index| self.contract_offers.get(index))
    }

    /// Takes on the offer at `index`, replacing the contract map with the one
    /// it describes.
    ///
    /// Allowed only from the hub and only before any encounter on the current
    /// contract has been entered; until then the player may change their mind
    /// and accept a different offer. A rejection leaves the run untouched.
    pub fn accept_contract_offer(&mut self, index: usize) -> Result<(), RunStateError> {
        self.require_phase("accept a contract", ContractPhase::Hub)?;
        if self.contract_started {
            return Err(RunStateError::ContractAlreadyStarted);
        }
        let Some(offer) = self.contract_offers.get(index) else {
            return Err(RunStateError::UnknownContractOffer {
                index,
                offered: self.contract_offers.len(),
            });
        };

        self.contract_map = offer.build_map();
        self.accepted_offer_index = Some(index);
        Ok(())
    }

    pub fn current_contract_node(&self) -> &EncounterNode {
        self.contract_map.current_encounter()
    }

    /// Every node with its status, for drawing the contract map.
    pub fn node_progress(&self) -> Vec<NodeProgress> {
        self.contract_map.node_progress()
    }

    /// Completed encounters out of the total on this contract.
    pub fn progress(&self) -> ContractProgress {
        self.contract_map.progress()
    }

    /// Which zone the player is in, how many are open, and how many there are.
    pub fn zone_progress(&self) -> ZoneProgress {
        self.contract_map.zone_progress()
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
        self.contract_started = true;
        self.active_encounter = Some(ActiveEncounter {
            id: selection.node_id.to_string(),
            encounter_type: selection.encounter_type,
            zone: selection.zone,
        });
        Ok(selection)
    }

    pub fn complete_encounter(&mut self) -> Result<(), RunStateError> {
        self.require_phase("complete encounter", ContractPhase::EncounterActive)?;
        self.contract_map
            .complete_current_encounter()
            .map_err(RunStateError::ContractMap)?;
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
        self.contract_map
            .fail_current_encounter()
            .map_err(RunStateError::ContractMap)?;
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
    ContractMap(ContractMapError),
    /// No contract is on offer at the requested position.
    UnknownContractOffer {
        index: usize,
        offered: usize,
    },
    /// An encounter on the current contract has already been entered, so the
    /// run is committed to it.
    ContractAlreadyStarted,
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
            Self::ContractMap(error) => fmt::Display::fmt(error, formatter),
            Self::UnknownContractOffer { index, offered } => write!(
                formatter,
                "contract offer {index} does not exist; {offered} contracts are on offer"
            ),
            Self::ContractAlreadyStarted => formatter.write_str(
                "this contract is already under way, so a different one cannot be accepted",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;

    fn active_combat() -> RunState {
        let mut state = RunState::new();
        state.select_encounter(1).unwrap();
        state
    }

    #[test]
    fn initial_state_is_hub_without_an_encounter() {
        let state = RunState::new();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
    }

    #[test]
    fn selecting_combat_records_identity_and_type() {
        let mut state = RunState::new();

        state.select_encounter(1).unwrap();

        assert_eq!(state.phase(), ContractPhase::EncounterActive);
        let encounter = state.active_encounter().unwrap();
        assert_eq!(encounter.id(), "1");
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
        assert_eq!(state.active_encounter().unwrap().id(), "1");
    }

    #[test]
    fn finishing_caught_returns_to_hub_and_clears_the_encounter() {
        let mut state = active_combat();
        state.report_caught().unwrap();

        state.finish_caught().unwrap();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
        let selectable: Vec<_> = state
            .selectable_encounters()
            .into_iter()
            .map(|node| node.id())
            .collect();
        assert_eq!(selectable, vec![1]);
    }

    #[test]
    fn selecting_a_second_encounter_is_rejected_without_mutation() {
        let mut state = active_combat();
        let before = state.clone();

        let result = state.select_encounter(2);

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

    fn generated_run(seed: u64) -> RunState {
        RunState::generated(&mut StdRng::seed_from_u64(seed))
    }

    /// Steps into whichever encounter the hub offers first.
    fn enter_first_encounter(state: &mut RunState) {
        let first = state.selectable_encounters()[0].id();
        state
            .select_encounter(first)
            .expect("the hub offers its first encounter");
    }

    /// Acceptance scenario 1: starting a run puts three contracts on the
    /// table, each with a name, a target, a reward and a number of zones.
    #[test]
    fn starting_a_new_run_offers_three_contracts() {
        let state = generated_run(5);

        let offers = state.contract_offers();
        assert_eq!(offers.len(), 3);
        for offer in offers {
            assert!(!offer.name.is_empty());
            assert!(!offer.target.is_empty());
            assert!(offer.credit_reward > 0);
            assert!(offer.shape.zone_count >= 1);
        }
        assert_eq!(
            state.accepted_offer(),
            None,
            "nothing is accepted until the player chooses"
        );
    }

    /// Acceptance scenario 2: accepting the second offer swaps the run onto
    /// the contract that offer describes.
    #[test]
    fn accepting_the_second_offer_builds_the_map_from_its_settings() {
        let mut state = generated_run(5);
        let second = state.contract_offers()[1].clone();

        state
            .accept_contract_offer(1)
            .expect("the second offer can be accepted");

        assert_eq!(state.contract_map, second.build_map());
        assert_eq!(state.zone_progress().total_zones, second.shape.zone_count);
        assert_eq!(state.progress().total, second.encounter_count());
        assert_eq!(state.accepted_offer_index(), Some(1));
        assert_eq!(state.accepted_offer(), Some(&second));
        assert_eq!(state.phase(), ContractPhase::Hub);
        assert!(
            !state.selectable_encounters().is_empty(),
            "the accepted contract is ready to play"
        );
    }

    /// Until the selection screen exists, the hub has to keep working on the
    /// contract a run is created with.
    #[test]
    fn a_generated_run_is_playable_before_any_offer_is_accepted() {
        let state = generated_run(5);

        assert_eq!(
            state.zone_progress().total_zones,
            ContractShape::default().zone_count
        );
        assert!(!state.selectable_encounters().is_empty());
        assert_eq!(state.accepted_offer_index(), None);
    }

    /// Changing your mind is fine until the first encounter is entered.
    #[test]
    fn a_different_offer_can_be_accepted_before_the_contract_starts() {
        let mut state = generated_run(9);
        let third = state.contract_offers()[2].clone();

        state.accept_contract_offer(0).expect("first offer");
        state.accept_contract_offer(2).expect("third offer");

        assert_eq!(state.contract_map, third.build_map());
        assert_eq!(state.accepted_offer(), Some(&third));
    }

    #[test]
    fn an_offer_index_past_the_end_is_rejected_without_mutation() {
        let mut state = generated_run(5);
        let before = state.clone();

        for index in [3, usize::MAX] {
            assert_eq!(
                state.accept_contract_offer(index),
                Err(RunStateError::UnknownContractOffer { index, offered: 3 })
            );
            assert_eq!(state, before, "a rejected index {index} changed the run");
        }
    }

    /// The authored run the tests use is not offered any contracts, and says
    /// so rather than accepting something that is not there.
    #[test]
    fn an_authored_run_has_no_offers_to_accept() {
        let mut state = RunState::new();
        let before = state.clone();

        assert!(state.contract_offers().is_empty());
        assert_eq!(
            state.accept_contract_offer(0),
            Err(RunStateError::UnknownContractOffer {
                index: 0,
                offered: 0
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn a_contract_cannot_be_accepted_during_an_encounter() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        let before = state.clone();

        assert!(matches!(
            state.accept_contract_offer(0),
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert_eq!(state, before);
    }

    /// Back at the hub after an encounter, the run is committed: the
    /// contract it accepted stays, and so does the record of which one it was.
    #[test]
    fn a_contract_cannot_be_swapped_once_an_encounter_is_completed() {
        let mut state = generated_run(5);
        state.accept_contract_offer(1).expect("second offer");
        enter_first_encounter(&mut state);
        state.complete_encounter().expect("the encounter is active");
        let before = state.clone();

        assert_eq!(
            state.accept_contract_offer(0),
            Err(RunStateError::ContractAlreadyStarted)
        );
        assert_eq!(state, before);
        assert_eq!(state.accepted_offer_index(), Some(1));
    }

    /// Getting caught does not complete anything, but the run has still
    /// committed to a route on this contract.
    #[test]
    fn a_contract_cannot_be_swapped_after_being_caught_on_it() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        state.report_caught().expect("the encounter is active");
        state.finish_caught().expect("the run was caught");
        let before = state.clone();

        assert_eq!(
            state.accept_contract_offer(2),
            Err(RunStateError::ContractAlreadyStarted)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn offer_rejections_explain_themselves() {
        let unknown = RunStateError::UnknownContractOffer {
            index: 4,
            offered: 3,
        }
        .to_string();
        assert!(unknown.contains('4') && unknown.contains('3'), "{unknown}");

        let started = RunStateError::ContractAlreadyStarted.to_string();
        assert!(started.contains("under way"), "{started}");
    }
}

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
use crate::run_stats::RunStats;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractPhase {
    Hub,
    EncounterActive,
    Caught,
    /// The final zone's boss is down and the contract is complete. Nothing
    /// is left to select; the run ends here and a new one can start.
    ContractWon,
}

impl ContractPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hub => "hub",
            Self::EncounterActive => "encounter_active",
            Self::Caught => "caught",
            Self::ContractWon => "contract_won",
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

    /// Whether this is an elite node, guarded by tougher security than the
    /// combat around it.
    pub fn is_elite(&self) -> bool {
        self.encounter_type == EncounterType::Elite
    }
}

/// Everything the end screen reports about a run, read in one go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub stats: RunStats,
    /// Encounters completed on the contract, out of the number on a route
    /// through it.
    pub encounters: ContractProgress,
    /// Zones whose boss has been beaten.
    pub zones_cleared: usize,
    pub total_zones: usize,
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
    /// What winning this contract paid, recorded by the one transition into
    /// `ContractWon`. `None` until the contract is won. Kept on the run, not
    /// just handed to the caller, so the victory screen can be rebuilt from
    /// run state alone.
    contract_reward: Option<i64>,
    /// What this run has achieved, for the end screen. Owned here so that
    /// it starts and ends with the run.
    stats: RunStats,
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
            contract_reward: None,
            stats: RunStats::default(),
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
            contract_reward: None,
            stats: RunStats::default(),
        }
    }

    /// Throws this run away and starts a fresh one in its place: a new
    /// default contract, new offers, nothing accepted, no zone progress and
    /// no stats. It is exactly what `generated` builds, so nothing from the
    /// old run can leak into the new one.
    ///
    /// Allowed from the hub, the caught screen and the victory screen, which
    /// is where a run ends. Refused while an encounter is active, because
    /// that encounter's screen would carry on playing against a run that no
    /// longer exists. A refusal leaves the run untouched.
    ///
    /// Only the run is reset here. The shared noise meter, knowledge and the
    /// run's deck live outside it, so `FlowCoordinator.start_new_run` resets
    /// those alongside this.
    pub fn start_new_run(&mut self, rng: &mut impl Rng) -> Result<(), RunStateError> {
        if self.phase == ContractPhase::EncounterActive {
            return Err(RunStateError::InvalidTransition {
                action: "start a new run",
                phase: self.phase,
            });
        }

        *self = Self::generated(rng);
        Ok(())
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

    /// Whether `accept_contract_offer` would currently take an offer: the run
    /// is at the hub, has offers to choose from and has not yet entered an
    /// encounter on its contract. That stays true after an offer is accepted,
    /// because the player can still change their mind until they enter an
    /// encounter; the hub reads it to offer Change contract on the map.
    pub fn can_accept_contract_offer(&self) -> bool {
        self.phase == ContractPhase::Hub
            && !self.contract_started
            && !self.contract_offers.is_empty()
    }

    /// Whether the hub should ask for a contract before showing the map: an
    /// offer could be accepted and none has been yet. False for an authored
    /// run, which has no offers, and once the contract is under way.
    pub fn needs_contract_choice(&self) -> bool {
        self.can_accept_contract_offer() && self.accepted_offer_index.is_none()
    }

    /// Whether an encounter on the current contract has been entered, whether
    /// it was then completed or the player was caught in it. This is what
    /// separates a contract nobody has touched from one whose first encounter
    /// failed: neither has completed anything, but only the first can still
    /// be swapped for another offer.
    pub fn contract_started(&self) -> bool {
        self.contract_started
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

    /// Completes the active encounter. A fight completed here is counted as
    /// a win; this is the only place that happens, so a win cannot be
    /// recorded twice or for a lost fight.
    ///
    /// Completing the final zone's boss wins the contract: it moves to
    /// `ContractWon` and records the contract reward (see `contract_reward`).
    /// Anything else, including an earlier zone's boss, which only opens the
    /// next zone, returns to the hub.
    pub fn complete_encounter(&mut self) -> Result<(), RunStateError> {
        self.require_phase("complete encounter", ContractPhase::EncounterActive)?;
        // Decided before completing, because completing clears the encounter.
        let wins_contract = self.active_encounter_wins_contract();
        self.contract_map
            .complete_current_encounter()
            .map_err(RunStateError::ContractMap)?;

        if let Some(completed) = self.active_encounter.take() {
            self.stats
                .record_encounter_completed(completed.encounter_type());
        }
        if wins_contract {
            self.phase = ContractPhase::ContractWon;
            self.contract_reward = Some(self.reward_for_winning());
        } else {
            self.phase = ContractPhase::Hub;
        }
        Ok(())
    }

    /// Whether completing the active encounter would win the contract: it is
    /// the boss closing the contract's final zone. False between encounters,
    /// and while caught, since a caught encounter is not completed.
    ///
    /// `FlowCoordinator` reads this before completing, so it can prepare the
    /// victory screen instead of the card reward.
    pub fn active_encounter_wins_contract(&self) -> bool {
        if self.phase != ContractPhase::EncounterActive {
            return false;
        }
        let Some(encounter) = &self.active_encounter else {
            return false;
        };

        let final_zone = self.zone_progress().total_zones.saturating_sub(1);
        encounter.is_boss() && encounter.zone() >= final_zone
    }

    /// What winning this contract paid: `Some(credits)` once it has been won,
    /// `None` before then and on a contract the player was caught on.
    ///
    /// It is recorded by the transition into `ContractWon`, which can happen
    /// only once per contract because that phase is terminal, so the reward
    /// cannot come due twice. Run state only records the amount: the caller
    /// that completed the encounter applies it to the player's balance and
    /// records it on the run, which `FlowCoordinator` does through
    /// `change_credits`.
    pub fn contract_reward(&self) -> Option<i64> {
        self.contract_reward
    }

    /// The accepted offer's `credit_reward`, or 0 for the default contract a
    /// run starts on, which was never offered and pays no reward.
    fn reward_for_winning(&self) -> i64 {
        self.accepted_offer()
            .map(|offer| offer.credit_reward)
            .unwrap_or(0)
    }

    /// Records a change the run made to the player's credits, gain or loss.
    /// Only gains count as earned. Call this wherever credits are applied
    /// (events, combat payouts and the contract payout), once per change.
    pub fn record_credit_change(&mut self, amount: i64) {
        self.stats.record_credit_change(amount);
    }

    /// Records a card going into the run's deck. Call this wherever a card
    /// is added, once per card.
    pub fn record_card_added(&mut self) {
        self.stats.record_card_added();
    }

    /// The run's stats together with how far through the contract it got,
    /// for the end screen.
    pub fn summary(&self) -> RunSummary {
        let zones = self.zone_progress();
        RunSummary {
            stats: self.stats,
            encounters: self.progress(),
            zones_cleared: zones.unlocked_zones,
            total_zones: zones.total_zones,
        }
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

    /// A run is created on a playable default contract, so anything driving
    /// it without the hub's offer picker (the headless tests) still works.
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

    /// Being caught in the first encounter completes nothing, so progress
    /// alone cannot tell that contract apart from an untouched one.
    #[test]
    fn a_contract_counts_as_started_from_its_first_encounter_even_when_caught() {
        let mut state = generated_run(5);
        assert!(!state.contract_started(), "a new run has entered nothing");

        enter_first_encounter(&mut state);
        assert!(state.contract_started(), "entering an encounter starts it");

        state.report_caught().expect("the encounter is active");
        state.finish_caught().expect("the run was caught");
        assert_eq!(state.progress().completed, 0);
        assert!(
            state.contract_started(),
            "being caught does not undo the start"
        );

        state
            .start_new_run(&mut StdRng::seed_from_u64(3))
            .expect("a new run can start from the hub");
        assert!(!state.contract_started(), "a new run starts afresh");
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

    /// The hub asks this to offer Change contract, so it has to agree with
    /// what `accept_contract_offer` actually allows.
    #[test]
    fn an_offer_can_be_accepted_only_before_the_contract_starts() {
        let mut state = generated_run(5);
        assert!(state.can_accept_contract_offer(), "a new run can choose");

        state.accept_contract_offer(1).expect("second offer");
        assert!(
            state.can_accept_contract_offer(),
            "the player can still change their mind before entering anything"
        );
        state
            .accept_contract_offer(0)
            .expect("so changing to another offer is accepted");
        assert_eq!(state.accepted_offer_index(), Some(0));

        enter_first_encounter(&mut state);
        assert!(
            !state.can_accept_contract_offer(),
            "not during an encounter"
        );

        state.complete_encounter().expect("the encounter is active");
        assert!(
            !state.can_accept_contract_offer(),
            "not once the contract is under way"
        );
    }

    #[test]
    fn an_offer_cannot_be_accepted_after_being_caught_on_the_contract() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        state.report_caught().expect("the encounter is active");
        state.finish_caught().expect("the run was caught");

        assert!(!state.can_accept_contract_offer());
    }

    #[test]
    fn an_authored_run_has_no_offer_choice_to_make() {
        let state = RunState::new();

        assert!(!state.can_accept_contract_offer());
        assert!(!state.needs_contract_choice());
    }

    /// The hub opens on the offer picker exactly while this is true, so a
    /// new run asks once and then shows the map.
    #[test]
    fn a_contract_choice_is_needed_only_until_an_offer_is_accepted() {
        let mut state = generated_run(5);
        assert!(
            state.needs_contract_choice(),
            "a new run asks for a contract"
        );

        state.accept_contract_offer(2).expect("third offer");
        assert!(
            !state.needs_contract_choice(),
            "an accepted offer goes straight to the map"
        );
        assert!(
            state.can_accept_contract_offer(),
            "while Change contract stays available"
        );
    }

    #[test]
    fn no_contract_choice_is_needed_once_the_default_contract_is_under_way() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        assert!(!state.needs_contract_choice(), "not during an encounter");

        state.complete_encounter().expect("the encounter is active");
        assert!(
            !state.needs_contract_choice(),
            "the run is committed to the contract it started on"
        );
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

    // --- Run stats and new runs (story #121) -------------------------------

    /// Enters and completes each node in turn, as a player walking the route.
    fn play_route(state: &mut RunState, route: &[NodeId]) {
        for node_id in route {
            state
                .select_encounter(*node_id)
                .expect("the route is selectable");
            state.complete_encounter().expect("the encounter is active");
        }
    }

    /// One straight route of four fights with an event in the middle, closed
    /// off by a boss: entry, combat, event, elite, combat, boss.
    fn four_fight_run() -> RunState {
        let mut state = RunState::new();
        state.contract_map = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1]),
                EncounterNode::new(1, EncounterType::Combat, vec![2]),
                EncounterNode::new(2, EncounterType::Event, vec![3]),
                EncounterNode::new(3, EncounterType::Elite, vec![4]),
                EncounterNode::new(4, EncounterType::Combat, vec![5]),
                EncounterNode::new(5, EncounterType::Boss, vec![]),
            ],
            0,
        )
        .unwrap();
        state
    }

    /// Walks a generated contract until its first boss goes down, so the run
    /// has zone progress to lose.
    fn clear_first_zone(state: &mut RunState) {
        // Bounded by the route length so a contract without a boss fails the
        // test rather than hanging it.
        for _ in 0..state.progress().total {
            enter_first_encounter(state);
            let was_boss = state.active_encounter().unwrap().is_boss();
            state.complete_encounter().expect("the encounter is active");
            if was_boss {
                return;
            }
        }
        panic!("the contract has no boss to clear");
    }

    /// Acceptance scenario 1: four fights won, 180 credits earned and three
    /// cards added are what the run summary reports at the end.
    #[test]
    fn stats_are_tracked_during_a_run() {
        let mut state = four_fight_run();

        play_route(&mut state, &[1, 2]);
        state.record_credit_change(120);
        state.record_card_added();
        // The event in the middle cost credits; that is not money earned.
        state.record_credit_change(-80);
        play_route(&mut state, &[3, 4]);
        state.record_credit_change(60);
        state.record_card_added();
        state.record_card_added();
        play_route(&mut state, &[5]);

        let summary = state.summary();
        assert_eq!(summary.stats.combats_won(), 4);
        assert_eq!(summary.stats.credits_earned(), 180);
        assert_eq!(summary.stats.cards_added(), 3);
        assert_eq!(
            summary.encounters,
            ContractProgress {
                completed: 5,
                total: 5
            }
        );
        assert_eq!(summary.zones_cleared, 1);
        assert_eq!(summary.total_zones, 1);
    }

    #[test]
    fn a_fresh_run_summary_is_empty() {
        let summary = generated_run(5).summary();

        assert_eq!(summary.stats, RunStats::default());
        assert_eq!(summary.encounters.completed, 0);
        assert_eq!(summary.zones_cleared, 0);
        assert_eq!(summary.total_zones, ContractShape::default().zone_count);
    }

    /// The authored contract has a shop route and an event route; only the
    /// fights along each are wins.
    #[test]
    fn events_and_shops_are_not_counted_as_wins() {
        let mut shop_route = RunState::new();
        play_route(&mut shop_route, &[1, 3, 5]);
        assert_eq!(shop_route.summary().stats.combats_won(), 2);

        let mut event_route = RunState::new();
        play_route(&mut event_route, &[2, 4, 5]);
        assert_eq!(event_route.summary().stats.combats_won(), 2);
    }

    /// Getting caught is losing the fight, so it is not a win; beating the
    /// same encounter on the retry is.
    #[test]
    fn a_caught_encounter_is_not_counted_as_a_win() {
        let mut state = active_combat();

        state.report_caught().unwrap();
        state.finish_caught().unwrap();
        assert_eq!(state.summary().stats.combats_won(), 0);

        play_route(&mut state, &[1]);
        assert_eq!(state.summary().stats.combats_won(), 1);
    }

    #[test]
    fn a_rejected_completion_is_not_counted_as_a_win() {
        let mut state = RunState::new();

        assert!(state.complete_encounter().is_err());

        assert_eq!(state.summary().stats.combats_won(), 0);
    }

    /// Credits and cards can come in at any point of the run, not only from
    /// the hub: an event pays out while its encounter is still active.
    #[test]
    fn credits_and_cards_are_recorded_in_any_phase() {
        let mut state = active_combat();

        state.record_credit_change(40);
        state.record_card_added();
        state.report_caught().unwrap();
        state.record_credit_change(10);

        let stats = state.summary().stats;
        assert_eq!(stats.credits_earned(), 50);
        assert_eq!(stats.cards_added(), 1);
    }

    /// Acceptance scenario 2: after a run with progress, stats, an accepted
    /// contract and a cleared zone, a new run starts from nothing.
    #[test]
    fn starting_a_new_run_resets_stats_zone_progress_and_offers() {
        let mut state = generated_run(5);
        let old_offers = state.contract_offers().to_vec();
        state.accept_contract_offer(1).expect("second offer");
        clear_first_zone(&mut state);
        state.record_credit_change(90);
        state.record_card_added();
        assert_eq!(
            state.zone_progress().unlocked_zones,
            1,
            "the old run cleared a zone"
        );

        state
            .start_new_run(&mut StdRng::seed_from_u64(77))
            .expect("a new run can start from the hub");

        assert_eq!(
            state,
            RunState::generated(&mut StdRng::seed_from_u64(77)),
            "a new run is exactly a freshly generated one"
        );
        assert_eq!(state.summary().stats, RunStats::default());
        assert_eq!(state.zone_progress().current_zone, 0);
        assert_eq!(state.zone_progress().unlocked_zones, 0);
        assert_eq!(state.progress().completed, 0);
        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
        assert_ne!(
            state.contract_offers(),
            old_offers.as_slice(),
            "a new run is offered new contracts"
        );
        assert_eq!(state.accepted_offer(), None);
        state
            .accept_contract_offer(0)
            .expect("the new run has not started a contract yet");
    }

    /// The run ends on the caught screen as often as on the map, so the end
    /// screen can start a new run from there.
    #[test]
    fn a_new_run_can_start_after_being_caught() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        state.report_caught().unwrap();

        state
            .start_new_run(&mut StdRng::seed_from_u64(3))
            .expect("a new run can start from the caught screen");

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
    }

    /// Abandoning a run part-way through a fight would leave its screen
    /// playing against a run that no longer exists, so it is refused.
    #[test]
    fn a_new_run_cannot_start_during_an_encounter() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        state.record_credit_change(25);
        let before = state.clone();

        let result = state.start_new_run(&mut StdRng::seed_from_u64(3));

        assert!(matches!(
            result,
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert!(
            result.unwrap_err().to_string().contains("start a new run"),
            "the rejection says what was refused"
        );
        assert_eq!(state, before);
    }

    // --- Winning the contract (stories #89 and #23) ------------------------

    /// A run on the authored two-zone contract: node 3 is zone 1's boss and
    /// node 6, the target, is the final zone's boss.
    fn zoned_run() -> RunState {
        let mut state = RunState::new();
        state.contract_map = ContractMap::demo_zoned();
        state
    }

    /// Walks a run through whatever the hub offers first until the contract
    /// is won, and returns how many encounters that took.
    fn play_to_victory(state: &mut RunState) -> usize {
        // Bounded by the route length so a contract that cannot be won fails
        // the test rather than hanging it.
        let route_length = state.progress().total;
        for played in 1..=route_length {
            enter_first_encounter(state);
            state.complete_encounter().expect("the encounter is active");
            if state.phase() == ContractPhase::ContractWon {
                return played;
            }
        }
        panic!("the contract was not won after {route_length} encounters");
    }

    /// Acceptance scenario 1 (#89): beating the last zone's boss completes
    /// the contract instead of returning to the map.
    #[test]
    fn defeating_the_final_zones_boss_wins_the_contract() {
        let mut state = zoned_run();
        play_route(&mut state, &[1, 3, 4]);
        state.select_encounter(6).unwrap();

        state.complete_encounter().unwrap();

        assert_eq!(state.phase(), ContractPhase::ContractWon);
        assert_eq!(state.phase().as_str(), "contract_won");
        assert_eq!(state.active_encounter(), None);
        assert!(
            state.selectable_encounters().is_empty(),
            "a won contract offers nothing more to play"
        );
        let summary = state.summary();
        assert_eq!(summary.zones_cleared, summary.total_zones);
        assert_eq!(summary.stats.combats_won(), 4);
    }

    /// Acceptance scenario 3 (#89): an earlier zone's boss only opens the
    /// next zone and sends the player back to the map.
    #[test]
    fn defeating_an_earlier_boss_unlocks_the_next_zone_and_returns_to_the_hub() {
        let mut state = zoned_run();
        play_route(&mut state, &[1]);
        state.select_encounter(3).unwrap();

        state.complete_encounter().unwrap();

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.zone_progress().unlocked_zones, 1);
        let selectable: Vec<_> = state
            .selectable_encounters()
            .into_iter()
            .map(|node| node.id())
            .collect();
        assert_eq!(selectable, vec![4, 5], "the next zone is open");
        assert_eq!(
            state.contract_reward(),
            None,
            "clearing a zone is not finishing the contract"
        );
    }

    #[test]
    fn only_the_final_zones_boss_is_reported_as_winning_the_contract() {
        let mut state = zoned_run();
        assert!(!state.active_encounter_wins_contract(), "nothing is active");

        state.select_encounter(1).unwrap();
        assert!(!state.active_encounter_wins_contract(), "an ordinary fight");
        state.complete_encounter().unwrap();

        state.select_encounter(3).unwrap();
        assert!(!state.active_encounter_wins_contract(), "zone 1's boss");
        state.complete_encounter().unwrap();

        play_route(&mut state, &[5]);
        state.select_encounter(6).unwrap();
        assert!(state.active_encounter_wins_contract(), "the final boss");

        state.report_caught().unwrap();
        assert!(
            !state.active_encounter_wins_contract(),
            "a caught encounter is not completed, so it wins nothing"
        );
    }

    /// The authored single-zone contract with a boss as its target is won by
    /// that boss, since its only zone is also its last.
    #[test]
    fn a_single_zone_contract_is_won_by_its_only_boss() {
        let mut state = four_fight_run();

        play_route(&mut state, &[1, 2, 3, 4, 5]);

        assert_eq!(state.phase(), ContractPhase::ContractWon);
    }

    /// A contract without a boss has no final boss to beat, so finishing its
    /// route is not a win; it stays on the hub as it always has.
    #[test]
    fn a_contract_without_a_boss_is_never_won() {
        let mut state = RunState::new();

        play_route(&mut state, &[1, 3, 5]);

        assert_eq!(state.phase(), ContractPhase::Hub);
    }

    #[test]
    fn nothing_can_be_entered_or_reported_after_the_contract_is_won() {
        let mut state = generated_run(5);
        play_to_victory(&mut state);
        let before = state.clone();

        assert!(matches!(
            state.select_encounter(1),
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert!(matches!(
            state.encounter_option(1),
            Err(RunStateError::InvalidTransition { .. })
        ));
        assert!(state.complete_encounter().is_err());
        assert!(state.report_caught().is_err());
        assert!(state.finish_caught().is_err());
        assert!(state.accept_contract_offer(0).is_err());
        assert_eq!(state, before);
    }

    /// Acceptance (#23 and #89 scenario 2): winning records the accepted
    /// contract's difficulty-derived reward, on the transition itself.
    #[test]
    fn winning_records_the_accepted_offers_credit_reward() {
        let mut state = generated_run(5);
        state.accept_contract_offer(2).expect("third offer");
        let reward = state.accepted_offer().unwrap().credit_reward;

        play_to_victory(&mut state);

        assert_eq!(state.contract_reward(), Some(reward));
    }

    /// The reward is recorded by the transition into `ContractWon`, and that
    /// phase is terminal, so nothing can make it come due a second time.
    #[test]
    fn the_contract_reward_can_only_come_due_once() {
        let mut state = generated_run(5);
        state.accept_contract_offer(0).expect("first offer");
        play_to_victory(&mut state);
        let won = state.clone();

        assert!(state.complete_encounter().is_err());
        assert!(state.select_encounter(1).is_err());
        assert_eq!(state, won, "a refused transition changes nothing");
    }

    #[test]
    fn no_contract_reward_is_recorded_before_the_contract_is_won() {
        let mut state = generated_run(5);
        state.accept_contract_offer(1).expect("second offer");
        assert_eq!(state.contract_reward(), None, "at the hub");

        enter_first_encounter(&mut state);
        assert_eq!(state.contract_reward(), None, "during an encounter");

        state.complete_encounter().expect("the encounter is active");
        assert_eq!(state.contract_reward(), None, "after an ordinary fight");
    }

    /// Each offer pays its own reward, so a longer, harder contract pays
    /// more on winning than a shorter one.
    #[test]
    fn a_harder_contract_pays_a_bigger_reward_on_winning() {
        let mut rewards = Vec::new();
        for index in 0..3 {
            let mut state = generated_run(11);
            state.accept_contract_offer(index).expect("offer");
            play_to_victory(&mut state);
            rewards.push(state.contract_reward().expect("won"));
        }

        assert!(
            rewards.windows(2).all(|pair| pair[0] < pair[1]),
            "offers are listed shortest first, so rewards rise: {rewards:?}"
        );
    }

    /// The default contract a run starts on was never offered, so it has no
    /// reward to pay. Winning it still ends the contract.
    #[test]
    fn winning_the_default_contract_pays_no_reward() {
        let mut state = generated_run(5);
        play_to_victory(&mut state);

        assert_eq!(state.phase(), ContractPhase::ContractWon);
        assert_eq!(state.contract_reward(), Some(0));
    }

    /// Getting caught fails the encounter rather than the reward ever coming
    /// due, so nothing is recorded while caught or after it.
    #[test]
    fn a_caught_contract_pays_no_contract_reward() {
        let mut state = generated_run(5);
        state.accept_contract_offer(0).expect("first offer");
        enter_first_encounter(&mut state);

        state.report_caught().unwrap();
        assert_eq!(state.contract_reward(), None, "while caught");

        state.finish_caught().unwrap();
        assert_eq!(state.contract_reward(), None, "after being caught");
    }

    /// Acceptance scenario 2 (#89): from the victory screen the player takes
    /// another contract, on a run that starts clean.
    #[test]
    fn a_new_run_can_start_after_winning_the_contract() {
        let mut state = generated_run(5);
        state.accept_contract_offer(1).expect("second offer");
        play_to_victory(&mut state);
        assert!(state.contract_reward().is_some(), "the contract was won");
        state.record_credit_change(500);

        state
            .start_new_run(&mut StdRng::seed_from_u64(21))
            .expect("a new run can start from the victory screen");

        assert_eq!(
            state,
            RunState::generated(&mut StdRng::seed_from_u64(21)),
            "a new run is exactly a freshly generated one"
        );
        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.summary().stats, RunStats::default());
        assert_eq!(
            state.contract_reward(),
            None,
            "the new contract has not been won yet"
        );
        assert!(
            state.needs_contract_choice(),
            "the new run asks for a contract"
        );
        state
            .accept_contract_offer(0)
            .expect("the new run can take another contract");
    }
}

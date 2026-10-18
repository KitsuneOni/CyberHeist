//! Contract lifecycle state and transitions.
//!
//! This module is plain Rust: it owns the run invariants without depending on
//! Godot, while `run_state_node.rs` adapts it for scenes.

use std::fmt;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::card_data::Rarity;
use crate::contract_map::{
    ContractMap, ContractMapError, ContractProgress, ContractShape, EncounterNode,
    EncounterSelection, EncounterType, NodeId, NodeProgress, ZoneProgress,
};
use crate::contract_offer::{ContractOffer, generate_offers};
use crate::events::{EventDefinition, event_catalogue, random_event};
use crate::noise_meter::NoiseLevel;
use crate::run_deck::RunDeck;
use crate::run_stats::RunStats;
use crate::shop::{
    self, Purchase, RemovedCard, Reroll, ScrubbedLogs, ShopError, ShopVisit, Upgrade,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Serialised whole as part of a saved run (see `save_game.rs`). A run read
/// back from a save must pass `validate_restored` before it is used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Ids of the events this run has already shown, so an event node does
    /// not repeat one. Owned here so a new run starts with a fresh pool.
    ///
    /// Defaults to none when missing, so a save made before events were
    /// tracked still loads. An id can appear twice: once every event has been
    /// seen, `random_event` draws from the whole catalogue again.
    #[serde(default)]
    seen_events: Vec<String>,
    /// The stock of the shop the run is in. Rolled the first time the shop
    /// screen opens and kept until the shop encounter ends, so reopening the
    /// screen during the same visit cannot reroll it. Always `None` outside a
    /// shop encounter.
    shop_visit: Option<ShopVisit>,
    /// Whether the player is browsing the shop from the dark web screen
    /// rather than from a shop encounter. It happens at the hub phase, and
    /// the visit's stock is kept in `shop_visit` just as for an encounter.
    /// Defaults to false so saves from before it existed still load.
    #[serde(default)]
    dark_web_visit: bool,
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
            seen_events: Vec::new(),
            shop_visit: None,
            dark_web_visit: false,
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
            seen_events: Vec::new(),
            shop_visit: None,
            dark_web_visit: false,
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

    /// Replaces this run with a fresh one whatever it is doing, even part
    /// way through an encounter, for quitting to the main menu. The run is
    /// not lost: the save file keeps it as of its last safe point for
    /// Continue. This only stops New contract picking the old run up from
    /// behind the menu.
    pub fn abandon(&mut self, rng: &mut impl Rng) {
        *self = Self::generated(rng);
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
        self.shop_visit = None;
        self.dark_web_visit = false;
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
        self.shop_visit = None;
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

    /// Rolls the event for an event node, avoiding any this run has already
    /// shown, and remembers it as seen.
    pub fn roll_event(&mut self, rng: &mut impl Rng) -> EventDefinition {
        let event = random_event(&self.seen_events, rng);
        self.seen_events.push(event.id.clone());
        event
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

    /// Whether the run is in a shop: a shop encounter, or a visit entered
    /// from the dark web screen. The only times there is a shop visit to open
    /// or buy from.
    pub fn is_in_shop(&self) -> bool {
        self.dark_web_visit || self.is_in_shop_encounter()
    }

    fn is_in_shop_encounter(&self) -> bool {
        self.phase == ContractPhase::EncounterActive
            && self
                .active_encounter
                .as_ref()
                .is_some_and(|encounter| encounter.encounter_type() == EncounterType::Shop)
    }

    /// Whether the player entered the current shop from the dark web screen.
    pub fn in_dark_web_visit(&self) -> bool {
        self.dark_web_visit
    }

    pub fn enter_dark_web_visit(&mut self) -> Result<(), RunStateError> {
        self.require_phase("enter the shop from the dark web", ContractPhase::Hub)?;
        if self.dark_web_visit {
            return Err(RunStateError::InvalidTransition {
                action: "enter the shop from the dark web",
                phase: self.phase,
            });
        }
        self.dark_web_visit = true;
        self.shop_visit = None;
        Ok(())
    }

    pub fn leave_dark_web_visit(&mut self) -> Result<(), RunStateError> {
        if !self.dark_web_visit {
            return Err(RunStateError::InvalidTransition {
                action: "leave the dark web shop visit",
                phase: self.phase,
            });
        }
        self.dark_web_visit = false;
        self.shop_visit = None;
        Ok(())
    }

    /// The shop visit the run is in, opening it with stock rolled from `pool`
    /// if this is the first look at it. Every later call during the same shop
    /// encounter returns that same visit, sold slots included, so the stock
    /// does not change however often the shop screen is rebuilt.
    ///
    /// Refused outside a shop encounter.
    pub fn open_shop_visit(
        &mut self,
        pool: &[(String, Rarity)],
        rng: &mut impl Rng,
    ) -> Result<&ShopVisit, RunStateError> {
        if !self.is_in_shop() {
            return Err(RunStateError::NotInShop { phase: self.phase });
        }
        Ok(self
            .shop_visit
            .get_or_insert_with(|| ShopVisit::open(pool, rng)))
    }

    /// The shop visit the run is in, once it has been opened.
    pub fn shop_visit(&self) -> Option<&ShopVisit> {
        self.shop_visit.as_ref()
    }

    /// The type and zone of the encounter completed most recently, which is
    /// what a card reward waiting at the hub was paid for. `None` before any
    /// encounter on the contract has been completed.
    pub fn last_completed_encounter(&self) -> Option<(EncounterType, usize)> {
        let node_id = self.contract_map.last_completed_node()?;
        let (node, zone) = self.contract_map.node_and_zone(node_id)?;
        Some((node.encounter_type(), zone))
    }

    /// Buys the card in `slot` of the open shop visit: takes its price from
    /// `credits`, adds the card to `deck`, and records the spend and the card
    /// on the run's stats, all in this one call (see [`ShopVisit::buy`]).
    ///
    /// Refused with [`ShopError::NotInShop`] unless the run is in a shop
    /// encounter whose visit has been opened. A refusal changes nothing.
    pub fn buy_from_shop(
        &mut self,
        slot: usize,
        credits: &mut i64,
        deck: &mut RunDeck,
    ) -> Result<Purchase, ShopError> {
        let purchase = self.open_visit_mut()?.buy(slot, credits, deck)?;
        self.stats.record_credit_change(-purchase.price);
        self.stats.record_card_added();
        Ok(purchase)
    }

    /// Buys Scrub Logs in the open shop visit and records the spend on the
    /// run, in this one call (see [`ShopVisit::scrub_logs`]). Refused, with
    /// nothing changed, as `buy_from_shop` is.
    pub fn scrub_logs_in_shop(
        &mut self,
        credits: &mut i64,
        noise: &mut NoiseLevel,
    ) -> Result<ScrubbedLogs, ShopError> {
        let scrubbed = self.open_visit_mut()?.scrub_logs(credits, noise)?;
        self.stats.record_credit_change(-scrubbed.price);
        Ok(scrubbed)
    }

    /// Rerolls the open shop visit's unsold stock and records the spend on
    /// the run, in this one call (see [`ShopVisit::reroll`]). Refused, with
    /// nothing changed, as `buy_from_shop` is.
    pub fn reroll_shop(
        &mut self,
        credits: &mut i64,
        rng: &mut impl Rng,
    ) -> Result<Reroll, ShopError> {
        let reroll = self.open_visit_mut()?.reroll(credits, rng)?;
        self.stats.record_credit_change(-reroll.price);
        Ok(reroll)
    }

    pub fn remove_card_in_shop(
        &mut self,
        card_id: &str,
        credits: &mut i64,
        deck: &mut RunDeck,
    ) -> Result<RemovedCard, ShopError> {
        let removed = self.open_visit_mut()?.remove_card(card_id, credits, deck)?;
        self.stats.record_credit_change(-removed.price);
        Ok(removed)
    }

    pub fn upgrade_card_in_shop(
        &mut self,
        credits: &mut i64,
        deck: &mut RunDeck,
        card_id: &str,
        upgraded_id: &str,
        rarity: Rarity,
    ) -> Result<Upgrade, ShopError> {
        self.open_visit_mut()?;
        let upgrade = shop::upgrade_card(credits, deck, card_id, upgraded_id, rarity)?;
        self.stats.record_credit_change(-upgrade.price);
        Ok(upgrade)
    }

    /// The shop visit to act on, or [`ShopError::NotInShop`] unless the run
    /// is in a shop encounter whose visit has been opened.
    fn open_visit_mut(&mut self) -> Result<&mut ShopVisit, ShopError> {
        if !self.is_in_shop() {
            return Err(ShopError::NotInShop);
        }
        self.shop_visit.as_mut().ok_or(ShopError::NotInShop)
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
        self.shop_visit = None;
        Ok(())
    }

    /// Checks a run read back from a save file is one the transitions above
    /// could have produced, so nothing that relies on their rules can be
    /// handed a run that breaks them.
    ///
    /// The map must hold together on its own (`ContractMap::validate_restored`),
    /// every offer must be one `generate_offers` could have rolled, an
    /// accepted offer must be one of the offers, and the phase must agree
    /// with the active encounter: one is under way while an encounter is
    /// active or the player has just been caught in it, and none otherwise.
    /// An active encounter must also be the node the map has under way, with
    /// that node's type and zone.
    ///
    /// A contract that has not started must still be at the hub with nothing
    /// played on its map, since Change contract is offered then and would
    /// throw that progress away. A contract reward is recorded exactly when
    /// the contract is won, and it is what winning that contract pays.
    ///
    /// The map itself must be the one the accepted offer builds from its
    /// seed and shape, whatever progress has been made on it, so an edited
    /// save cannot shorten the contract or change what its nodes are. A
    /// generated run cannot get under way without accepting an offer, so one
    /// that has started on none is refused. A shop's stock is only kept while
    /// the run is in that shop, and every event the run has seen must exist.
    pub fn validate_restored(&self) -> Result<(), RunStateError> {
        self.contract_map
            .validate_restored()
            .map_err(RunStateError::ContractMap)?;
        self.validate_restored_map_layout()?;
        self.validate_restored_seen_events()?;
        if self.dark_web_visit && self.phase != ContractPhase::Hub {
            return Err(RunStateError::InconsistentRun(
                "a dark web shop visit is kept for a run not at the hub",
            ));
        }
        if self.shop_visit.is_some() && !self.is_in_shop() {
            return Err(RunStateError::InconsistentRun(
                "a shop's stock is kept for a run not in a shop",
            ));
        }

        if !self.contract_offers.iter().all(ContractOffer::is_offerable) {
            return Err(RunStateError::InconsistentRun(
                "a contract offer has a shape or reward no contract is offered with",
            ));
        }
        if let Some(index) = self.accepted_offer_index
            && index >= self.contract_offers.len()
        {
            return Err(RunStateError::UnknownContractOffer {
                index,
                offered: self.contract_offers.len(),
            });
        }

        self.validate_restored_contract_start()?;
        self.validate_restored_contract_reward()?;

        let expects_encounter = matches!(
            self.phase,
            ContractPhase::EncounterActive | ContractPhase::Caught
        );
        match (&self.active_encounter, self.contract_map.active_node()) {
            (None, None) if !expects_encounter => Ok(()),
            (Some(encounter), Some(active_node)) if expects_encounter => {
                self.validate_restored_encounter(encounter, active_node)
            }
            _ => Err(RunStateError::InconsistentRun(
                "the phase and the encounter under way disagree",
            )),
        }
    }

    /// The accepted offer's map, laid out exactly as that offer builds it.
    /// Offers are only ever empty on the authored run the tests use, which
    /// has no offer to compare with.
    fn validate_restored_map_layout(&self) -> Result<(), RunStateError> {
        match self.accepted_offer() {
            Some(offer) if !self.contract_map.has_same_layout(&offer.build_map()) => {
                Err(RunStateError::InconsistentRun(
                    "the map is not the one the accepted contract builds",
                ))
            }
            None if self.contract_started && !self.contract_offers.is_empty() => Err(
                RunStateError::InconsistentRun("a contract under way was never accepted"),
            ),
            _ => Ok(()),
        }
    }

    /// Until an encounter is entered the player may still change contract,
    /// which replaces the map, so a run claiming not to have started must
    /// have nothing on its map to lose.
    /// Every event the run has seen must be one this build has. Repeats are
    /// allowed (see `seen_events`).
    fn validate_restored_seen_events(&self) -> Result<(), RunStateError> {
        let catalogue = event_catalogue();
        let is_known = |id: &String| catalogue.iter().any(|event| event.id == *id);
        if !self.seen_events.iter().all(is_known) {
            return Err(RunStateError::InconsistentRun(
                "a seen event is not one this build has",
            ));
        }
        Ok(())
    }

    fn validate_restored_contract_start(&self) -> Result<(), RunStateError> {
        if self.contract_started {
            return Ok(());
        }
        if self.phase != ContractPhase::Hub || !self.contract_map.is_untouched() {
            return Err(RunStateError::InconsistentRun(
                "a contract with progress on it is marked as not started",
            ));
        }
        Ok(())
    }

    /// The reward is recorded by the transition into `ContractWon` and by
    /// nothing else, so it is there exactly in that phase, and it is the
    /// accepted offer's reward (0 on the default contract). A save claiming
    /// any other amount would be paid nothing extra on Continue, but the
    /// victory screen would show a payout that never happened.
    fn validate_restored_contract_reward(&self) -> Result<(), RunStateError> {
        let won = self.phase == ContractPhase::ContractWon;
        match self.contract_reward {
            None if won => Err(RunStateError::InconsistentRun(
                "a won contract has no contract reward recorded",
            )),
            Some(_) if !won => Err(RunStateError::InconsistentRun(
                "a contract reward is recorded for a contract not won",
            )),
            Some(reward) if reward != self.reward_for_winning() => {
                Err(RunStateError::InconsistentRun(
                    "the contract reward is not what the accepted contract pays",
                ))
            }
            _ => Ok(()),
        }
    }

    fn validate_restored_encounter(
        &self,
        encounter: &ActiveEncounter,
        active_node: NodeId,
    ) -> Result<(), RunStateError> {
        if encounter.id != active_node.to_string() {
            return Err(RunStateError::InconsistentRun(
                "the active encounter is not the node under way on the map",
            ));
        }
        let (node, zone) =
            self.contract_map
                .node_and_zone(active_node)
                .ok_or(RunStateError::ContractMap(ContractMapError::UnknownNode(
                    active_node,
                )))?;
        if node.encounter_type() != encounter.encounter_type || zone != encounter.zone {
            return Err(RunStateError::InconsistentRun(
                "the active encounter's type or zone does not match its node",
            ));
        }
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
    /// A run read back from a save breaks one of the rules its transitions
    /// keep. Says which.
    InconsistentRun(&'static str),
    /// A shop visit was asked for while the run is not in a shop encounter.
    NotInShop {
        phase: ContractPhase,
    },
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
            Self::InconsistentRun(reason) => {
                write!(formatter, "the saved run is invalid: {reason}")
            }
            Self::NotInShop { phase } => write!(
                formatter,
                "cannot open a shop while not in a shop encounter (phase is {})",
                phase.as_str()
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

    /// Story #110: quitting to the menu mid-fight leaves a fresh run
    /// behind the menu, where starting one normally would be refused.
    #[test]
    fn a_run_can_be_abandoned_mid_encounter() {
        let mut state = generated_run(5);
        enter_first_encounter(&mut state);
        assert!(state.start_new_run(&mut StdRng::seed_from_u64(3)).is_err());

        state.abandon(&mut StdRng::seed_from_u64(3));

        assert_eq!(state.phase(), ContractPhase::Hub);
        assert_eq!(state.active_encounter(), None);
        assert!(!state.contract_started());
    }

    /// Story #116, scenario 2: the run remembers which events it has shown,
    /// so a later event node gets a different one.
    #[test]
    fn a_run_does_not_roll_the_same_event_twice() {
        let mut state = RunState::new();
        let mut rng = StdRng::seed_from_u64(13);

        let first = state.roll_event(&mut rng);
        for _ in 0..20 {
            let mut retry = state.clone();
            let next = retry.roll_event(&mut rng);
            assert_ne!(next.id, first.id, "the run showed '{}' twice", first.id);
        }
    }

    /// The seen events belong to the run, so a new run can see them again.
    #[test]
    fn a_new_run_forgets_the_events_it_has_seen() {
        let mut state = generated_run(5);
        state.roll_event(&mut StdRng::seed_from_u64(1));

        state
            .start_new_run(&mut StdRng::seed_from_u64(3))
            .expect("a new run can start from the hub");

        assert_eq!(state.seen_events, Vec::<String>::new());
    }

    /// A run read back from a save may only have seen events this build has.
    /// Repeats are fine: once every event has been seen, the whole catalogue
    /// is drawn from again.
    #[test]
    fn a_restored_run_must_only_have_seen_real_events() {
        let mut state = RunState::new();
        let mut rng = StdRng::seed_from_u64(17);
        for _ in 0..event_catalogue().len() + 2 {
            state.roll_event(&mut rng);
        }
        assert!(state.seen_events.len() > event_catalogue().len());
        assert_eq!(state.validate_restored(), Ok(()));

        state.seen_events.push("not_a_real_event".to_string());

        assert_eq!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(
                "a seen event is not one this build has"
            ))
        );
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

    // --- Restoring a saved run (story #10) ---------------------------------

    /// Whatever the transitions reach, a save of it must read back. Walks a
    /// generated contract through every phase, checking at each step.
    #[test]
    fn every_run_the_transitions_reach_passes_the_restore_check() {
        let mut state = generated_run(21);
        state.accept_contract_offer(2).expect("third offer");
        assert_eq!(
            state.validate_restored(),
            Ok(()),
            "a run that has just started"
        );

        enter_first_encounter(&mut state);
        assert_eq!(state.validate_restored(), Ok(()), "an encounter under way");
        state.report_caught().unwrap();
        assert_eq!(state.validate_restored(), Ok(()), "caught in it");
        state.finish_caught().unwrap();
        assert_eq!(
            state.validate_restored(),
            Ok(()),
            "back at the hub to retry"
        );

        play_to_victory(&mut state);
        assert_eq!(
            state.validate_restored(),
            Ok(()),
            "a contract won, with its reward recorded"
        );

        state.start_new_run(&mut StdRng::seed_from_u64(22)).unwrap();
        assert_eq!(state.validate_restored(), Ok(()), "a new run after it");
    }

    /// A run that accepted an offer and then changed its mind is still one
    /// the player can change again after Continue.
    #[test]
    fn a_changed_contract_before_any_encounter_passes_the_restore_check() {
        let mut state = generated_run(23);
        state.accept_contract_offer(2).expect("third offer");
        state
            .accept_contract_offer(0)
            .expect("changed to the first");

        assert_eq!(state.validate_restored(), Ok(()));
        assert!(state.can_accept_contract_offer(), "Change contract stays");
        assert!(
            !state.needs_contract_choice(),
            "the map shows straight away"
        );
    }

    #[test]
    fn a_contract_with_progress_marked_as_not_started_is_refused() {
        let mut at_hub = generated_run(24);
        at_hub.accept_contract_offer(1).expect("second offer");
        enter_first_encounter(&mut at_hub);
        at_hub
            .complete_encounter()
            .expect("the encounter is active");
        at_hub.contract_started = false;

        let mut in_encounter = generated_run(24);
        enter_first_encounter(&mut in_encounter);
        in_encounter.contract_started = false;

        for state in [at_hub, in_encounter] {
            assert!(
                matches!(
                    state.validate_restored(),
                    Err(RunStateError::InconsistentRun(_))
                ),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_won_contract_without_its_reward_recorded_is_refused() {
        let mut state = generated_run(25);
        state.accept_contract_offer(0).expect("first offer");
        play_to_victory(&mut state);
        state.contract_reward = None;

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    #[test]
    fn a_contract_reward_recorded_before_the_contract_is_won_is_refused() {
        let mut state = generated_run(26);
        state.accept_contract_offer(0).expect("first offer");
        state.contract_reward = Some(state.accepted_offer().unwrap().credit_reward);

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    #[test]
    fn a_contract_reward_other_than_the_accepted_offers_is_refused() {
        let mut state = generated_run(27);
        state.accept_contract_offer(1).expect("second offer");
        play_to_victory(&mut state);
        let reward = state.contract_reward().expect("won");
        state.contract_reward = Some(reward + 1);

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    #[test]
    fn an_active_encounter_the_map_does_not_have_under_way_is_refused() {
        let mut state = active_combat();
        state.active_encounter = Some(ActiveEncounter {
            id: "2".to_string(),
            encounter_type: EncounterType::Event,
            zone: 0,
        });

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    #[test]
    fn an_active_encounter_with_the_wrong_type_is_refused() {
        let mut state = active_combat();
        state.active_encounter = Some(ActiveEncounter {
            id: "1".to_string(),
            encounter_type: EncounterType::Shop,
            zone: 0,
        });

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    #[test]
    fn a_hub_run_with_an_encounter_still_active_is_refused() {
        let mut state = active_combat();
        state.phase = ContractPhase::Hub;

        assert!(matches!(
            state.validate_restored(),
            Err(RunStateError::InconsistentRun(_))
        ));
    }

    // --- the shop visit belongs to the run --------------------------------

    fn shop_pool() -> Vec<(String, Rarity)> {
        [
            ("strike", Rarity::Common),
            ("vpn", Rarity::Common),
            ("phish", Rarity::Uncommon),
            ("rootkit", Rarity::Uncommon),
            ("zero_day", Rarity::Rare),
            ("wannacry", Rarity::Rare),
            ("stuxnet", Rarity::Legendary),
        ]
        .iter()
        .map(|(card_id, rarity)| (card_id.to_string(), *rarity))
        .collect()
    }

    /// The authored contract with its shop (node 3) entered.
    fn in_shop() -> RunState {
        let mut state = RunState::new();
        state.select_encounter(1).unwrap();
        state.complete_encounter().unwrap();
        state.select_encounter(3).unwrap();
        state
    }

    #[test]
    fn only_a_shop_encounter_is_in_a_shop() {
        let mut state = RunState::new();
        assert!(!state.is_in_shop(), "the hub is not a shop");

        state.select_encounter(1).unwrap();
        assert!(!state.is_in_shop(), "a combat is not a shop");

        assert!(in_shop().is_in_shop());
    }

    #[test]
    fn reopening_the_shop_during_a_visit_keeps_its_stock_and_sold_slots() {
        let mut state = in_shop();
        let first = state
            .open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(1))
            .unwrap()
            .clone();
        let mut credits = 1_000;
        let mut deck = RunDeck::default();
        state.buy_from_shop(0, &mut credits, &mut deck).unwrap();

        // A different seed would roll different stock if it rolled at all.
        let reopened = state
            .open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(99))
            .unwrap();

        assert_eq!(reopened.slots().len(), first.slots().len());
        for (index, (now, was)) in reopened.slots().iter().zip(first.slots()).enumerate() {
            assert_eq!(now.card_id(), was.card_id(), "slot {index} changed card");
            assert_eq!(now.is_sold(), index == 0, "slot {index} sold state");
        }
    }

    #[test]
    fn a_shop_cannot_be_opened_or_bought_from_outside_a_shop_encounter() {
        let mut state = active_combat();
        let mut credits = 1_000;
        let mut deck = RunDeck::default();

        assert_eq!(
            state.open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(1)),
            Err(RunStateError::NotInShop {
                phase: ContractPhase::EncounterActive
            })
        );
        assert_eq!(
            state.buy_from_shop(0, &mut credits, &mut deck),
            Err(ShopError::NotInShop)
        );
        assert_eq!(credits, 1_000);
        assert!(deck.is_empty());
    }

    #[test]
    fn buying_before_the_shop_is_opened_is_refused() {
        let mut state = in_shop();
        let mut credits = 1_000;
        let mut deck = RunDeck::default();

        assert_eq!(
            state.buy_from_shop(0, &mut credits, &mut deck),
            Err(ShopError::NotInShop)
        );
        assert_eq!(credits, 1_000);
    }

    #[test]
    fn a_purchase_records_the_spend_and_the_card_on_the_run() {
        let mut state = in_shop();
        state
            .open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(2))
            .unwrap();
        let earned_before = state.summary().stats.credits_earned();
        let mut credits = 1_000;
        let mut deck = RunDeck::default();

        let purchase = state.buy_from_shop(1, &mut credits, &mut deck).unwrap();

        assert_eq!(credits, 1_000 - purchase.price);
        assert_eq!(deck.count_of(&purchase.card_id), 1);
        assert_eq!(state.summary().stats.cards_added(), 1);
        assert_eq!(
            state.summary().stats.credits_earned(),
            earned_before,
            "a spend is not earnings"
        );
    }

    #[test]
    fn leaving_the_shop_ends_the_visit_and_the_next_shop_rolls_afresh() {
        let mut state = in_shop();
        state
            .open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(3))
            .unwrap();

        state.complete_encounter().unwrap();

        assert_eq!(state.shop_visit(), None);
        assert_eq!(
            state.open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(3)),
            Err(RunStateError::NotInShop {
                phase: ContractPhase::Hub
            })
        );
    }

    #[test]
    fn shop_services_record_their_spend_and_are_refused_outside_a_shop() {
        let mut state = in_shop();
        state
            .open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(4))
            .unwrap();
        let mut credits = 1_000;
        let mut noise = NoiseLevel::new(100);
        noise.add(50);
        let mut rng = StdRng::seed_from_u64(5);

        let scrubbed = state.scrub_logs_in_shop(&mut credits, &mut noise).unwrap();
        let reroll = state.reroll_shop(&mut credits, &mut rng).unwrap();

        assert_eq!(credits, 1_000 - scrubbed.price - reroll.price);
        assert_eq!(state.summary().stats.cards_added(), 0);

        state.complete_encounter().unwrap();
        assert_eq!(
            state.scrub_logs_in_shop(&mut credits, &mut noise),
            Err(ShopError::NotInShop)
        );
        assert_eq!(
            state.reroll_shop(&mut credits, &mut rng),
            Err(ShopError::NotInShop)
        );
        assert_eq!(credits, 1_000 - scrubbed.price - reroll.price);
    }
}

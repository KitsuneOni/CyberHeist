mod audio_mix;
mod card_data;
mod card_database;
mod card_play;
mod card_reward;
mod card_text;
mod contract_map;
mod contract_offer;
mod deck;
mod effects;
mod encounter_reward;
mod encounter_text;
mod events;
mod events_node;
mod knowledge;
mod noise_meter;
mod progression_node;
mod relics;
mod run_deck;
mod run_deck_node;
mod run_state;
mod run_state_node;
mod run_stats;
mod save_game;
mod save_game_node;
mod sentry;
mod sentry_node;
mod shop;
mod shop_node;
mod starter_deck;

use audio_mix::SoundCue;
use card_data::CardData;
use card_database::CardDatabase;
use deck::Deck;
use effects::PlayerEffects;
use godot::builtin::{VarDictionary, dict};
use godot::prelude::*;
use knowledge::KnowledgeMeter;
use noise_meter::NoiseMeter;
use progression_node::{PROGRESSION_PATH, ProgressionNode};
use run_deck_node::RunDeckNode;
use sentry_node::SentryNode;

/// Where `RunDeckNode` lives relative to the scene root. Per its own doc
/// comment it's a child of the `FlowCoordinator` autoload, not its own
/// top-level autoload — confirm this path matches your actual scene tree.
const RUN_DECK_PATH: &str = "/root/FlowCoordinator/RunDeck";

// Where the shared autoloads live. Every Rust node that reaches one looks it
// up by these, so renaming an autoload only touches this one place.
pub(crate) const CARD_DATABASE_PATH: &str = "/root/CardDatabaseGlobal";
pub(crate) const PLAYER_STATE_PATH: &str = "/root/PlayerStateGlobal";
pub(crate) const NOISE_METER_PATH: &str = "/root/NoiseMeterGlobal";
pub(crate) const KNOWLEDGE_METER_PATH: &str = "/root/KnowledgeMeterGlobal";

/// `{ok: false, error}`: how every Rust node reports a refusal to GDScript.
pub(crate) fn failure(error: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => error,
    }
}

/// A card's player-facing details as a dictionary, the shape every screen
/// that describes a card reads: `{name, type, rarity, cost, cost_text,
/// noise_text, description, keywords: [{label, explanation}]}`. Callers add
/// their own fields on top, such as an id or a shop price.
pub(crate) fn card_detail_dictionary(detail: &card_text::CardDetail) -> VarDictionary {
    let mut keywords: Array<VarDictionary> = Array::new();
    for keyword in &detail.keywords {
        keywords.push(&vdict! {
            "label" => keyword.label.as_str(),
            "explanation" => keyword.explanation.as_str(),
        });
    }

    vdict! {
        "name" => detail.name.as_str(),
        "type" => detail.card_type.as_str(),
        "rarity" => detail.rarity.as_str(),
        "cost" => detail.cost,
        "cost_text" => detail.cost_text.as_str(),
        "noise_text" => detail.noise_text.as_str(),
        "description" => detail.description.as_str(),
        "keywords" => &keywords,
    }
}

struct CyberHeistExtension;

#[gdextension]
unsafe impl ExtensionLibrary for CyberHeistExtension {}

#[derive(GodotClass)]
#[class(base=Node)]
struct BridgeCheck {
    base: Base<Node>,
}

#[godot_api]
impl INode for BridgeCheck {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    fn ready(&mut self) {
        godot_print!("CyberHeist: Rust bridge is live (BridgeCheck::ready).");
    }
}

#[godot_api]
impl BridgeCheck {
    #[func]
    fn ping(&self) -> GString {
        "pong from Rust".into()
    }
}

#[derive(GodotClass)]
#[class(base=Node)]
struct PlayerState {
    base: Base<Node>,
    money: i64,
    upgrades: Vec<GString>,
}

#[godot_api]
impl INode for PlayerState {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            money: 0,
            upgrades: Vec::new(),
        }
    }
}

#[godot_api]
impl PlayerState {
    #[func]
    fn add_upgrade(&mut self, name: GString) {
        self.upgrades.push(name);
    }

    /// The upgrades earned on the current contract, in the order they were
    /// gained.
    #[func]
    fn upgrades(&self) -> Array<GString> {
        self.upgrades.iter().cloned().collect()
    }

    /// Drops the upgrades earned on the current contract and keeps the credit
    /// balance.
    ///
    /// Upgrades only last for the contract they were earned on, while credits
    /// are the player's balance between runs. `FlowCoordinator.start_new_run()`
    /// calls this so a won contract's upgrades never leak into the next run;
    /// being caught already loses them through `apply_penalty`.
    #[func]
    fn clear_upgrades(&mut self) {
        self.upgrades.clear();
    }

    #[func]
    fn money(&self) -> i64 {
        self.money
    }

    #[func]
    fn add_credits(&mut self, amount: i64) {
        self.money += amount;
    }

    #[func]
    fn apply_penalty(&mut self, fine: i64) -> VarDictionary {
        let lost: Array<GString> = self.upgrades.drain(..).collect();
        self.money -= fine;

        dict! {
            "fine" => fine,
            "lost_upgrades" => &lost,
        }
    }
}

impl PlayerState {
    /// The credit balance itself, for a Rust rule that has to check and spend
    /// it in one step. `ShopNode` lends it to `RunState::buy_from_shop`, which
    /// refuses rather than take the balance below zero.
    pub(crate) fn credits_mut(&mut self) -> &mut i64 {
        &mut self.money
    }

    /// The current contract's upgrades, for saving the run.
    pub(crate) fn upgrade_names(&self) -> Vec<String> {
        self.upgrades.iter().map(GString::to_string).collect()
    }

    /// Replaces the current contract's upgrades, for a run resumed from a
    /// save.
    pub(crate) fn restore_upgrades(&mut self, names: &[String]) {
        self.upgrades = names
            .iter()
            .map(|name| GString::from(name.as_str()))
            .collect();
    }
}

/// Godot-facing node for the draw phase.
///
/// Wraps a [`Deck`] built each encounter from whatever `RunDeckNode` says the
/// run currently owns — not from `starter_deck.ron` directly, so a card
/// gained mid-run (e.g. via `RunDeckNode::add_card`) is actually available at
/// the next encounter instead of being silently discarded.
///
/// Also holds this combat's persistent player effects (see `effects.rs`).
/// The node is rebuilt for every combat screen, so they never carry into the
/// next encounter.
#[derive(GodotClass)]
#[class(base=Node)]
struct DrawPhase {
    #[export]
    hand_size: i32,

    #[export]
    play_count: i32,

    #[export]
    turn_number: i32,

    #[export]
    max_energy: i32,

    #[export]
    energy: i32,

    deck: Deck,
    hand: Vec<CardData>,
    noise_meter: Option<Gd<NoiseMeter>>,
    sentry_node: Option<Gd<SentryNode>>,
    knowledge_meter: Option<Gd<KnowledgeMeter>>,
    effects: PlayerEffects,
    base: Base<Node>,
}

#[godot_api]
impl INode for DrawPhase {
    fn init(base: Base<Node>) -> Self {
        Self {
            hand_size: 5,
            play_count: 0,
            turn_number: 1,
            max_energy: 3,
            energy: 3,
            deck: Deck::new(Vec::new(), &mut rand::rng()),
            hand: Vec::new(),
            noise_meter: None,
            sentry_node: None,
            knowledge_meter: None,
            effects: PlayerEffects::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.noise_meter = Some(self.base().get_node_as::<NoiseMeter>(NOISE_METER_PATH));

        // Shield is per-encounter; clearing it here — not just at end_turn —
        // means it can't leak in regardless of how the previous encounter
        // ended (normal completion, caught, or a mid-turn sentry defeat).
        self.noise_meter().clone().bind_mut().clear_shield();

        // DrawPhase and Sentry are sibling nodes under CombatScreen.
        self.sentry_node = Some(self.base().get_node_as::<SentryNode>("../Sentry"));

        // Knowledge is run-persistent, unlike noise/shield/health, so it is
        // deliberately NOT reset here — it carries across encounters.
        self.knowledge_meter = Some(
            self.base()
                .get_node_as::<KnowledgeMeter>(KNOWLEDGE_METER_PATH),
        );

        let card_db = self.base().get_node_as::<CardDatabase>(CARD_DATABASE_PATH);
        let card_db = card_db.bind();

        // The run's cards live on RunDeckNode, not starter_deck.ron directly.
        // Reading it fresh here means every encounter deals from whatever
        // the run currently owns, including anything gained since the last
        // encounter (e.g. a reward picked via RunDeckNode::add_card).
        let run_deck = self.base().get_node_as::<RunDeckNode>(RUN_DECK_PATH);
        let owned_ids = run_deck.bind().owned_card_ids();

        let mut cards = Vec::with_capacity(owned_ids.len());
        let mut missing = Vec::new();
        for id in &owned_ids {
            match card_db.get(id) {
                Some(card) => cards.push(card.clone()),
                None => missing.push(id.clone()),
            }
        }
        if !missing.is_empty() {
            godot_error!(
                "RunDeckNode owns ids that do not exist in cards.ron: {}",
                missing.join(", ")
            );
        }

        let mut rng = rand::rng();
        self.deck = Deck::new(cards, &mut rng);
    }
}

#[godot_api]
impl DrawPhase {
    /// Draws a new hand of `hand_size` randomized cards, reshuffling the
    /// discard pile into the draw pile if it runs out mid-draw.
    #[func]
    fn draw_hand(&mut self) -> PackedStringArray {
        let previous_hand = std::mem::take(&mut self.hand);
        self.deck.discard(previous_hand);
        self.energy = self.max_energy;

        let mut rng = rand::rng();
        self.hand = self.deck.draw_hand(self.hand_size as usize, &mut rng);

        let noise = self.current_noise();
        self.hand
            .iter()
            .map(|card| GString::from(card.display_name(noise).as_str()))
            .collect()
    }

    /// Resolves one paid play against the shared noise meter, the sentry's
    /// health, run-persistent knowledge, this combat's persistent effects,
    /// and this encounter's deck/hand for card draw. The model owns
    /// validation, energy, discard, and every signed/clamped effect as one
    /// transaction.
    #[func]
    fn play_card(&mut self, index: i32) -> VarDictionary {
        let mut meter = self.noise_meter().clone();
        if meter.bind().is_at_cap() {
            return vdict! { "ok" => false, "error" => "detected" };
        }
        if !self.base().is_inside_tree() || self.base().is_queued_for_deletion() {
            return vdict! { "ok" => false, "error" => "inactive_encounter" };
        }
        let mut sentry_node = self.sentry_node().clone();
        let mut knowledge_meter = self.knowledge_meter().clone();
        let mut rng = rand::rng();
        let result = card_play::play_card(
            &mut self.hand,
            &mut self.deck,
            &mut self.energy,
            &mut self.max_energy,
            meter.bind_mut().level_mut(),
            sentry_node.bind_mut().sentry_mut(),
            knowledge_meter.bind_mut().level_mut(),
            &mut self.effects,
            &mut rng,
            index,
        );
        match result {
            Ok(played) => {
                self.play_count += 1;
                vdict! {
                    "ok" => true,
                    "name" => played.name.as_str(),
                    "type" => card_text::card_type_name(played.card_type),
                    "sound_cue" => SoundCue::for_card_type(played.card_type).id(),
                    "noise_change" => played.noise_change,
                    "damage_dealt" => played.damage_dealt,
                    "damage_blocked" => played.damage_blocked,
                    "shield_added" => played.shield_added,
                    "cards_drawn" => played.cards_drawn,
                    "knowledge_change" => played.knowledge_change,
                    "max_energy_gained" => played.max_energy_gained,
                    "corruption_added" => played.corruption_added,
                    "corruption_boost_added" => played.corruption_boost_added,
                    "energy_tax_paid" => played.energy_tax_paid,
                    "exhausted" => played.exhausted,
                    "firewall_stripped" => played.firewall_stripped,
                    "might_gained" => played.might_gained,
                }
            }
            Err(error) => {
                let error = match error {
                    card_play::PlayError::Detected => "detected",
                    card_play::PlayError::InvalidIndex => "invalid_index",
                    card_play::PlayError::NotEnoughEnergy => "not_enough_energy",
                };
                vdict! { "ok" => false, "error" => error }
            }
        }
    }

    /// The card ids in the hand, in the same order as `hand_names`. Unlike a
    /// name, an id does not change with noise, so this is what to compare
    /// against the run's deck.
    #[func]
    fn hand_ids(&self) -> PackedStringArray {
        self.hand
            .iter()
            .map(|card| GString::from(card.id.as_str()))
            .collect()
    }

    #[func]
    fn hand_names(&self) -> PackedStringArray {
        let noise = self.current_noise();
        self.hand
            .iter()
            .map(|c| GString::from(c.display_name(noise).as_str()))
            .collect()
    }

    #[func]
    fn card_detail(&self, index: i32) -> VarDictionary {
        let Ok(index) = usize::try_from(index) else {
            return vdict! { "ok" => false };
        };
        let Some(card) = self.hand.get(index) else {
            return vdict! { "ok" => false };
        };

        let noise = self.current_noise();
        let mut detail = card_detail_dictionary(&card_text::describe(card, noise));
        detail.set("ok", true);
        detail
    }

    #[func]
    fn hand_costs(&self) -> PackedInt32Array {
        let noise = self.current_noise();
        self.hand
            .iter()
            .enumerate()
            .map(|(index, card)| {
                let tax = card_play::hand_energy_tax(&self.hand, index, noise);
                card.cost as i32 + tax
            })
            .collect()
    }

    #[func]
    fn discard_hand(&mut self) {
        let hand = std::mem::take(&mut self.hand);
        self.deck.discard(hand);
    }

    #[signal]
    fn security_phase(finished_turn: i32);

    #[func]
    fn end_turn(&mut self) -> PackedStringArray {
        if !self.base().is_inside_tree()
            || self.base().is_queued_for_deletion()
            || self.noise_meter().bind().is_at_cap()
        {
            return self.hand_names();
        }
        let finished_turn = self.turn_number;

        {
            let mut meter = self.noise_meter().clone();
            let mut sentry_node = self.sentry_node().clone();
            let mut knowledge_meter = self.knowledge_meter().clone();
            let mut rng = rand::rng();
            card_play::resolve_end_of_turn_effects(
                &mut self.hand,
                &mut self.deck,
                &mut self.energy,
                &mut self.max_energy,
                meter.bind_mut().level_mut(),
                sentry_node.bind_mut().sentry_mut(),
                knowledge_meter.bind_mut().level_mut(),
                &mut self.effects,
                &mut rng,
            );
        }

        let remaining_hand = std::mem::take(&mut self.hand);
        self.deck.discard(remaining_hand);

        self.signals().security_phase().emit(finished_turn);
        // Planted before the next hand is drawn, so a reshuffle on this draw
        // can already bring the junk back round.
        self.plant_injected_malware();
        self.noise_meter().clone().bind_mut().clear_shield();
        self.sentry_node()
            .clone()
            .bind_mut()
            .sentry_mut()
            .clear_corruption_boost();
        self.apply_starting_corrupting_boost();

        self.turn_number += 1;
        self.energy = self.max_energy;

        let mut rng = rand::rng();
        self.hand = self.deck.draw_hand(self.hand_size as usize, &mut rng);

        let noise = self.current_noise();
        self.hand
            .iter()
            .map(|card| GString::from(card.display_name(noise).as_str()))
            .collect()
    }

    /// Takes energy off the turn now under way and reports how much actually
    /// went, which is less than asked for when there was not that much left.
    ///
    /// Called by the combat screen after `end_turn` has refreshed the pool, so
    /// a boss lockdown bites into the turn it opens rather than the one that
    /// has just been spent. A negative amount is ignored: this drains, it is
    /// not a back door for handing energy out.
    #[func]
    fn drain_energy(&mut self, amount: i32) -> i32 {
        let before = self.energy;
        self.energy = self.energy.saturating_sub(amount.max(0)).max(0);
        before - self.energy
    }

    #[func]
    fn apply_run_bonuses(&mut self) {
        let Some(progression_node) = self
            .base()
            .try_get_node_as::<ProgressionNode>(PROGRESSION_PATH)
        else {
            return;
        };
        let (extra_energy, extra_draw) = {
            let progression_node = progression_node.bind();
            let progression = progression_node.progression();
            (
                progression.bonus_max_energy(),
                progression.bonus_cards_drawn(),
            )
        };

        self.max_energy += extra_energy;
        self.energy = self.max_energy;
        self.hand_size += extra_draw;
        self.apply_starting_corrupting_boost();
    }

    #[func]
    fn draw_pile_count(&self) -> i32 {
        self.deck.draw_pile_len() as i32
    }

    #[func]
    fn discard_pile_count(&self) -> i32 {
        self.deck.discard_pile_len() as i32
    }

    /// Card ids in the draw pile, so a screen or test can see what is in it
    /// without drawing.
    #[func]
    fn draw_pile_ids(&self) -> PackedStringArray {
        self.deck
            .draw_pile_ids()
            .iter()
            .map(|id| GString::from(id.as_str()))
            .collect()
    }

    /// Card ids in the discard pile, oldest first.
    #[func]
    fn discard_pile_ids(&self) -> PackedStringArray {
        self.deck
            .discard_pile_ids()
            .iter()
            .map(|id| GString::from(id.as_str()))
            .collect()
    }

    /// Stacks of Might for this combat.
    #[func]
    fn might(&self) -> i32 {
        self.effects.might()
    }
}

impl DrawPhase {
    fn noise_meter(&self) -> &Gd<NoiseMeter> {
        self.noise_meter.as_ref().expect("DrawPhase must be ready")
    }

    fn sentry_node(&self) -> &Gd<SentryNode> {
        self.sentry_node.as_ref().expect("DrawPhase must be ready")
    }

    fn knowledge_meter(&self) -> &Gd<KnowledgeMeter> {
        self.knowledge_meter
            .as_ref()
            .expect("DrawPhase must be ready")
    }

    fn current_noise(&self) -> i32 {
        self.noise_meter().bind().get_noise()
    }

    fn starting_corrupting_boost(&self) -> i32 {
        let Some(node) = self
            .base()
            .try_get_node_as::<ProgressionNode>(PROGRESSION_PATH)
        else {
            return 0;
        };
        let node = node.bind();
        node.progression().starting_corrupting_boost()
    }

    fn apply_starting_corrupting_boost(&mut self) {
        let boost = self.starting_corrupting_boost();
        if boost > 0 {
            self.sentry_node()
                .clone()
                .bind_mut()
                .sentry_mut()
                .add_corruption_boost(boost);
        }
    }

    /// Puts whatever Inject Malware the sentry just ran into this
    /// encounter's discard pile. Only this deck, never `RunDeckNode`, so the
    /// junk is gone once the encounter ends.
    fn plant_injected_malware(&mut self) {
        let count = self
            .sentry_node()
            .clone()
            .bind_mut()
            .take_malware_to_plant();
        if count <= 0 {
            return;
        }

        let card_db = self.base().get_node_as::<CardDatabase>(CARD_DATABASE_PATH);
        let Some(malware) = card_db.bind().get(sentry::MALWARE_CARD_ID).cloned() else {
            godot_error!(
                "Inject Malware has no card to plant: '{}' is missing from cursed_cards.ron",
                sentry::MALWARE_CARD_ID
            );
            return;
        };
        card_play::plant_junk(&mut self.deck, &malware, count);
    }
}
mod card_data;
mod card_database;
mod card_play;
mod card_reward;
mod card_text;
mod contract_map;
mod deck;
mod encounter_text;
mod events;
mod events_node;
mod knowledge;
mod noise_meter;
mod run_deck;
mod run_deck_node;
mod run_state;
mod run_state_node;
mod sentry;
mod sentry_node;
mod starter_deck;

use card_data::CardData;
use card_database::CardDatabase;
use deck::Deck;
use godot::builtin::{VarDictionary, dict};
use godot::prelude::*;
use knowledge::KnowledgeMeter;
use noise_meter::NoiseMeter;
use run_deck_node::RunDeckNode;
use sentry_node::SentryNode;

/// Where `RunDeckNode` lives relative to the scene root. Per its own doc
/// comment it's a child of the `FlowCoordinator` autoload, not its own
/// top-level autoload — confirm this path matches your actual scene tree.
const RUN_DECK_PATH: &str = "/root/FlowCoordinator/RunDeck";

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

/// Godot-facing node for the draw phase.
///
/// Wraps a [`Deck`] built each encounter from whatever `RunDeckNode` says the
/// run currently owns — not from `starter_deck.ron` directly, so a card
/// gained mid-run (e.g. via `RunDeckNode::add_card`) is actually available at
/// the next encounter instead of being silently discarded.
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
            base,
        }
    }

    fn ready(&mut self) {
        self.noise_meter = Some(
            self.base()
                .get_node_as::<NoiseMeter>("/root/NoiseMeterGlobal"),
        );

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
                .get_node_as::<KnowledgeMeter>("/root/KnowledgeMeterGlobal"),
        );

        let card_db = self
            .base()
            .get_node_as::<CardDatabase>("/root/CardDatabaseGlobal");
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
    /// health, run-persistent knowledge, and this encounter's deck/hand for
    /// card draw. The model owns validation, energy, discard, and every
    /// signed/clamped effect as one transaction.
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
            &mut rng,
            index,
        );
        match result {
            Ok(played) => {
                self.play_count += 1;
                vdict! {
                    "ok" => true,
                    "name" => played.name.as_str(),
                    "noise_change" => played.noise_change,
                    "damage_dealt" => played.damage_dealt,
                    "shield_added" => played.shield_added,
                    "cards_drawn" => played.cards_drawn,
                    "knowledge_change" => played.knowledge_change,
                    "max_energy_gained" => played.max_energy_gained,
                    "corruption_added" => played.corruption_added,
                    "corruption_boost_added" => played.corruption_boost_added,
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
        let detail = card_text::describe(card, noise);

        let mut keywords: Array<VarDictionary> = Array::new();
        for keyword in &detail.keywords {
            keywords.push(&vdict! {
                "label" => keyword.label.as_str(),
                "explanation" => keyword.explanation.as_str(),
            });
        }

        vdict! {
            "ok" => true,
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

    #[func]
    fn hand_costs(&self) -> PackedInt32Array {
        self.hand.iter().map(|c| c.cost as i32).collect()
    }

    #[func]
    fn discard_hand(&mut self) {
        let hand = std::mem::take(&mut self.hand);
        self.deck.discard(hand);
    }

    #[signal]
    fn security_phase(finished_turn: i32);

    /// Ends the player's turn: every card still in hand goes to the discard
    /// pile (however many there are — a `Draw` play earlier this turn may
    /// have grown the hand past `hand_size`, and all of it still goes to
    /// discard here), the security system gets its phase, then the next turn
    /// begins with refreshed energy and a freshly drawn hand.
    #[func]
    fn end_turn(&mut self) -> PackedStringArray {
        if !self.base().is_inside_tree()
            || self.base().is_queued_for_deletion()
            || self.noise_meter().bind().is_at_cap()
        {
            return self.hand_names();
        }
        let finished_turn = self.turn_number;

        let remaining_hand = std::mem::take(&mut self.hand);
        self.deck.discard(remaining_hand);

        self.signals().security_phase().emit(finished_turn);
        self.noise_meter().clone().bind_mut().clear_shield();
        self.sentry_node()
            .clone()
            .bind_mut()
            .sentry_mut()
            .clear_corruption_boost();

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
    fn draw_pile_count(&self) -> i32 {
        self.deck.draw_pile_len() as i32
    }

    #[func]
    fn discard_pile_count(&self) -> i32 {
        self.deck.discard_pile_len() as i32
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
}

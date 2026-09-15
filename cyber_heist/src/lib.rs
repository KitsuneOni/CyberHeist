//! CyberHeist GDExtension library for Godot 4.
//!
//! Game logic lives in this crate; the Godot project under `../godot` handles
//! scenes, UI and presentation. Add new gameplay modules here and register them
//! as Godot classes with `#[derive(GodotClass)]`.
//!
//! Classes defined here:
//! - 'BridgeCheck' temporary demo proving the bridge between Godot and Rust works. Delete once real gameplay and classes are wired up
//! - 'PlayerState' - autoload singleton tracking player money and upgrades,
//!   and applying the "caught" penalty (fine + lose this contract's upgrades).

mod card_data;
mod card_database;
mod card_text;
mod contract_map;
mod deck;
mod encounter_text;
mod events;
mod events_node;
mod run_state;
mod run_state_node;
mod starter_deck;

use card_data::CardData;
use card_database::CardDatabase;
use deck::Deck;
use godot::builtin::{VarDictionary, dict};
use godot::prelude::*;

struct CyberHeistExtension;

#[gdextension]
unsafe impl ExtensionLibrary for CyberHeistExtension {}

/// Temporary node that proves the Rust <-> Godot bridge is wired up correctly.
///
/// This no longer appears in the bootstrap scene, but remains available for
/// targeted bridge diagnostics until a later cleanup removes it.
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
    /// Callable from GDScript to confirm calls cross the bridge, e.g.
    /// `print($BridgeCheck.ping())`.
    #[func]
    fn ping(&self) -> GString {
        "pong from Rust".into()
    }
}

/// Tracks player money and the upgrades earned so far during the current contract.
/// Exposed as an autoload singleton so GDScript can call it from anywhere
#[derive(GodotClass)]
#[class(base=Node)]
struct PlayerState {
    base: Base<Node>,
    money: i64,
    // Upgrades gained during the current contract only. Cleared on
    // apply_penalty' (caught) - permanent upgrades would need separate storage.
    upgrades: Vec<GString>,
}

#[godot_api]
impl INode for PlayerState {
    // called once when the autoload node enters the scene tree
    // starting values: no money, no upgrades yet.
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
    // Records an upgrade earned this contract e.g. called when the player picks a reward mid run
    #[func]
    fn add_upgrade(&mut self, name: GString) {
        self.upgrades.push(name);
    }

    // Current credit balance. Read by screens that show the player's money,
    // e.g. after an event node changes it.
    #[func]
    fn money(&self) -> i64 {
        self.money
    }

    // Adds credits to the balance. `amount` may be negative for a cost, e.g.
    // paying a broker during an event.
    #[func]
    fn add_credits(&mut self, amount: i64) {
        self.money += amount;
    }

    // Called when the player is caught. Deducts 'fine' from money and wipes out any upgrades earned this contract
    // Returns what was lost so the CaugthtScreen can display it:
    // {"fine": int, "lost_upgrades": Array[String] }.
    #[func]
    fn apply_penalty(&mut self, fine: i64) -> VarDictionary {
        // take ownership of the current upgrades list and empty it out
        // drain(..) removes every element and hands them to lost
        let lost: Array<GString> = self.upgrades.drain(..).collect();
        self.money -= fine;

        // build the untyped dictionary GDScript expects back
        dict! {
            "fine" => fine,
            "lost_upgrades" => &lost,
        }
    }
}

/// Godot-facing node for the draw phase.
///
/// Wraps a [`Deck`] built from the starter pack. Gameplay rules live in
/// `deck.rs`/`card.rs`/`pack.rs` as plain Rust so they stay unit testable;
/// this node just exposes them to GDScript.
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
            base,
        }
    }

    fn ready(&mut self) {
        let card_db = self
            .base()
            .get_node_as::<CardDatabase>("/root/CardDatabaseGlobal");
        let card_db = card_db.bind();

        // Every run starts from the same predefined list rather than one copy
        // of every card in the database.
        let entries = starter_deck::load_starter_deck_entries();
        let starter_cards: Vec<CardData> =
            match starter_deck::build_starter_deck(&entries, |id| card_db.get(id)) {
                Ok(cards) => cards,
                Err(missing) => {
                    godot_error!(
                        "starter_deck.ron refers to cards that do not exist in cards.ron: {}",
                        missing.join(", ")
                    );
                    Vec::new()
                }
            };

        let mut rng = rand::rng();
        self.deck = Deck::new(starter_cards, &mut rng);
    }
}

#[godot_api]
impl DrawPhase {
    /// Draws a new hand of `hand_size` randomized cards, reshuffling the
    /// discard pile into the draw pile if it runs out mid-draw. Returns the
    /// drawn cards' names for GDScript to display.
    ///
    /// Any cards still held from a previous hand go to the discard pile
    /// first, so calling this repeatedly cycles cards through the discard
    /// pile instead of quietly dropping them.
    #[func]
    fn draw_hand(&mut self) -> PackedStringArray {
        let previous_hand = std::mem::take(&mut self.hand);
        self.deck.discard(previous_hand);
        self.energy = self.max_energy;

        let mut rng = rand::rng();
        self.hand = self.deck.draw_hand(self.hand_size as usize, &mut rng);
        self.hand
            .iter()
            .map(|card| GString::from(card.name.as_str()))
            .collect()
    }

    #[func]
    fn play_card(&mut self, index: i32) -> GString {
        let index = index as usize;

        if index >= self.hand.len() {
            godot_warn!(
                "play_card: index {index} out of bounds (hand has {} cards)",
                self.hand.len()
            );
            return GString::new();
        }

        let cost = self.hand[index].cost as i32;
        if cost > self.energy {
            godot_warn!(
                "play_card: not enough energy to play '{}' (cost {cost}, have {})",
                self.hand[index].name,
                self.energy
            );
            return GString::new();
        }

        let card = self.hand.remove(index);
        let name = card.name.clone();

        self.energy -= cost;
        self.deck.discard(vec![card]);
        self.play_count += 1;

        godot_print!("Played {name} (Total plays: {})", self.play_count);
        GString::from(name.as_str())
    }

    #[func]
    fn hand_names(&self) -> PackedStringArray {
        self.hand
            .iter()
            .map(|c| GString::from(c.name.as_str()))
            .collect()
    }

    /// Everything worth knowing about the card at `index` in hand, for the
    /// detail panel: name, type, rarity, cost, noise, description and one
    /// entry per effect with an explanation of what that effect does.
    ///
    /// Returns `{ok: false}` for an index that is not in hand, so a stale
    /// selection cannot show another card's details.
    #[func]
    fn card_detail(&self, index: i32) -> VarDictionary {
        let Ok(index) = usize::try_from(index) else {
            return vdict! { "ok" => false };
        };
        let Some(card) = self.hand.get(index) else {
            return vdict! { "ok" => false };
        };

        let detail = card_text::describe(card);

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

    /// Energy costs of the current hand, in the same order as
    /// `hand_names()`/`draw_hand()`, so GDScript can disable cards it can't
    /// afford.
    #[func]
    fn hand_costs(&self) -> PackedInt32Array {
        self.hand.iter().map(|c| c.cost as i32).collect()
    }

    /// Sends the current hand to the discard pile, e.g. at end of turn.
    #[func]
    fn discard_hand(&mut self) {
        let hand = std::mem::take(&mut self.hand);
        self.deck.discard(hand);
    }

    /// Emitted after the player's turn ends and before the next one begins,
    /// carrying the turn number that just finished.
    ///
    /// This is the seam the security system's own turn hangs off (Trello card
    /// 30). Nothing listens to it yet, so the phase currently passes straight
    /// through and control returns to the player.
    #[signal]
    fn security_phase(finished_turn: i32);

    /// Ends the player's turn: every card still in hand goes to the discard
    /// pile, the security system gets its phase, then the next turn begins
    /// with refreshed energy and a freshly drawn hand.
    ///
    /// Returns the new hand's card names, so GDScript can render it directly.
    #[func]
    fn end_turn(&mut self) -> PackedStringArray {
        let finished_turn = self.turn_number;

        // The player's turn ends: nothing is carried over into the next hand.
        let remaining_hand = std::mem::take(&mut self.hand);
        self.deck.discard(remaining_hand);

        self.signals().security_phase().emit(finished_turn);

        // Control comes back to the player for a fresh turn.
        self.turn_number += 1;
        self.energy = self.max_energy;

        let mut rng = rand::rng();
        self.hand = self.deck.draw_hand(self.hand_size as usize, &mut rng);
        self.hand
            .iter()
            .map(|card| GString::from(card.name.as_str()))
            .collect()
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

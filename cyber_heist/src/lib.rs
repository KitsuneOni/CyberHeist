//! CyberHeist GDExtension library for Godot 4.
//!
//! Game logic lives in this crate; the Godot project under `../godot` handles
//! scenes, UI and presentation. Add new gameplay modules here and register them
//! as Godot classes with `#[derive(GodotClass)]`.

mod card;
mod deck;
mod pack;
mod run_state;
mod run_state_node;

use godot::prelude::*;

use deck::Deck;

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

    deck: Deck,
    hand: Vec<card::Card>,
    base: Base<Node>,
}

#[godot_api]
impl INode for DrawPhase {
    fn init(base: Base<Node>) -> Self {
        let mut rng = rand::rng();
        Self {
            hand_size: 5,
            deck: Deck::new(pack::starter_pack(), &mut rng),
            hand: Vec::new(),
            base,
        }
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

        let mut rng = rand::rng();
        self.hand = self.deck.draw_hand(self.hand_size as usize, &mut rng);
        self.hand
            .iter()
            .map(|card| GString::from(card.name.as_str()))
            .collect()
    }

    /// Sends the current hand to the discard pile, e.g. at end of turn.
    #[func]
    fn discard_hand(&mut self) {
        let hand = std::mem::take(&mut self.hand);
        self.deck.discard(hand);
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

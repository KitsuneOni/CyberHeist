//! CyberHeist GDExtension library for Godot 4.
//!
//! Game logic lives in this crate; the Godot project under `../godot` handles
//! scenes, UI and presentation. Add new gameplay modules here and register them
//! as Godot classes with `#[derive(GodotClass)]`.

mod deck;
mod card_data;
mod card_database;

use godot::prelude::*;
use card_data::CardData;
use card_database::CardDatabase;
use deck::Deck;

struct CyberHeistExtension;

#[gdextension]
unsafe impl ExtensionLibrary for CyberHeistExtension {}

/// Temporary node that proves the Rust <-> Godot bridge is wired up correctly.
///
/// Attach it to a scene (or use `godot/scenes/main.tscn`) and run the project:
/// the `_ready` message should appear in Godot's Output panel. Once real
/// gameplay classes exist this can be deleted along with `main.tscn`.
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

    #[export]
    play_count: i32,

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
            deck: Deck::new(Vec::new(), &mut  rand::rng()),
            hand: Vec::new(),
            base,
        }
    }

    fn ready(&mut self){
        let card_db = self.base().get_node_as::<CardDatabase>("/root/CardDatabaseGlobal");
        let card_db = card_db.bind();

        let starter_cards: Vec<CardData> = card_db.all().cloned().collect();

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

        if index >= self.hand.len(){
            godot_warn!(
                "Play_card: index {index} out of bounds (hand has {} cards)", self.hand.len()
            );
            return GString::new();
        }

        let card = self.hand.remove(index);
        let name = card.name.clone();

        self.deck.discard(vec![card]);
        self.play_count += 1;

        godot_print!("Played {name} (Total plays: {})", self.play_count);
        GString::from(name.as_str())
    }

    #[func]
    fn hand_names(&self) -> PackedStringArray{
        self.hand.iter().map(|c| GString::from(c.name.as_str())).collect()
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

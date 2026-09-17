//! The cards the player owns for the length of a run.
//!
//! Until now there was no such thing: `DrawPhase` rebuilt a deck from
//! `starter_deck.ron` every time the combat scene loaded, so a card picked up
//! mid-run was thrown away at the next encounter. This owns the run's cards so
//! they survive from one encounter to the next.
//!
//! Cards are held as ids rather than resolved [`CardData`], for the same
//! reason `starter_deck.rs` does: the card database is the single source of
//! truth for what an id means, and ids are what a save file would store.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md.

use crate::starter_deck::StarterDeckEntry;

/// Every card the player currently owns this run, one entry per physical card.
///
/// Duplicates are expected and meaningful: three copies of a card means three
/// entries, so the draw pile has three of them to shuffle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunDeck {
    card_ids: Vec<String>,
}

impl RunDeck {
    /// Expands the authored starter list into one entry per physical card, so
    /// a run begins owning exactly what `starter_deck.ron` describes.
    pub fn from_starter_entries(entries: &[StarterDeckEntry]) -> Self {
        let mut card_ids = Vec::new();
        for entry in entries {
            for _ in 0..entry.count {
                card_ids.push(entry.id.clone());
            }
        }
        Self { card_ids }
    }

    pub fn card_ids(&self) -> &[String] {
        &self.card_ids
    }

    pub fn len(&self) -> usize {
        self.card_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.card_ids.is_empty()
    }

    /// Adds a card to the run's deck, e.g. a reward picked after clearing an
    /// encounter. Duplicates are allowed.
    pub fn add(&mut self, card_id: &str) {
        self.card_ids.push(card_id.to_string());
    }

    /// How many copies of `card_id` the run owns.
    pub fn count_of(&self, card_id: &str) -> usize {
        self.card_ids.iter().filter(|id| *id == card_id).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, count: u8) -> StarterDeckEntry {
        StarterDeckEntry {
            id: id.to_string(),
            count,
        }
    }

    #[test]
    fn a_starter_list_expands_into_one_entry_per_physical_card() {
        let deck = RunDeck::from_starter_entries(&[entry("strike", 3), entry("vpn", 1)]);

        assert_eq!(deck.len(), 4);
        assert_eq!(deck.count_of("strike"), 3);
        assert_eq!(deck.count_of("vpn"), 1);
    }

    #[test]
    fn an_entry_with_no_copies_contributes_nothing() {
        let deck = RunDeck::from_starter_entries(&[entry("strike", 0), entry("vpn", 2)]);

        assert_eq!(deck.len(), 2);
        assert_eq!(deck.count_of("strike"), 0);
    }

    #[test]
    fn a_reward_card_is_added_to_what_the_run_already_owns() {
        let mut deck = RunDeck::from_starter_entries(&[entry("strike", 2)]);

        deck.add("wannacry");

        assert_eq!(deck.len(), 3);
        assert_eq!(deck.count_of("wannacry"), 1);
        assert_eq!(deck.count_of("strike"), 2, "existing cards are untouched");
    }

    #[test]
    fn the_same_card_can_be_picked_more_than_once() {
        let mut deck = RunDeck::default();
        deck.add("wannacry");
        deck.add("wannacry");

        assert_eq!(deck.count_of("wannacry"), 2);
        assert_eq!(deck.card_ids(), ["wannacry", "wannacry"]);
    }

    #[test]
    fn a_new_run_deck_is_empty() {
        let deck = RunDeck::default();
        assert!(deck.is_empty());
        assert_eq!(deck.len(), 0);
    }
}

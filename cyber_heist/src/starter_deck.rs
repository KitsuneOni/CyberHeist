//! The deck every run begins with.
//!
//! Covers the "start each run with a default starter deck" acceptance tests
//! from Trello:
//! 1. A new run's draw pile contains the predefined starter deck cards, ready
//!    to draw from immediately.
//! 2. Every new run starts from that same set, with nothing carried over.
//!
//! The list itself lives in `godot/data/starter_deck.ron` and refers to cards
//! by id from `cards.ron`, so the starter deck can only ever be built out of
//! cards that actually exist. Parsing and expansion are plain Rust so they
//! stay unit testable per docs/rust-godot-setup.md.

use godot::classes::FileAccess;
use serde::Deserialize;

use crate::card_data::CardData;

/// One line of the starter deck list: a card id and how many copies of it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StarterDeckEntry {
    pub id: String,
    pub count: u8,
}

pub fn parse_starter_deck(text: &str) -> Vec<StarterDeckEntry> {
    ron::from_str(text).expect("failed to parse starter_deck.ron - check syntax")
}

pub fn load_starter_deck_entries() -> Vec<StarterDeckEntry> {
    let path = "res://data/starter_deck.ron";

    let file = FileAccess::open(path, godot::classes::file_access::ModeFlags::READ)
        .expect("Failed to open starter_deck.ron");

    let text = file.get_as_text().to_string();
    parse_starter_deck(&text)
}

/// Expands the entries into the actual cards a run starts with.
///
/// An id that is not in the card database is returned as an error rather than
/// skipped, so a typo shows up immediately instead of quietly shrinking the
/// deck.
pub fn build_starter_deck<'a>(
    entries: &[StarterDeckEntry],
    lookup: impl Fn(&str) -> Option<&'a CardData>,
) -> Result<Vec<CardData>, Vec<String>> {
    let mut deck = Vec::new();
    let mut missing = Vec::new();

    for entry in entries {
        match lookup(&entry.id) {
            Some(card) => {
                for _ in 0..entry.count {
                    deck.push(card.clone());
                }
            }
            None => missing.push(entry.id.clone()),
        }
    }

    if missing.is_empty() {
        Ok(deck)
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_data::{CardType, Rarity};
    use crate::card_database::parse_cards;
    use std::collections::HashMap;

    const SAMPLE: &str = r#"
    [
        (id: "strike", count: 3),
        (id: "shield", count: 1),
    ]
    "#;

    fn card(id: &str) -> CardData {
        CardData {
            id: id.to_string(),
            name: id.to_string(),
            description: "test card".to_string(),
            cost: 1,
            noise_generated: 0,
            card_type: CardType::Attack,
            rarity: Rarity::Common,
            keywords: Vec::new(),
        }
    }

    fn test_database(ids: &[&str]) -> HashMap<String, CardData> {
        ids.iter().map(|id| (id.to_string(), card(id))).collect()
    }

    #[test]
    fn parses_entries_with_their_counts() {
        let entries = parse_starter_deck(SAMPLE);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "strike");
        assert_eq!(entries[0].count, 3);
        assert_eq!(entries[1].count, 1);
    }

    /// Acceptance scenario 1: the draw pile holds the predefined cards.
    #[test]
    fn building_expands_each_entry_into_that_many_copies() {
        let database = test_database(&["strike", "shield"]);
        let entries = parse_starter_deck(SAMPLE);

        let deck = build_starter_deck(&entries, |id| database.get(id)).expect("all ids exist");

        assert_eq!(deck.len(), 4, "3 strikes + 1 shield");
        let strikes = deck.iter().filter(|c| c.id == "strike").count();
        let shields = deck.iter().filter(|c| c.id == "shield").count();
        assert_eq!(strikes, 3);
        assert_eq!(shields, 1);
    }

    #[test]
    fn an_unknown_card_id_is_reported_rather_than_skipped() {
        let database = test_database(&["strike"]);
        let entries = parse_starter_deck(SAMPLE);

        let result = build_starter_deck(&entries, |id| database.get(id));

        assert_eq!(result.unwrap_err(), vec!["shield".to_string()]);
    }

    #[test]
    fn a_zero_count_entry_contributes_nothing() {
        let database = test_database(&["strike"]);
        let entries = vec![StarterDeckEntry {
            id: "strike".to_string(),
            count: 0,
        }];

        let deck = build_starter_deck(&entries, |id| database.get(id)).expect("id exists");

        assert!(deck.is_empty());
    }

    /// Acceptance scenario 2: every run starts from the same set, so building
    /// it twice must produce the same cards.
    #[test]
    fn the_starter_deck_is_identical_every_time_it_is_built() {
        let database = test_database(&["strike", "shield"]);
        let entries = parse_starter_deck(SAMPLE);

        let first = build_starter_deck(&entries, |id| database.get(id)).expect("all ids exist");
        let second = build_starter_deck(&entries, |id| database.get(id)).expect("all ids exist");

        let first_ids: Vec<&str> = first.iter().map(|c| c.id.as_str()).collect();
        let second_ids: Vec<&str> = second.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(first_ids, second_ids);
    }

    /// The shipped starter deck must be buildable purely from the cards that
    /// already exist in cards.ron, so it cannot drift from the real card set.
    #[test]
    fn the_real_starter_deck_uses_only_real_cards() {
        let deck_text = std::fs::read_to_string("../godot/data/starter_deck.ron")
            .expect("could not find starter_deck.ron - check the relative path");
        let cards_text = std::fs::read_to_string("../godot/data/cards.ron")
            .expect("could not find cards.ron - check the relative path");

        let entries = parse_starter_deck(&deck_text);
        let database = parse_cards(&cards_text);

        assert!(!entries.is_empty(), "the starter deck is empty");

        let deck = build_starter_deck(&entries, |id| database.get(id))
            .expect("every starter deck id must exist in cards.ron");

        assert!(
            deck.len() >= 5,
            "a starter deck of {} cards is too small to draw a hand from",
            deck.len()
        );
    }
}

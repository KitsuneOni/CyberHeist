//! Draw pile / discard pile / hand for a player's deck.
//!
//! Covers the "As a player I want to draw a hand of randomized cards per
//! turn" acceptance tests from Trello:
//! 1. Drawing a hand deals randomized cards from the draw pile.
//! 2. If the draw pile runs out mid-draw, the discard pile is shuffled into
//!    a new draw pile and the remainder of the hand is completed from it.
//!
//! Kept as a plain Rust struct (no Godot types) so it can be unit tested
//! without running the engine, per docs/rust-godot-setup.md.

use rand::Rng;
use rand::seq::SliceRandom;

use crate::card_data::CardData;

pub struct Deck {
    draw_pile: Vec<CardData>,
    discard_pile: Vec<CardData>,
}

impl Deck {
    /// Builds a deck from `cards` and shuffles the draw pile.
    pub fn new(cards: Vec<CardData>, rng: &mut impl Rng) -> Self {
        let mut draw_pile = cards;
        draw_pile.shuffle(rng);
        Self {
            draw_pile,
            discard_pile: Vec::new(),
        }
    }

    pub fn draw_pile_len(&self) -> usize {
        self.draw_pile.len()
    }

    pub fn discard_pile_len(&self) -> usize {
        self.discard_pile.len()
    }

    /// Moves `cards` into the discard pile, e.g. at end of turn.
    pub fn discard(&mut self, cards: Vec<CardData>) {
        self.discard_pile.extend(cards);
    }

    /// Draws a hand of up to `hand_size` randomized cards.
    ///
    /// If the draw pile empties partway through, the discard pile is
    /// shuffled into a fresh draw pile and drawing continues from there. If
    /// both piles run out entirely, the hand comes back short rather than
    /// panicking.
    pub fn draw_hand(&mut self, hand_size: usize, rng: &mut impl Rng) -> Vec<CardData> {
        let mut hand = Vec::with_capacity(hand_size);

        while hand.len() < hand_size {
            match self.draw_pile.pop() {
                Some(card) => hand.push(card),
                None if !self.discard_pile.is_empty() => {
                    self.draw_pile.append(&mut self.discard_pile);
                    self.draw_pile.shuffle(rng);
                }
                None => break,
            }
        }

        hand
    }
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardType, Rarity, TargetType};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn dummy_card(id: &str) -> Card {
        Card::new(
            id,
            id,
            1,
            Rarity::Common,
            TargetType::Enemy,
            CardType::Attack,
            "test card",
        )
    }

    fn dummy_cards(ids: &[&str]) -> Vec<Card> {
        ids.iter().map(|id| dummy_card(id)).collect()
    }
    
    #[test]
    fn draw_hand_deals_requested_number_of_cards() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut deck = Deck::new(dummy_cards(&["a", "b", "c", "d", "e"]), &mut rng);

        let hand = deck.draw_hand(3, &mut rng);

        assert_eq!(hand.len(), 3);
        assert_eq!(deck.draw_pile_len(), 2);
    }

    #[test]
    fn draw_hand_draws_different_orders_across_shuffles() {
        let ids: Vec<String> = (0..20).map(|i| format!("card_{i}")).collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();

        let mut rng_a = StdRng::seed_from_u64(1);
        let mut deck_a = Deck::new(dummy_cards(&id_refs), &mut rng_a);
        let hand_a: Vec<String> = deck_a
            .draw_hand(20, &mut rng_a)
            .into_iter()
            .map(|c| c.id)
            .collect();

        let mut rng_b = StdRng::seed_from_u64(2);
        let mut deck_b = Deck::new(dummy_cards(&id_refs), &mut rng_b);
        let hand_b: Vec<String> = deck_b
            .draw_hand(20, &mut rng_b)
            .into_iter()
            .map(|c| c.id)
            .collect();

        assert_ne!(
            hand_a, hand_b,
            "different seeds should shuffle into different orders"
        );
    }

    #[test]
    fn draw_hand_reshuffles_discard_pile_when_draw_pile_runs_out() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut deck = Deck::new(dummy_cards(&["a", "b"]), &mut rng);
        deck.discard(dummy_cards(&["c", "d", "e"]));

        let hand = deck.draw_hand(5, &mut rng);

        assert_eq!(
            hand.len(),
            5,
            "should draw remaining cards, reshuffle discard, then finish the hand"
        );
        assert_eq!(deck.draw_pile_len(), 0);
        assert_eq!(deck.discard_pile_len(), 0);

        let mut ids: Vec<&str> = hand.iter().map(|c| c.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn draw_hand_of_seven_with_three_in_draw_pile_draws_those_three_then_tops_up_from_discard() {
        let mut rng = StdRng::seed_from_u64(99);
        let mut deck = Deck::new(dummy_cards(&["a", "b", "c"]), &mut rng);
        deck.discard(dummy_cards(&["d", "e", "f", "g", "h"]));

        let hand = deck.draw_hand(7, &mut rng);

        assert_eq!(
            hand.len(),
            7,
            "3 from the draw pile + 4 topped up after reshuffling the discard pile"
        );
        assert_eq!(
            deck.draw_pile_len(),
            1,
            "reshuffled discard had 5 cards, only 4 of which were needed"
        );
        assert_eq!(deck.discard_pile_len(), 0);

        let ids: std::collections::HashSet<&str> = hand.iter().map(|c| c.id.as_str()).collect();
        assert!(
            ["a", "b", "c"].iter().all(|id| ids.contains(id)),
            "all 3 original draw pile cards must be in the hand: {ids:?}"
        );
        let topped_up_from_discard = ids
            .iter()
            .filter(|id| ["d", "e", "f", "g", "h"].contains(id))
            .count();
        assert_eq!(
            topped_up_from_discard, 4,
            "exactly 4 more cards topped up from the reshuffled discard pile"
        );
    }

    #[test]
    fn draw_hand_comes_back_short_when_both_piles_are_exhausted() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut deck = Deck::new(dummy_cards(&["a"]), &mut rng);

        let hand = deck.draw_hand(5, &mut rng);

        assert_eq!(hand.len(), 1);
        assert_eq!(deck.draw_pile_len(), 0);
        assert_eq!(deck.discard_pile_len(), 0);
    }
        
}
    */

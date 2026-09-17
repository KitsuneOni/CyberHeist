//! Choosing which cards to offer the player after they clear an encounter.
//!
//! Covers the first acceptance test of Trello card 53: clearing an encounter
//! presents a selection of cards to choose from. Which cards appear is decided
//! here; what taking one does belongs to [`crate::run_deck::RunDeck`].
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md.

use rand::Rng;
use rand::seq::SliceRandom;

/// Picks up to `count` distinct cards from `pool` to offer as a reward.
///
/// Distinct within a single offer, so the player never chooses between three
/// copies of the same card. A pool smaller than `count` yields a shorter offer
/// rather than padding it with repeats.
pub fn offer_from_pool(pool: &[String], count: usize, rng: &mut impl Rng) -> Vec<String> {
    let mut candidates: Vec<String> = pool.to_vec();
    candidates.sort();
    candidates.dedup();
    candidates.shuffle(rng);
    candidates.truncate(count);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashSet;

    fn pool(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn an_offer_has_the_requested_number_of_cards() {
        let mut rng = StdRng::seed_from_u64(1);
        let pool = pool(&["a", "b", "c", "d", "e"]);

        let offer = offer_from_pool(&pool, 3, &mut rng);

        assert_eq!(offer.len(), 3);
    }

    #[test]
    fn every_card_in_an_offer_comes_from_the_pool() {
        let mut rng = StdRng::seed_from_u64(2);
        let pool = pool(&["a", "b", "c", "d", "e"]);

        let offer = offer_from_pool(&pool, 3, &mut rng);

        for id in &offer {
            assert!(pool.contains(id), "{id} is not in the pool");
        }
    }

    #[test]
    fn an_offer_never_repeats_the_same_card() {
        let mut rng = StdRng::seed_from_u64(3);
        // A pool where one card is far more common than the others.
        let pool = pool(&["a", "a", "a", "a", "a", "b", "c"]);

        for _ in 0..50 {
            let offer = offer_from_pool(&pool, 3, &mut rng);
            let unique: HashSet<&String> = offer.iter().collect();
            assert_eq!(
                unique.len(),
                offer.len(),
                "offer repeated a card: {offer:?}"
            );
        }
    }

    #[test]
    fn a_pool_smaller_than_the_offer_yields_a_shorter_offer() {
        let mut rng = StdRng::seed_from_u64(4);
        let pool = pool(&["a", "b"]);

        let offer = offer_from_pool(&pool, 3, &mut rng);

        assert_eq!(offer.len(), 2, "two distinct cards cannot fill three slots");
    }

    #[test]
    fn an_empty_pool_offers_nothing_rather_than_panicking() {
        let mut rng = StdRng::seed_from_u64(5);

        let offer = offer_from_pool(&[], 3, &mut rng);

        assert!(offer.is_empty());
    }

    #[test]
    fn offers_vary_between_encounters() {
        let pool: Vec<String> = (0..30).map(|i| format!("card_{i}")).collect();

        let mut rng_a = StdRng::seed_from_u64(6);
        let mut rng_b = StdRng::seed_from_u64(7);

        let offer_a = offer_from_pool(&pool, 3, &mut rng_a);
        let offer_b = offer_from_pool(&pool, 3, &mut rng_b);

        assert_ne!(
            offer_a, offer_b,
            "a reward screen that always offers the same three cards is not a choice"
        );
    }
}

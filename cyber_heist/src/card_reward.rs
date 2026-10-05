//! Choosing which cards to offer the player after they clear an encounter.
//!
//! Covers the first acceptance test of Trello card 53: clearing an encounter
//! presents a selection of cards to choose from. Which cards appear is decided
//! here; what taking one does belongs to [`crate::run_deck::RunDeck`].
//!
//! Also covers the reward half of Trello card E1: an elite's offer always
//! includes at least one Rare, via [`offer_with_guarantee`].
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md.

use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};

use crate::card_data::Rarity;

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

/// Like [`offer_from_pool`], but when `guaranteed` is set the offer includes
/// at least one card of that rarity or better.
///
/// One qualifying card is picked first and the rest of the offer is filled
/// from everything else, so the offer stays distinct. Where the guaranteed
/// card lands is shuffled too, so it is not always the first one shown. A pool
/// with no qualifying card falls back to an ordinary offer rather than coming
/// up short.
pub fn offer_with_guarantee(
    pool: &[(String, Rarity)],
    count: usize,
    guaranteed: Option<Rarity>,
    rng: &mut impl Rng,
) -> Vec<String> {
    let ids: Vec<String> = pool.iter().map(|(id, _)| id.clone()).collect();
    let Some(minimum) = guaranteed else {
        return offer_from_pool(&ids, count, rng);
    };
    if count == 0 {
        return Vec::new();
    }

    let mut qualifying: Vec<&String> = pool
        .iter()
        .filter(|(_, rarity)| *rarity >= minimum)
        .map(|(id, _)| id)
        .collect();
    qualifying.sort();
    qualifying.dedup();
    let Some(&anchor) = qualifying.choose(rng) else {
        return offer_from_pool(&ids, count, rng);
    };

    let others: Vec<String> = ids.into_iter().filter(|id| id != anchor).collect();
    let mut offer = offer_from_pool(&others, count - 1, rng);
    offer.push(anchor.clone());
    offer.shuffle(rng);
    offer
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

    fn rated(cards: &[(&str, Rarity)]) -> Vec<(String, Rarity)> {
        cards
            .iter()
            .map(|(id, rarity)| (id.to_string(), *rarity))
            .collect()
    }

    fn mixed_pool() -> Vec<(String, Rarity)> {
        rated(&[
            ("a", Rarity::Common),
            ("b", Rarity::Common),
            ("c", Rarity::Common),
            ("d", Rarity::Common),
            ("e", Rarity::Common),
            ("r1", Rarity::Rare),
            ("r2", Rarity::Rare),
        ])
    }

    /// Elite card scenario 2: the offer includes at least one rare card,
    /// every time rather than most of the time.
    #[test]
    fn a_guaranteed_offer_always_includes_a_rare() {
        let pool = mixed_pool();
        let rares = ["r1", "r2"];
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let offer = offer_with_guarantee(&pool, 3, Some(Rarity::Rare), &mut rng);

            assert_eq!(offer.len(), 3, "seed {seed}");
            assert!(
                offer.iter().any(|id| rares.contains(&id.as_str())),
                "seed {seed}: no rare in {offer:?}"
            );
            let unique: HashSet<&String> = offer.iter().collect();
            assert_eq!(unique.len(), offer.len(), "seed {seed}: repeated {offer:?}");
        }
    }

    #[test]
    fn a_better_rarity_than_the_one_guaranteed_counts() {
        let pool = rated(&[("a", Rarity::Common), ("l", Rarity::Legendary)]);
        let mut rng = StdRng::seed_from_u64(8);

        let offer = offer_with_guarantee(&pool, 1, Some(Rarity::Rare), &mut rng);

        assert_eq!(offer, vec!["l".to_string()]);
    }

    #[test]
    fn the_guaranteed_card_is_not_always_shown_first() {
        let pool = mixed_pool();
        let positions: HashSet<usize> = (0..100)
            .map(|seed| {
                let mut rng = StdRng::seed_from_u64(seed);
                let offer = offer_with_guarantee(&pool, 3, Some(Rarity::Rare), &mut rng);
                offer
                    .iter()
                    .position(|id| id.starts_with('r'))
                    .expect("a rare is offered")
            })
            .collect();

        assert!(positions.len() > 1, "the rare always sat at {positions:?}");
    }

    #[test]
    fn a_pool_without_the_guaranteed_rarity_falls_back_to_an_ordinary_offer() {
        let pool = rated(&[
            ("a", Rarity::Common),
            ("b", Rarity::Common),
            ("c", Rarity::Common),
        ]);
        let mut rng = StdRng::seed_from_u64(9);

        let offer = offer_with_guarantee(&pool, 3, Some(Rarity::Rare), &mut rng);

        assert_eq!(offer.len(), 3, "still a full offer, just without a rare");
    }

    #[test]
    fn no_guarantee_is_exactly_an_ordinary_offer() {
        let pool = mixed_pool();
        let ids: Vec<String> = pool.iter().map(|(id, _)| id.clone()).collect();

        let guaranteed = offer_with_guarantee(&pool, 3, None, &mut StdRng::seed_from_u64(10));
        let ordinary = offer_from_pool(&ids, 3, &mut StdRng::seed_from_u64(10));

        assert_eq!(guaranteed, ordinary);
    }

    #[test]
    fn a_guaranteed_offer_of_nothing_is_empty() {
        let mut rng = StdRng::seed_from_u64(11);
        assert!(offer_with_guarantee(&mixed_pool(), 0, Some(Rarity::Rare), &mut rng).is_empty());
        assert!(offer_with_guarantee(&[], 3, Some(Rarity::Rare), &mut rng).is_empty());
    }
}

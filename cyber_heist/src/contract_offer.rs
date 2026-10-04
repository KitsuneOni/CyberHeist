//! Contract offers: the jobs a run can choose between before it starts.
//!
//! Plain Rust, so the rules for what is on offer can be tested without Godot.
//! `run_state.rs` owns the offers for a run and builds the contract map from
//! whichever one the player accepts; the selection screen only reads them.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::sentry::{BOSS_NAMES, Sentry};

    fn offers_for(seed: u64) -> Vec<ContractOffer> {
        generate_offers(&mut StdRng::seed_from_u64(seed))
    }

    /// Acceptance scenario 1: a new run is offered three contracts, and each
    /// one says what it is, who it hits, what it pays and how long it runs.
    #[test]
    fn three_offers_each_have_a_name_target_reward_and_zone_count() {
        for seed in 0..50 {
            let offers = offers_for(seed);

            assert_eq!(offers.len(), 3, "seed {seed} should offer three contracts");
            for offer in &offers {
                assert!(
                    !offer.name.trim().is_empty(),
                    "seed {seed}: an offer has no name"
                );
                assert!(
                    !offer.target.trim().is_empty(),
                    "seed {seed}: {} has no target",
                    offer.name
                );
                assert!(
                    offer.credit_reward > 0,
                    "seed {seed}: {} pays nothing",
                    offer.name
                );
                assert!(
                    offer.shape.zone_count >= 1,
                    "seed {seed}: {} has no zones",
                    offer.name
                );
            }
        }
    }

    /// Two offers called the same thing, or hitting the same target, would
    /// read as a duplicate rather than a choice.
    #[test]
    fn no_two_offers_in_a_set_share_a_name_or_a_target() {
        for seed in 0..50 {
            let offers = offers_for(seed);

            let names: BTreeSet<&str> = offers.iter().map(|offer| offer.name.as_str()).collect();
            let targets: BTreeSet<&str> =
                offers.iter().map(|offer| offer.target.as_str()).collect();

            assert_eq!(names.len(), offers.len(), "seed {seed} repeats a name");
            assert_eq!(targets.len(), offers.len(), "seed {seed} repeats a target");
        }
    }

    /// The choice has to be about something: every offer in a set runs a
    /// different length, and they are listed shortest first.
    #[test]
    fn the_offers_in_a_set_run_different_lengths_listed_shortest_first() {
        for seed in 0..50 {
            let lengths: Vec<usize> = offers_for(seed)
                .iter()
                .map(ContractOffer::encounter_count)
                .collect();

            let mut ascending = lengths.clone();
            ascending.sort_unstable();
            ascending.dedup();
            assert_eq!(
                lengths, ascending,
                "seed {seed}: lengths {lengths:?} are repeated or out of order"
            );
        }
    }

    /// A longer contract means more encounters to survive, so it has to be
    /// worth more, or the short one is always the right answer.
    #[test]
    fn a_longer_contract_pays_more_credits() {
        let shapes = offerable_shapes();
        for longer in &shapes {
            for shorter in &shapes {
                if longer.length() <= shorter.length() {
                    continue;
                }
                assert!(
                    credit_reward_for(*longer) > credit_reward_for(*shorter),
                    "{longer:?} pays {} but the shorter {shorter:?} pays {}",
                    credit_reward_for(*longer),
                    credit_reward_for(*shorter)
                );
            }
        }
    }

    /// Every zone ends in a boss, so an offer must never run deeper than the
    /// bosses that have actually been written.
    #[test]
    fn every_offered_zone_is_guarded_by_an_authored_boss() {
        for seed in 0..50 {
            for offer in offers_for(seed) {
                for zone in 0..offer.shape.zone_count {
                    let boss = Sentry::boss_for_zone(zone);
                    assert!(
                        BOSS_NAMES.contains(&boss.name()),
                        "seed {seed}: {} reaches zone {} where only the fallback {} stands guard",
                        offer.name,
                        zone + 1,
                        boss.name()
                    );
                }
            }
        }
    }

    /// The seed travels with the offer, so accepting the same offer always
    /// lays out the same contract.
    #[test]
    fn an_offer_always_builds_the_same_map() {
        for offer in offers_for(11) {
            assert_eq!(
                offer.build_map(),
                offer.build_map(),
                "{} built two different maps",
                offer.name
            );
        }
    }

    /// The map an offer builds is the contract it advertised: as many zones,
    /// and as many encounters on a route through it.
    #[test]
    fn an_offers_map_has_the_zones_and_length_it_advertised() {
        for seed in 0..25 {
            for offer in offers_for(seed) {
                let map = offer.build_map();

                assert_eq!(
                    map.zone_progress().total_zones,
                    offer.shape.zone_count,
                    "seed {seed}: {} has the wrong number of zones",
                    offer.name
                );
                assert_eq!(
                    map.progress().total,
                    offer.encounter_count(),
                    "seed {seed}: {} has the wrong number of encounters",
                    offer.name
                );
            }
        }
    }

    #[test]
    fn the_same_seed_offers_the_same_contracts() {
        assert_eq!(offers_for(7), offers_for(7));
    }

    #[test]
    fn different_seeds_offer_different_contracts() {
        assert_ne!(
            offers_for(1),
            offers_for(2),
            "two runs should not be offered the same jobs"
        );
    }

    /// The pools have to be big enough to fill a set without repeating
    /// themselves, or the distinctness above could not hold.
    #[test]
    fn the_authored_pools_can_fill_a_set_without_repeats() {
        let names: BTreeSet<&str> = CONTRACT_NAMES.iter().copied().collect();
        let targets: BTreeSet<&str> = CONTRACT_TARGETS.iter().copied().collect();
        let lengths: BTreeSet<usize> = offerable_shapes()
            .iter()
            .map(ContractShape::length)
            .collect();

        assert_eq!(
            names.len(),
            CONTRACT_NAMES.len(),
            "a contract name is listed twice"
        );
        assert_eq!(
            targets.len(),
            CONTRACT_TARGETS.len(),
            "a target is listed twice"
        );
        assert!(names.len() >= OFFER_COUNT);
        assert!(targets.len() >= OFFER_COUNT);
        assert!(
            lengths.len() >= OFFER_COUNT,
            "there are not enough different contract lengths to offer"
        );
    }
}

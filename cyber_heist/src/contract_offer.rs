//! Contract offers: the jobs a run can choose between before it starts.
//!
//! Plain Rust, so the rules for what is on offer can be tested without Godot.
//! `run_state.rs` owns the offers for a run and builds the contract map from
//! whichever one the player accepts; the selection screen only reads them.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::contract_map::{ContractMap, ContractShape};
use crate::sentry::BOSS_NAMES;

/// How many contracts the selection screen puts in front of the player.
pub const OFFER_COUNT: usize = 3;

/// The shallowest and deepest contracts on offer, in zones. Every zone ends in
/// a boss, so the deepest is capped at the bosses that have been written.
const MIN_ZONE_COUNT: usize = 1;
const MAX_ZONE_COUNT: usize = BOSS_NAMES.len();

/// Columns of ordinary encounters a zone can have before its boss.
const ZONE_LENGTHS: [usize; 2] = [2, 3];

/// What each encounter on a route through the contract is worth, bosses
/// included, plus the extra each boss is worth for being one. Together they
/// make a longer contract pay more than a shorter one, which is what turns the
/// selection into a trade between risk and reward.
const CREDITS_PER_ENCOUNTER: i64 = 30;
const CREDITS_PER_BOSS: i64 = 75;

/// Job names. Each set of offers draws without repeats, so this needs at
/// least `OFFER_COUNT` entries.
const CONTRACT_NAMES: [&str; 10] = [
    "Glass Hammer",
    "Midnight Ledger",
    "Silent Relay",
    "Paper Ghost",
    "Cold Open",
    "Black Static",
    "Neon Requiem",
    "Low Orbit",
    "Dead Drop",
    "Iron Lullaby",
];

/// The organisations a job is aimed at. Drawn without repeats, like the names.
const CONTRACT_TARGETS: [&str; 10] = [
    "Halcyon Mutual Bank",
    "Orbis Dynamics",
    "Kestrel Biotech",
    "Nightingale Telecom",
    "Meridian Data Vault",
    "Paragon Insurance Group",
    "Apex Freight Logistics",
    "Sable Defence Systems",
    "Lumen Health Records",
    "Vireo Casino Holdings",
];

/// One job the player can take on: what it is called, who it hits, what it
/// pays and how the contract map is laid out if it is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractOffer {
    pub name: String,
    /// The organisation being broken into.
    pub target: String,
    /// Credits paid for finishing the contract. Signed, like every other
    /// credit amount in the game.
    pub credit_reward: i64,
    /// Zones and their length, which is what the map is generated from.
    pub shape: ContractShape,
    /// Seeds the map generator, so the same offer always lays out the same
    /// contract however many times it is built.
    pub map_seed: u64,
}

impl ContractOffer {
    fn new(name: String, target: String, shape: ContractShape, map_seed: u64) -> Self {
        Self {
            name,
            target,
            credit_reward: credit_reward_for(shape),
            shape,
            map_seed,
        }
    }

    /// Encounters on a route through the contract, bosses included.
    pub fn encounter_count(&self) -> usize {
        encounters_on_route(self.shape)
    }

    /// The contract map this offer describes. Deterministic: building the
    /// same offer twice gives the same map.
    pub fn build_map(&self) -> ContractMap {
        let mut rng = StdRng::seed_from_u64(self.map_seed);
        ContractMap::generate(self.shape, &mut rng)
    }
}

/// Rolls the contracts a new run chooses between.
///
/// Every offer in a set has a different shape, and no two shapes on offer run
/// the same length, so the offers always differ in how long they are and what
/// they pay. They are listed shortest first, so the selection reads as a ladder
/// from a quick job to a long haul. Names and targets are never repeated
/// within a set.
pub fn generate_offers(rng: &mut impl Rng) -> Vec<ContractOffer> {
    let mut shapes = offerable_shapes();
    shapes.shuffle(rng);
    shapes.truncate(OFFER_COUNT);
    shapes.sort_by_key(ContractShape::length);

    let names = pick_distinct(&CONTRACT_NAMES, OFFER_COUNT, rng);
    let targets = pick_distinct(&CONTRACT_TARGETS, OFFER_COUNT, rng);

    let mut offers = Vec::with_capacity(OFFER_COUNT);
    for ((shape, name), target) in shapes.into_iter().zip(names).zip(targets) {
        offers.push(ContractOffer::new(name, target, shape, rng.random()));
    }
    offers
}

/// Every contract shape an offer can have: each supported zone count with each
/// zone length. Route width is left at the default for all of them.
fn offerable_shapes() -> Vec<ContractShape> {
    let mut shapes = Vec::new();
    for zone_count in MIN_ZONE_COUNT..=MAX_ZONE_COUNT {
        for zone_length in ZONE_LENGTHS {
            shapes.push(ContractShape {
                zone_count,
                zone_length,
                ..ContractShape::default()
            });
        }
    }
    shapes
}

/// Encounters a player plays through on a contract of this shape: one per
/// column after the entry, whichever branches they take.
fn encounters_on_route(shape: ContractShape) -> usize {
    shape.length() - 1
}

/// What finishing a contract of this shape pays.
fn credit_reward_for(shape: ContractShape) -> i64 {
    let encounters = encounters_on_route(shape) as i64;
    let bosses = shape.zone_count as i64;
    encounters * CREDITS_PER_ENCOUNTER + bosses * CREDITS_PER_BOSS
}

/// `count` different entries from `pool`, in random order.
fn pick_distinct(pool: &[&str], count: usize, rng: &mut impl Rng) -> Vec<String> {
    let mut shuffled = pool.to_vec();
    shuffled.shuffle(rng);
    shuffled.into_iter().take(count).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::sentry::Sentry;

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

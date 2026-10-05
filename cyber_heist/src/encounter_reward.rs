//! What clearing an encounter pays out, by the kind of encounter it was.
//!
//! Covers the "elite rewards are better" acceptance test from Trello card E1:
//! beating an elite pays more credits than an ordinary combat in the same
//! zone, and its card reward always includes at least one Rare. Bosses are at
//! least as hard as elites, so they pay at least as well.
//!
//! Which cards end up in the offer is `card_reward`'s job; this only says what
//! the offer has to guarantee.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md.

use crate::card_data::Rarity;
use crate::contract_map::EncounterType;

/// Credits for clearing an ordinary combat in the first zone, and how much
/// more each zone deeper pays.
const COMBAT_CREDITS: i64 = 25;
const COMBAT_CREDITS_PER_ZONE: i64 = 15;

/// Elites and bosses pay a multiple of the combat payout in the same zone, so
/// the gap holds however deep the contract runs.
const ELITE_MULTIPLIER: i64 = 2;
const BOSS_MULTIPLIER: i64 = 3;

/// Credits paid for clearing an encounter of `encounter_type` in `zone`.
///
/// Only fights pay out here. Events and shops settle their own credits, and
/// the entry node is not an encounter at all, so they pay nothing.
pub fn credits_for(encounter_type: EncounterType, zone: usize) -> i64 {
    let zone = i64::try_from(zone).unwrap_or(i64::MAX);
    let combat = COMBAT_CREDITS.saturating_add(COMBAT_CREDITS_PER_ZONE.saturating_mul(zone));

    match encounter_type {
        EncounterType::Combat => combat,
        EncounterType::Elite => combat.saturating_mul(ELITE_MULTIPLIER),
        EncounterType::Boss => combat.saturating_mul(BOSS_MULTIPLIER),
        EncounterType::Entry | EncounterType::Event | EncounterType::Shop => 0,
    }
}

/// The rarity a card reward for `encounter_type` must include at least one
/// of, or `None` when an ordinary offer will do.
pub fn guaranteed_rarity(encounter_type: EncounterType) -> Option<Rarity> {
    match encounter_type {
        EncounterType::Elite | EncounterType::Boss => Some(Rarity::Rare),
        EncounterType::Entry
        | EncounterType::Combat
        | EncounterType::Event
        | EncounterType::Shop => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Elite card scenario 2: more credits than a standard combat.
    #[test]
    fn an_elite_pays_more_than_a_combat_in_every_zone() {
        for zone in 0..6 {
            let combat = credits_for(EncounterType::Combat, zone);
            let elite = credits_for(EncounterType::Elite, zone);
            assert!(combat > 0, "zone {zone}: a combat pays something");
            assert!(
                elite > combat,
                "zone {zone}: elite {elite} against {combat}"
            );
        }
    }

    #[test]
    fn a_boss_pays_at_least_as_well_as_an_elite() {
        for zone in 0..6 {
            assert!(
                credits_for(EncounterType::Boss, zone) >= credits_for(EncounterType::Elite, zone)
            );
        }
    }

    #[test]
    fn deeper_zones_pay_more() {
        for zone in 1..6 {
            for kind in [
                EncounterType::Combat,
                EncounterType::Elite,
                EncounterType::Boss,
            ] {
                assert!(credits_for(kind, zone) > credits_for(kind, zone - 1));
            }
        }
    }

    #[test]
    fn the_first_zone_pays_the_authored_amounts() {
        assert_eq!(credits_for(EncounterType::Combat, 0), 25);
        assert_eq!(credits_for(EncounterType::Elite, 0), 50);
        assert_eq!(credits_for(EncounterType::Boss, 0), 75);
    }

    #[test]
    fn nodes_that_are_not_fights_pay_nothing_here() {
        for kind in [
            EncounterType::Entry,
            EncounterType::Event,
            EncounterType::Shop,
        ] {
            assert_eq!(credits_for(kind, 0), 0, "{kind:?}");
            assert_eq!(guaranteed_rarity(kind), None, "{kind:?}");
        }
    }

    /// Elite card scenario 2: at least one rare card.
    #[test]
    fn an_elite_reward_guarantees_a_rare_and_a_combat_does_not() {
        assert_eq!(guaranteed_rarity(EncounterType::Elite), Some(Rarity::Rare));
        assert_eq!(guaranteed_rarity(EncounterType::Boss), Some(Rarity::Rare));
        assert_eq!(guaranteed_rarity(EncounterType::Combat), None);
    }

    #[test]
    fn a_zone_too_deep_to_count_does_not_overflow() {
        assert!(credits_for(EncounterType::Boss, usize::MAX) > 0);
    }
}

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::card_data::Rarity;

const POINTS_PER_RUN: u32 = 1;

const CREDITS_PER_BONUS_POINT: i64 = 500;

pub const RELIC_PRICE: u32 = 1;
pub const SHOP_ENTRY_MIN_POINTS: u32 = 1;
pub const CARDS_UNLOCKED_PER_WIN: usize = 3;

pub fn starts_locked(rarity: Rarity) -> bool {
    rarity >= Rarity::Rare
}

const EXTRA_ENERGY: i32 = 1;
const EXTRA_CARDS_DRAWN: i32 = 1;
const STARTING_CORRUPTING_BOOST: i32 = 1;
const CARDS_UPGRADED_AT_RUN_START: usize = 5;
const STARTING_MIGHT: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Relic {
    ExtraEnergy,
    ExtraDraw,
    CorruptingStart,
    DeckUpgrade,
    Might,
}

impl Relic {
    pub const ALL: [Relic; 5] = [
        Relic::ExtraEnergy,
        Relic::ExtraDraw,
        Relic::CorruptingStart,
        Relic::DeckUpgrade,
        Relic::Might,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Relic::ExtraEnergy => "extra_energy",
            Relic::ExtraDraw => "extra_draw",
            Relic::CorruptingStart => "corrupting_start",
            Relic::DeckUpgrade => "deck_upgrade",
            Relic::Might => "might",
        }
    }

    pub fn from_id(id: &str) -> Option<Relic> {
        Relic::ALL.into_iter().find(|relic| relic.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Relic::ExtraEnergy => "Overclocked Core",
            Relic::ExtraDraw => "Neural Cache",
            Relic::CorruptingStart => "Seeded Payload",
            Relic::DeckUpgrade => "Forged Toolkit",
            Relic::Might => "Raw Might",
        }
    }

    pub fn description(self) -> String {
        match self {
            Relic::ExtraEnergy => format!("Gain {EXTRA_ENERGY} max energy in every battle."),
            Relic::ExtraDraw => format!("Draw {EXTRA_CARDS_DRAWN} additional card each turn."),
            Relic::CorruptingStart => format!(
                "Start every turn of each battle with {STARTING_CORRUPTING_BOOST} Corrupting Boost."
            ),
            Relic::DeckUpgrade => format!(
                "When a run begins, {CARDS_UPGRADED_AT_RUN_START} random cards in your deck are upgraded."
            ),
            Relic::Might => format!("Gain {STARTING_MIGHT} Might at the start of each combat."),
        }
    }

    /// Whether the relic can be bought yet. Every relic is buyable now, so
    /// this always returns true. It stays, with `RelicError::NotAvailable`
    /// and the "coming soon" row in the dark web shop, so the next
    /// placeholder relic only has to return false here.
    pub fn is_available(self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelicError {
    NotAvailable,
    AlreadyOwned,
    NotEnoughPoints,
}

impl RelicError {
    pub fn code(self) -> &'static str {
        match self {
            RelicError::NotAvailable => "not_available",
            RelicError::AlreadyOwned => "already_owned",
            RelicError::NotEnoughPoints => "not_enough_points",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progression {
    #[serde(default)]
    points: u32,
    #[serde(default)]
    relics: Vec<Relic>,
    #[serde(default)]
    unlocked_cards: Vec<String>,
}

fn points_for_run(credits: i64) -> u32 {
    let bonus = credits.max(0) / CREDITS_PER_BONUS_POINT;
    POINTS_PER_RUN.saturating_add(u32::try_from(bonus).unwrap_or(u32::MAX))
}

impl Progression {
    pub fn points(&self) -> u32 {
        self.points
    }

    pub fn owns(&self, relic: Relic) -> bool {
        self.relics.contains(&relic)
    }

    pub fn award_run_completion(&mut self, credits: i64) -> u32 {
        let earned = points_for_run(credits);
        self.points = self.points.saturating_add(earned);
        earned
    }

    pub fn can_buy(&self, relic: Relic) -> bool {
        self.refusal(relic).is_none()
    }

    pub fn buy(&mut self, relic: Relic) -> Result<(), RelicError> {
        if let Some(refusal) = self.refusal(relic) {
            return Err(refusal);
        }
        self.points -= RELIC_PRICE;
        self.relics.push(relic);
        Ok(())
    }

    pub fn unlocked_cards(&self) -> &[String] {
        &self.unlocked_cards
    }

    pub fn is_unlocked(&self, card_id: &str, rarity: Rarity) -> bool {
        !starts_locked(rarity) || self.unlocked_cards.iter().any(|id| id == card_id)
    }

    /// The cards from `pool` that can be offered right now: the ones that
    /// never started locked, plus the ones a won contract has unlocked.
    pub fn open_pool(&self, pool: Vec<(String, Rarity)>) -> Vec<(String, Rarity)> {
        pool.into_iter()
            .filter(|(id, rarity)| self.is_unlocked(id, *rarity))
            .collect()
    }

    pub fn unlock_random_cards(
        &mut self,
        pool: &[(String, Rarity)],
        count: usize,
        rng: &mut impl Rng,
    ) -> Vec<String> {
        let mut candidates: Vec<String> = pool
            .iter()
            .filter(|(id, rarity)| !self.is_unlocked(id, *rarity))
            .map(|(id, _)| id.clone())
            .collect();
        candidates.sort();
        candidates.dedup();
        candidates.shuffle(rng);
        candidates.truncate(count);
        self.unlocked_cards.extend(candidates.iter().cloned());
        candidates
    }

    fn refusal(&self, relic: Relic) -> Option<RelicError> {
        if !relic.is_available() {
            Some(RelicError::NotAvailable)
        } else if self.owns(relic) {
            Some(RelicError::AlreadyOwned)
        } else if self.points < RELIC_PRICE {
            Some(RelicError::NotEnoughPoints)
        } else {
            None
        }
    }

    pub fn sanitized(mut self) -> Self {
        let mut seen: Vec<Relic> = Vec::new();
        self.relics.retain(|relic| {
            if seen.contains(relic) {
                false
            } else {
                seen.push(*relic);
                true
            }
        });
        self.unlocked_cards.sort();
        self.unlocked_cards.dedup();
        self
    }

    pub fn can_enter_shop(&self) -> bool {
        self.points >= SHOP_ENTRY_MIN_POINTS
    }

    pub fn spend_points_for_shop(&mut self) -> Result<u32, RelicError> {
        if !self.can_enter_shop() {
            return Err(RelicError::NotEnoughPoints);
        }
        let spent = self.points;
        self.points = 0;
        Ok(spent)
    }

    pub fn bonus_max_energy(&self) -> i32 {
        if self.owns(Relic::ExtraEnergy) {
            EXTRA_ENERGY
        } else {
            0
        }
    }

    pub fn bonus_cards_drawn(&self) -> i32 {
        if self.owns(Relic::ExtraDraw) {
            EXTRA_CARDS_DRAWN
        } else {
            0
        }
    }

    pub fn starting_corrupting_boost(&self) -> i32 {
        if self.owns(Relic::CorruptingStart) {
            STARTING_CORRUPTING_BOOST
        } else {
            0
        }
    }

    pub fn starting_might(&self) -> i32 {
        if self.owns(Relic::Might) {
            STARTING_MIGHT
        } else {
            0
        }
    }

    pub fn cards_upgraded_at_run_start(&self) -> usize {
        if self.owns(Relic::DeckUpgrade) {
            CARDS_UPGRADED_AT_RUN_START
        } else {
            0
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashSet;

    fn with_points(points: u32) -> Progression {
        Progression {
            points,
            ..Progression::default()
        }
    }

    #[test]
    fn a_finished_run_earns_a_point_plus_one_per_500_credits() {
        let mut progression = Progression::default();

        assert_eq!(progression.award_run_completion(0), 1);
        assert_eq!(progression.award_run_completion(1499), 3);
        assert_eq!(
            progression.award_run_completion(-200),
            1,
            "debt earns no bonus"
        );
        assert_eq!(progression.points(), 5);
    }

    #[test]
    fn buying_a_relic_spends_its_price_and_keeps_it() {
        let mut progression = with_points(2);

        assert_eq!(progression.buy(Relic::ExtraEnergy), Ok(()));

        assert!(progression.owns(Relic::ExtraEnergy));
        assert_eq!(progression.points(), 2 - RELIC_PRICE);
        assert_eq!(progression.bonus_max_energy(), EXTRA_ENERGY);
        assert_eq!(progression.bonus_cards_drawn(), 0, "only what was bought");
    }

    #[test]
    fn a_relic_is_refused_when_owned_unavailable_or_unaffordable() {
        let mut progression = with_points(5);
        progression.buy(Relic::ExtraDraw).unwrap();

        assert_eq!(
            progression.buy(Relic::ExtraDraw),
            Err(RelicError::AlreadyOwned)
        );
        assert_eq!(
            with_points(0).buy(Relic::DeckUpgrade),
            Err(RelicError::NotEnoughPoints)
        );
        assert_eq!(
            progression.points(),
            5 - RELIC_PRICE,
            "refusals cost nothing"
        );
    }

    #[test]
    fn each_relic_grants_its_own_bonus() {
        let mut progression = with_points(4);
        for relic in [
            Relic::ExtraEnergy,
            Relic::ExtraDraw,
            Relic::CorruptingStart,
            Relic::DeckUpgrade,
        ] {
            progression.buy(relic).unwrap();
        }

        assert_eq!(progression.bonus_max_energy(), EXTRA_ENERGY);
        assert_eq!(progression.bonus_cards_drawn(), EXTRA_CARDS_DRAWN);
        assert_eq!(
            progression.starting_corrupting_boost(),
            STARTING_CORRUPTING_BOOST
        );
        assert_eq!(
            progression.cards_upgraded_at_run_start(),
            CARDS_UPGRADED_AT_RUN_START
        );
    }

    #[test]
    fn entering_the_shop_spends_every_point_and_needs_one() {
        let mut progression = with_points(3);

        assert_eq!(progression.spend_points_for_shop(), Ok(3));
        assert_eq!(progression.points(), 0);
        assert_eq!(
            progression.spend_points_for_shop(),
            Err(RelicError::NotEnoughPoints)
        );
    }

    #[test]
    fn the_open_pool_is_the_never_locked_cards_plus_the_unlocked_ones() {
        let pool = card_pool();
        let open_ids = |progression: &Progression| {
            let mut ids: Vec<String> = progression
                .open_pool(pool.clone())
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            ids.sort();
            ids
        };
        let mut progression = Progression::default();
        assert_eq!(open_ids(&progression), vec!["malware", "shield", "strike"]);

        let unlocked = progression.unlock_random_cards(&pool, 1, &mut StdRng::seed_from_u64(6));

        let open = open_ids(&progression);
        assert_eq!(open.len(), 4);
        assert!(open.contains(&unlocked[0]));
    }

    #[test]
    fn a_reset_forgets_points_and_relics() {
        let mut progression = with_points(3);
        progression.buy(Relic::ExtraEnergy).unwrap();

        progression.reset();

        assert_eq!(progression, Progression::default());
    }

    #[test]
    fn a_loaded_progression_owns_each_relic_once() {
        let loaded = Progression {
            relics: vec![Relic::ExtraDraw, Relic::ExtraDraw, Relic::ExtraEnergy],
            ..Progression::default()
        }
        .sanitized();

        assert_eq!(loaded.relics, vec![Relic::ExtraDraw, Relic::ExtraEnergy]);
    }

    #[test]
    fn every_relic_id_round_trips() {
        for relic in Relic::ALL {
            assert_eq!(Relic::from_id(relic.id()), Some(relic));
        }
        assert_eq!(Relic::from_id("nope"), None);
    }

    #[test]
    fn might_can_be_bought_now_that_it_has_an_effect() {
        let mut progression = Progression::default();
        progression.award_run_completion(0);

        assert!(Relic::Might.is_available());
        assert!(progression.can_buy(Relic::Might));
        assert_eq!(progression.buy(Relic::Might), Ok(()));
        assert_eq!(progression.points(), 0);
        assert!(progression.owns(Relic::Might));
        assert_eq!(
            progression.buy(Relic::Might),
            Err(RelicError::AlreadyOwned),
            "it can only be bought once"
        );
    }

    #[test]
    fn starting_might_is_granted_only_with_the_relic() {
        let mut progression = Progression::default();
        assert_eq!(progression.starting_might(), 0);

        progression.award_run_completion(0);
        progression.award_run_completion(0);
        progression.buy(Relic::ExtraEnergy).unwrap();
        assert_eq!(
            progression.starting_might(),
            0,
            "another relic does not grant Might"
        );

        progression.buy(Relic::Might).unwrap();
        assert_eq!(progression.starting_might(), STARTING_MIGHT);
    }

    fn card_pool() -> Vec<(String, Rarity)> {
        [
            ("strike", Rarity::Common),
            ("shield", Rarity::Common),
            ("malware", Rarity::Uncommon),
            ("burn", Rarity::Rare),
            ("tor", Rarity::Rare),
            ("backdoor", Rarity::Rare),
            ("spyware", Rarity::Rare),
            ("wannacry", Rarity::Legendary),
            ("root_access", Rarity::Legendary),
        ]
        .iter()
        .map(|(id, rarity)| (id.to_string(), *rarity))
        .collect()
    }

    fn rarity_in(pool: &[(String, Rarity)], id: &str) -> Rarity {
        pool.iter()
            .find(|(card, _)| card == id)
            .map(|(_, rarity)| *rarity)
            .expect("card is in the pool")
    }

    #[test]
    fn rare_and_legendary_cards_start_locked() {
        let progression = Progression::default();

        assert!(progression.is_unlocked("strike", Rarity::Common));
        assert!(progression.is_unlocked("malware", Rarity::Uncommon));
        assert!(!progression.is_unlocked("backdoor", Rarity::Rare));
        assert!(!progression.is_unlocked("root_access", Rarity::Legendary));
        assert!(progression.unlocked_cards().is_empty());
    }

    #[test]
    fn a_win_unlocks_locked_cards_and_only_locked_ones() {
        let pool = card_pool();
        let mut progression = Progression::default();

        let unlocked = progression.unlock_random_cards(
            &pool,
            CARDS_UNLOCKED_PER_WIN,
            &mut StdRng::seed_from_u64(1),
        );

        assert_eq!(unlocked.len(), CARDS_UNLOCKED_PER_WIN);
        for id in &unlocked {
            let rarity = rarity_in(&pool, id);
            assert!(starts_locked(rarity), "{id} was never locked");
            assert!(progression.is_unlocked(id, rarity));
        }
        assert_eq!(progression.unlocked_cards().len(), CARDS_UNLOCKED_PER_WIN);
    }

    #[test]
    fn the_same_card_is_never_unlocked_twice_and_nothing_is_left_at_the_end() {
        let pool = card_pool();
        let mut progression = Progression::default();
        let mut rng = StdRng::seed_from_u64(2);

        let mut everything: Vec<String> = Vec::new();
        for _ in 0..2 {
            everything.extend(progression.unlock_random_cards(&pool, 3, &mut rng));
        }

        let distinct: HashSet<&String> = everything.iter().collect();
        assert_eq!(distinct.len(), everything.len(), "a card repeated");
        assert_eq!(everything.len(), 6, "all six locked cards are unlocked");
        assert!(
            progression
                .unlock_random_cards(&pool, 3, &mut rng)
                .is_empty(),
            "a win with nothing left to unlock unlocks nothing"
        );
    }

    #[test]
    fn fewer_locked_cards_than_requested_unlocks_what_is_left() {
        let pool = card_pool();
        let mut progression = Progression::default();
        let mut rng = StdRng::seed_from_u64(3);

        assert_eq!(progression.unlock_random_cards(&pool, 4, &mut rng).len(), 4);
        assert_eq!(progression.unlock_random_cards(&pool, 4, &mut rng).len(), 2);
    }

    #[test]
    fn the_same_seed_unlocks_the_same_cards_whatever_order_the_pool_is_in() {
        let mut reversed = card_pool();
        reversed.reverse();

        let mut forwards = Progression::default();
        let mut backwards = Progression::default();
        let a = forwards.unlock_random_cards(&card_pool(), 3, &mut StdRng::seed_from_u64(9));
        let b = backwards.unlock_random_cards(&reversed, 3, &mut StdRng::seed_from_u64(9));

        assert_eq!(a, b);
    }

    #[test]
    fn unlocked_cards_survive_being_saved_and_read_back() {
        let pool = card_pool();
        let mut progression = Progression::default();
        let unlocked = progression.unlock_random_cards(&pool, 3, &mut StdRng::seed_from_u64(4));

        let text = ron::to_string(&progression).expect("progress can be written");
        let restored: Progression = ron::from_str(&text).expect("progress can be read back");

        assert_eq!(restored, progression);
        for id in &unlocked {
            assert!(restored.is_unlocked(id, rarity_in(&pool, id)));
        }
    }

    #[test]
    fn a_progress_file_from_before_unlocks_still_loads() {
        let restored: Progression =
            ron::from_str("(points: 2, relics: [])").expect("an older file reads");

        assert_eq!(restored.points(), 2);
        assert!(restored.unlocked_cards().is_empty());
    }

    #[test]
    fn a_reset_locks_every_card_again() {
        let pool = card_pool();
        let mut progression = Progression::default();
        progression.unlock_random_cards(&pool, 6, &mut StdRng::seed_from_u64(5));

        progression.reset();

        assert!(progression.unlocked_cards().is_empty());
        assert!(!progression.is_unlocked("backdoor", Rarity::Rare));
    }

    #[test]
    fn a_loaded_progression_lists_each_unlocked_card_once() {
        let loaded = Progression {
            unlocked_cards: vec!["spyware".into(), "backdoor".into(), "spyware".into()],
            ..Progression::default()
        }
        .sanitized();

        assert_eq!(loaded.unlocked_cards().len(), 2);
    }

    /// Guards the data: the starting pool has to offer a reward screen, and
    /// there has to be something to unlock.
    #[test]
    fn the_real_card_set_has_open_cards_to_offer_and_locked_cards_to_unlock() {
        let cards = crate::card_database::parse_cards(include_str!("../../godot/data/cards.ron"));
        let open = cards.values().filter(|c| !starts_locked(c.rarity)).count();
        let locked = cards.values().filter(|c| starts_locked(c.rarity)).count();

        assert!(
            open >= crate::card_reward::OFFER_SIZE * 2,
            "only {open} open cards"
        );
        assert!(
            locked >= CARDS_UNLOCKED_PER_WIN,
            "only {locked} locked cards"
        );
    }
}

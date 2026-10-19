use serde::{Deserialize, Serialize};

const POINTS_PER_RUN: u32 = 1;

const CREDITS_PER_BONUS_POINT: i64 = 500;

pub const RELIC_PRICE: u32 = 1;
pub const SHOP_ENTRY_MIN_POINTS: u32 = 1;

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

    fn with_points(points: u32) -> Progression {
        Progression {
            points,
            relics: Vec::new(),
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
    fn a_reset_forgets_points_and_relics() {
        let mut progression = with_points(3);
        progression.buy(Relic::ExtraEnergy).unwrap();

        progression.reset();

        assert_eq!(progression, Progression::default());
    }

    #[test]
    fn a_loaded_progression_owns_each_relic_once() {
        let loaded = Progression {
            points: 0,
            relics: vec![Relic::ExtraDraw, Relic::ExtraDraw, Relic::ExtraEnergy],
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
        assert!(STARTING_MIGHT > 0);
    }
}

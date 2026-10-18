use serde::{Deserialize, Serialize};

const POINTS_PER_RUN: u32 = 1;

const CREDITS_PER_BONUS_POINT: i64 = 500;

pub const RELIC_PRICE: u32 = 1;
pub const SHOP_ENTRY_MIN_POINTS: u32 = 1;

const EXTRA_ENERGY: i32 = 1;
const EXTRA_CARDS_DRAWN: i32 = 1;
const STARTING_CORRUPTING_BOOST: i32 = 1;
const CARDS_UPGRADED_AT_RUN_START: usize = 5;

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
            Relic::Might => "Gain 1 Might. Not available yet.".to_string(),
        }
    }

    /// Whether the relic can be bought yet.
    pub fn is_available(self) -> bool {
        !matches!(self, Relic::Might)
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

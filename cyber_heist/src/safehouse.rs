//! Safehouse nodes: somewhere on the map to pause between fights.
//!
//! Covers the acceptance tests from Trello card #112 (G4 – Safehouse node):
//! 1. Lie Low: entering with noise at 60 and choosing Lie Low drops noise by
//!    30 to 30, and the node is completed.
//! 2. Upgrade instead: choosing Upgrade and picking a card upgrades that card
//!    for the rest of the run, and Lie Low can no longer be chosen at the same
//!    safehouse.
//!
//! A safehouse offers one or the other, never both, so it is a real choice
//! between recovering from a loud run and strengthening the deck. Both are
//! free: the price is the encounter slot on the route, not credits.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `safehouse_node.rs` adapts this for the screen.

use serde::{Deserialize, Serialize};

use crate::noise_meter::NoiseLevel;
use crate::run_deck::RunDeck;

/// How much noise Lying Low takes off. Half the meter's alert threshold, so it
/// pulls a loud run back from the edge without wiping it clean.
pub const LIE_LOW_NOISE_REDUCTION: i32 = 30;

/// Which of its two options a safehouse was used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafehouseChoice {
    LieLow,
    Upgrade,
}

impl SafehouseChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LieLow => "lie_low",
            Self::Upgrade => "upgrade",
        }
    }
}

/// Why a safehouse refused to do something. Refusals change nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafehouseError {
    /// The run is not in a safehouse encounter.
    NotInSafehouse,
    /// This safehouse has already been used, for either option.
    AlreadyUsed,
    /// Lying Low would not take any noise off, so it is not offered rather
    /// than spending the safehouse on nothing.
    NothingToLieLow,
    /// The card to upgrade is not in the run's deck.
    NotInDeck,
    /// The card has no upgraded version.
    NotUpgradable,
}

impl SafehouseError {
    /// A stable name for the Godot side to report and test against.
    pub fn code(self) -> &'static str {
        match self {
            Self::NotInSafehouse => "not_in_safehouse",
            Self::AlreadyUsed => "already_used",
            Self::NothingToLieLow => "nothing_to_lie_low",
            Self::NotInDeck => "not_in_deck",
            Self::NotUpgradable => "not_upgradable",
        }
    }
}

/// What Lying Low did, for the screen to report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayLow {
    /// How much noise actually came off, which is less than
    /// [`LIE_LOW_NOISE_REDUCTION`] when the meter was already lower.
    pub noise_removed: i32,
    /// The noise left on the meter afterwards.
    pub noise_left: i32,
}

/// The card a safehouse upgraded, for the screen to report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafehouseUpgrade {
    pub from_id: String,
    pub to_id: String,
}

/// One visit to a safehouse node. Opened when the player enters the node and
/// kept on the run until the encounter ends, so the screen being rebuilt
/// cannot offer the safehouse a second time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafehouseVisit {
    /// What the safehouse was used for, once it has been.
    choice: Option<SafehouseChoice>,
}

impl SafehouseVisit {
    pub fn new() -> Self {
        Self::default()
    }

    /// What this safehouse was used for, or `None` while both are on offer.
    pub fn choice(&self) -> Option<SafehouseChoice> {
        self.choice
    }

    /// Whether Lie Low would go through right now: the safehouse is unused
    /// and it would take some noise off.
    pub fn can_lie_low(&self, noise: &NoiseLevel) -> bool {
        self.choice.is_none() && lie_low_reduction(noise) > 0
    }

    /// Whether Upgrade is still on offer. Which cards can be upgraded is the
    /// card database's business, so this only says the safehouse is unused.
    pub fn can_upgrade(&self) -> bool {
        self.choice.is_none()
    }

    /// Lowers `noise` by up to [`LIE_LOW_NOISE_REDUCTION`] and uses up the
    /// safehouse. Refused, with nothing changed, once the safehouse has been
    /// used or when there is no noise to take off.
    pub fn lie_low(&mut self, noise: &mut NoiseLevel) -> Result<LayLow, SafehouseError> {
        if self.choice.is_some() {
            return Err(SafehouseError::AlreadyUsed);
        }
        if lie_low_reduction(noise) <= 0 {
            return Err(SafehouseError::NothingToLieLow);
        }

        let noise_removed = -noise.add(-LIE_LOW_NOISE_REDUCTION);
        self.choice = Some(SafehouseChoice::LieLow);
        Ok(LayLow {
            noise_removed,
            noise_left: noise.noise(),
        })
    }

    /// Upgrades one copy of `card_id` in `deck` to `upgraded_id` and uses up
    /// the safehouse. `upgraded_id` is `None` for a card with no upgrade.
    /// Refused, with nothing changed, once the safehouse has been used, for a
    /// card not in the deck, or for one that cannot be upgraded.
    pub fn upgrade(
        &mut self,
        deck: &mut RunDeck,
        card_id: &str,
        upgraded_id: Option<&str>,
    ) -> Result<SafehouseUpgrade, SafehouseError> {
        if self.choice.is_some() {
            return Err(SafehouseError::AlreadyUsed);
        }
        if deck.count_of(card_id) == 0 {
            return Err(SafehouseError::NotInDeck);
        }
        let upgraded_id = upgraded_id.ok_or(SafehouseError::NotUpgradable)?;

        deck.replace_one(card_id, upgraded_id);
        self.choice = Some(SafehouseChoice::Upgrade);
        Ok(SafehouseUpgrade {
            from_id: card_id.to_string(),
            to_id: upgraded_id.to_string(),
        })
    }
}

/// How much Lie Low would take off `noise` right now, after any detection
/// resistance and the meter's floor of zero. Worked out on a copy, so the
/// real meter is untouched.
pub fn lie_low_reduction(noise: &NoiseLevel) -> i32 {
    let mut preview = *noise;
    -preview.add(-LIE_LOW_NOISE_REDUCTION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise_meter::MAX_NOISE;

    fn noise_at(level: i32) -> NoiseLevel {
        let mut noise = NoiseLevel::new(MAX_NOISE);
        noise.add(level);
        noise
    }

    fn deck() -> RunDeck {
        RunDeck::from_card_ids(vec![
            "strike".to_string(),
            "strike".to_string(),
            "vpn".to_string(),
        ])
    }

    /// Acceptance scenario 1: noise 60, Lie Low, noise 30.
    #[test]
    fn lying_low_at_60_noise_drops_it_by_30_to_30() {
        let mut visit = SafehouseVisit::new();
        let mut noise = noise_at(60);

        let laid_low = visit.lie_low(&mut noise).unwrap();

        assert_eq!(noise.noise(), 30);
        assert_eq!(
            laid_low,
            LayLow {
                noise_removed: 30,
                noise_left: 30
            }
        );
        assert_eq!(visit.choice(), Some(SafehouseChoice::LieLow));
    }

    #[test]
    fn lying_low_never_takes_noise_below_zero() {
        let mut visit = SafehouseVisit::new();
        let mut noise = noise_at(12);

        let laid_low = visit.lie_low(&mut noise).unwrap();

        assert_eq!(noise.noise(), 0);
        assert_eq!(laid_low.noise_removed, 12);
    }

    #[test]
    fn lying_low_with_no_noise_is_refused_and_keeps_the_safehouse() {
        let mut visit = SafehouseVisit::new();
        let mut noise = noise_at(0);

        assert!(!visit.can_lie_low(&noise));
        assert_eq!(
            visit.lie_low(&mut noise),
            Err(SafehouseError::NothingToLieLow)
        );
        assert_eq!(visit.choice(), None, "the safehouse is still unused");
        assert!(visit.can_upgrade());
    }

    #[test]
    fn a_safehouse_can_only_be_lain_low_in_once() {
        let mut visit = SafehouseVisit::new();
        let mut noise = noise_at(90);
        visit.lie_low(&mut noise).unwrap();

        assert!(!visit.can_lie_low(&noise));
        assert_eq!(visit.lie_low(&mut noise), Err(SafehouseError::AlreadyUsed));
        assert_eq!(noise.noise(), 60, "the second attempt took nothing off");
    }

    /// Acceptance scenario 2: picking a card upgrades it in the run's deck.
    #[test]
    fn upgrading_replaces_one_copy_of_the_card_in_the_run_deck() {
        let mut visit = SafehouseVisit::new();
        let mut deck = deck();

        let upgrade = visit
            .upgrade(&mut deck, "strike", Some("strike_plus"))
            .unwrap();

        assert_eq!(
            upgrade,
            SafehouseUpgrade {
                from_id: "strike".to_string(),
                to_id: "strike_plus".to_string(),
            }
        );
        assert_eq!(deck.count_of("strike"), 1);
        assert_eq!(deck.count_of("strike_plus"), 1);
        assert_eq!(deck.len(), 3, "an upgrade adds and removes nothing");
        assert_eq!(visit.choice(), Some(SafehouseChoice::Upgrade));
    }

    /// Acceptance scenario 2: after upgrading, Lie Low is not also available.
    #[test]
    fn after_upgrading_the_player_cannot_also_lie_low() {
        let mut visit = SafehouseVisit::new();
        let mut deck = deck();
        let mut noise = noise_at(60);
        visit
            .upgrade(&mut deck, "strike", Some("strike_plus"))
            .unwrap();

        assert!(!visit.can_lie_low(&noise));
        assert_eq!(visit.lie_low(&mut noise), Err(SafehouseError::AlreadyUsed));
        assert_eq!(noise.noise(), 60);
    }

    #[test]
    fn after_lying_low_the_player_cannot_also_upgrade() {
        let mut visit = SafehouseVisit::new();
        let mut deck = deck();
        visit.lie_low(&mut noise_at(60)).unwrap();

        assert!(!visit.can_upgrade());
        assert_eq!(
            visit.upgrade(&mut deck, "strike", Some("strike_plus")),
            Err(SafehouseError::AlreadyUsed)
        );
        assert_eq!(deck.count_of("strike"), 2, "the deck is untouched");
    }

    #[test]
    fn upgrading_a_card_not_in_the_deck_is_refused_and_keeps_the_safehouse() {
        let mut visit = SafehouseVisit::new();
        let mut deck = deck();

        assert_eq!(
            visit.upgrade(&mut deck, "trojan", Some("trojan_plus")),
            Err(SafehouseError::NotInDeck)
        );
        assert_eq!(visit.choice(), None);
        assert_eq!(deck.card_ids(), self::deck().card_ids());
    }

    #[test]
    fn upgrading_a_card_with_no_upgrade_is_refused_and_keeps_the_safehouse() {
        let mut visit = SafehouseVisit::new();
        let mut deck = deck();

        assert_eq!(
            visit.upgrade(&mut deck, "vpn", None),
            Err(SafehouseError::NotUpgradable)
        );
        assert_eq!(visit.choice(), None);
        assert_eq!(deck.count_of("vpn"), 1);
    }

    #[test]
    fn the_reduction_preview_matches_what_lying_low_takes_off() {
        for level in [0, 5, 30, 31, 60, 100] {
            let noise = noise_at(level);
            let preview = lie_low_reduction(&noise);

            let mut applied = noise;
            let result = SafehouseVisit::new().lie_low(&mut applied);

            match result {
                Ok(laid_low) => assert_eq!(laid_low.noise_removed, preview, "noise {level}"),
                Err(_) => assert_eq!(preview, 0, "noise {level}"),
            }
            assert_eq!(lie_low_reduction(&noise), preview, "the preview is a copy");
        }
    }

    #[test]
    fn error_codes_are_distinct() {
        let codes = [
            SafehouseError::NotInSafehouse,
            SafehouseError::AlreadyUsed,
            SafehouseError::NothingToLieLow,
            SafehouseError::NotInDeck,
            SafehouseError::NotUpgradable,
        ]
        .map(SafehouseError::code);
        let mut unique = codes.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
    }
}

//! What a run has achieved so far, for the end screen to summarise.
//!
//! Plain Rust, so the counting rules can be tested without Godot. `RunState`
//! owns the stats, which ties their lifetime to the run: starting a new run
//! starts them again from zero.
//!
//! Wins are counted by `RunState` itself as encounters complete. Credits and
//! cards are gained elsewhere (an event, a card reward, later a combat payout
//! or the contract payout), so whatever applies one also records it through
//! `RunState::record_credit_change` or `RunState::record_card_added`.

use crate::contract_map::EncounterType;

/// Running totals for the current run.
///
/// The fields only change through the `record_*` methods, which hold the
/// counting rules, so nothing can push a total backwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    combats_won: usize,
    credits_earned: i64,
    cards_added: usize,
}

impl RunStats {
    /// Combat, elite and boss encounters completed. A fight the player was
    /// caught in is not completed, so it is not a win.
    pub fn combats_won(&self) -> usize {
        self.combats_won
    }

    /// Every credit gained over the run, added up. Losses do not reduce it:
    /// it is what the run earned, not the balance it left behind.
    pub fn credits_earned(&self) -> i64 {
        self.credits_earned
    }

    /// Cards that went into the run's deck after it started, on top of the
    /// starter deck.
    pub fn cards_added(&self) -> usize {
        self.cards_added
    }

    /// Counts a completed encounter as a win when it was a fight.
    pub fn record_encounter_completed(&mut self, encounter_type: EncounterType) {
        if encounter_type.is_fight() {
            self.combats_won += 1;
        }
    }

    /// Records a change to the player's credits. Takes losses as well as
    /// gains so a caller can pass along whatever it applied, but only a gain
    /// counts towards `credits_earned`.
    pub fn record_credit_change(&mut self, amount: i64) {
        if amount > 0 {
            self.credits_earned = self.credits_earned.saturating_add(amount);
        }
    }

    pub fn record_card_added(&mut self) {
        self.cards_added += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encounter_text::all_encounter_types;

    #[test]
    fn a_run_starts_with_nothing_recorded() {
        let stats = RunStats::default();

        assert_eq!(stats.combats_won(), 0);
        assert_eq!(stats.credits_earned(), 0);
        assert_eq!(stats.cards_added(), 0);
    }

    /// Combat, elite and boss encounters are fights; the entry node, events
    /// and shops are not, so completing them is not a win.
    #[test]
    fn only_fights_count_as_combats_won() {
        for encounter_type in all_encounter_types() {
            let mut stats = RunStats::default();

            stats.record_encounter_completed(encounter_type);

            let is_fight = matches!(
                encounter_type,
                EncounterType::Combat | EncounterType::Elite | EncounterType::Boss
            );
            assert_eq!(
                stats.combats_won(),
                usize::from(is_fight),
                "completing a {} encounter",
                encounter_type.as_str()
            );
        }
    }

    #[test]
    fn credits_earned_adds_up_every_gain() {
        let mut stats = RunStats::default();

        stats.record_credit_change(120);
        stats.record_credit_change(60);

        assert_eq!(stats.credits_earned(), 180);
    }

    /// A fine or a bad gamble costs the player credits, but it does not undo
    /// what they earned: "earned" only ever goes up.
    #[test]
    fn losses_and_zero_changes_do_not_reduce_credits_earned() {
        let mut stats = RunStats::default();

        stats.record_credit_change(120);
        stats.record_credit_change(-80);
        stats.record_credit_change(0);
        stats.record_credit_change(60);

        assert_eq!(stats.credits_earned(), 180);
    }

    #[test]
    fn a_run_that_only_lost_credits_earned_nothing() {
        let mut stats = RunStats::default();

        stats.record_credit_change(-100);
        stats.record_credit_change(i64::MIN);

        assert_eq!(stats.credits_earned(), 0);
    }

    #[test]
    fn an_enormous_gain_saturates_instead_of_overflowing() {
        let mut stats = RunStats::default();

        stats.record_credit_change(i64::MAX);
        stats.record_credit_change(i64::MAX);

        assert_eq!(stats.credits_earned(), i64::MAX);
    }

    #[test]
    fn every_card_added_is_counted() {
        let mut stats = RunStats::default();

        stats.record_card_added();
        stats.record_card_added();
        stats.record_card_added();

        assert_eq!(stats.cards_added(), 3);
    }
}

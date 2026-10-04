//! What a run has achieved so far, for the end screen to summarise.
//!
//! Plain Rust, so the counting rules can be tested without Godot. `RunState`
//! owns the stats, which ties their lifetime to the run: starting a new run
//! starts them again from zero.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_map::EncounterType;
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

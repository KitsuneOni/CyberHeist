//! The sentry: the security construct that takes a turn after the player.
//!
//! Covers the "security system takes its own turn" acceptance tests from
//! Trello card 30:
//! 1. The sentry performs its queued action when its phase begins, and the
//!    result is in the game state before the next player turn starts.
//! 2. If that action pushes the noise meter to its cap, the meter stops at the
//!    cap and the run ends in detection failure.
//!
//! Every sentry has a name, so a later encounter can put a different construct
//! in the way without any of these rules changing.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `sentry_node.rs` adapts this for scenes.

/// One action a sentry can take on its turn.
///
/// The player gets to see the queued one while it is still their turn, which
/// is the whole point: you plan against a threat you can read, rather than
/// being surprised by a dice roll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentryAction {
    /// Player-facing name, shown while the action is queued and again once it
    /// has been performed.
    pub name: String,
    /// Noise this action adds to the meter when it runs.
    pub noise: i32,
}

impl SentryAction {
    pub fn new(name: &str, noise: i32) -> Self {
        Self {
            name: name.to_string(),
            noise,
        }
    }
}

/// What the sentry did on its turn, for the scene to show and to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentryTurn {
    /// The sentry that acted, so the scene can name it without looking it up.
    pub sentry_name: String,
    /// The action that just ran.
    pub action_name: String,
    /// Noise it actually added, which is less than the action's own noise if
    /// the meter filled up partway through.
    pub noise_added: i32,
    /// The meter after the action ran.
    pub noise: i32,
    /// True when the meter reached its cap, meaning the player was detected
    /// and the run is over.
    pub run_failed: bool,
}

/// A named security construct: its noise meter and the actions it works
/// through.
///
/// Actions come from an authored list that the sentry cycles, so a turn plays
/// out the same way every time and is easy to test. Picking actions in
/// response to what the player is doing is a later card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentry {
    name: String,
    noise: i32,
    max_noise: i32,
    script: Vec<SentryAction>,
    next_index: usize,
}

impl Sentry {
    /// Builds a sentry with an empty meter and `script` as its action queue.
    ///
    /// An empty script is allowed and means the sentry has nothing queued, so
    /// its turn does nothing at all.
    pub fn new(name: &str, max_noise: i32, script: Vec<SentryAction>) -> Self {
        Self {
            name: name.to_string(),
            noise: 0,
            max_noise: max_noise.max(0),
            script,
            next_index: 0,
        }
    }

    /// The sentry guarding the current combat encounter. Three actions on a
    /// ten point meter, so an encounter the player never quietens down runs
    /// out at about turn five.
    pub fn warden_7() -> Self {
        Self::new(
            "WARDEN-7",
            10,
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
                SentryAction::new("Lockdown Probe", 3),
            ],
        )
    }

    /// Renames the sentry, for an encounter that fields a different construct
    /// with the same behaviour.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn noise(&self) -> i32 {
        self.noise
    }

    pub fn max_noise(&self) -> i32 {
        self.max_noise
    }

    /// True once the meter is full, which is what being detected means.
    pub fn is_detected(&self) -> bool {
        self.noise >= self.max_noise
    }

    /// The action the sentry will take when its turn begins, or `None` if
    /// there is nothing queued.
    pub fn queued_action(&self) -> Option<&SentryAction> {
        self.script.get(self.next_index)
    }

    /// Moves the meter by `noise` and returns how much actually went on, which
    /// is what the caller should report rather than the amount asked for.
    ///
    /// The meter never drops below zero or climbs past its cap, so a very loud
    /// action cannot bank extra noise for later and a quietening effect cannot
    /// build up credit.
    pub fn add_noise(&mut self, noise: i32) -> i32 {
        let before = self.noise;
        self.noise = (self.noise + noise).clamp(0, self.max_noise);
        self.noise - before
    }

    /// Takes the sentry's turn: run the queued action, apply its noise, then
    /// queue the next one.
    ///
    /// Everything the action does has landed by the time this returns, so
    /// whoever starts the next player turn is working from the updated meter.
    /// Returns `None` when nothing was queued and the turn was skipped.
    pub fn take_turn(&mut self) -> Option<SentryTurn> {
        let action = self.script.get(self.next_index)?.clone();
        let noise_added = self.add_noise(action.noise);

        // Walk to the next action, going back to the start of the list once
        // the sentry has been through all of them.
        self.next_index = (self.next_index + 1) % self.script.len();

        Some(SentryTurn {
            sentry_name: self.name.clone(),
            action_name: action.name,
            noise_added,
            noise: self.noise,
            run_failed: self.is_detected(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_sentry_is_silent_with_its_first_action_queued() {
        let sentry = Sentry::warden_7();

        assert_eq!(sentry.name(), "WARDEN-7");
        assert_eq!(sentry.noise(), 0);
        assert_eq!(sentry.queued_action().unwrap().name, "Packet Sniff");
        assert!(!sentry.is_detected());
    }

    #[test]
    fn the_turn_performs_the_queued_action_and_leaves_the_result_behind() {
        let mut sentry = Sentry::new("WARDEN-7", 10, vec![SentryAction::new("Trace Sweep", 2)]);

        let turn = sentry.take_turn().unwrap();

        assert_eq!(turn.sentry_name, "WARDEN-7");
        assert_eq!(turn.action_name, "Trace Sweep");
        assert_eq!(turn.noise_added, 2);
        assert_eq!(turn.noise, 2);
        assert!(!turn.run_failed);
        // The state a next player turn would read matches what was reported.
        assert_eq!(sentry.noise(), 2);
    }

    #[test]
    fn the_queue_moves_on_each_turn_and_starts_over_at_the_end() {
        let mut sentry = Sentry::new(
            "WARDEN-7",
            20,
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
            ],
        );

        assert_eq!(sentry.queued_action().unwrap().name, "Packet Sniff");
        sentry.take_turn().unwrap();
        assert_eq!(sentry.queued_action().unwrap().name, "Trace Sweep");
        sentry.take_turn().unwrap();
        assert_eq!(sentry.queued_action().unwrap().name, "Packet Sniff");
        assert_eq!(sentry.noise(), 3);
    }

    #[test]
    fn an_action_that_fills_the_meter_fails_the_run() {
        let mut sentry = Sentry::new("WARDEN-7", 3, vec![SentryAction::new("Lockdown Probe", 3)]);

        let turn = sentry.take_turn().unwrap();

        assert_eq!(turn.noise, 3);
        assert!(turn.run_failed);
        assert!(sentry.is_detected());
    }

    #[test]
    fn noise_stops_at_the_cap_instead_of_overshooting_it() {
        let mut sentry = Sentry::new("WARDEN-7", 4, vec![SentryAction::new("Lockdown Probe", 9)]);

        let turn = sentry.take_turn().unwrap();

        assert_eq!(turn.noise_added, 4);
        assert_eq!(turn.noise, 4);
        assert_eq!(sentry.noise(), sentry.max_noise());
        assert!(turn.run_failed);
    }

    #[test]
    fn a_turn_that_stays_under_the_cap_lets_the_run_continue() {
        let mut sentry = Sentry::new("WARDEN-7", 5, vec![SentryAction::new("Packet Sniff", 1)]);

        for _ in 0..4 {
            assert!(!sentry.take_turn().unwrap().run_failed);
        }

        assert_eq!(sentry.noise(), 4);
        assert!(!sentry.is_detected());
    }

    #[test]
    fn a_sentry_with_nothing_queued_skips_its_turn() {
        let mut sentry = Sentry::new("WARDEN-7", 10, Vec::new());

        assert_eq!(sentry.queued_action(), None);
        assert_eq!(sentry.take_turn(), None);
        assert_eq!(sentry.noise(), 0);
    }

    #[test]
    fn noise_from_elsewhere_shares_the_meter_and_stays_in_range() {
        let mut sentry = Sentry::new("WARDEN-7", 10, vec![SentryAction::new("Trace Sweep", 2)]);

        assert_eq!(sentry.add_noise(3), 3);
        assert_eq!(sentry.noise(), 3);

        // Something quietening can pull the meter back down, but not past zero.
        assert_eq!(sentry.add_noise(-9), -3);
        assert_eq!(sentry.noise(), 0);
    }

    #[test]
    fn noise_already_on_the_meter_can_make_the_next_turn_the_fatal_one() {
        let mut sentry = Sentry::new("WARDEN-7", 5, vec![SentryAction::new("Trace Sweep", 2)]);

        sentry.add_noise(3);
        let turn = sentry.take_turn().unwrap();

        assert_eq!(turn.noise, 5);
        assert!(turn.run_failed);
    }

    #[test]
    fn a_renamed_sentry_reports_the_new_name_on_its_turn() {
        let mut sentry = Sentry::warden_7();

        sentry.set_name("BLACKGATE");
        let turn = sentry.take_turn().unwrap();

        assert_eq!(sentry.name(), "BLACKGATE");
        assert_eq!(turn.sentry_name, "BLACKGATE");
    }
}

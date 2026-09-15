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

/// A named security construct: its noise meter and the actions it works
/// through.
///
/// Actions come from an authored list that the sentry cycles, so a turn plays
/// out the same way every time and is easy to test. Picking actions in
/// response to what the player is doing is a later card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentry {
    name: String,
    script: Vec<SentryAction>,
    next_index: usize,
}

impl Sentry {
    /// Builds a sentry with an empty meter and `script` as its action queue.
    ///
    /// An empty script is allowed and means the sentry has nothing queued, so
    /// its turn does nothing at all.
    pub fn new(name: &str, script: Vec<SentryAction>) -> Self {
        Self {
            name: name.to_string(),
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

    /// The action the sentry will take when its turn begins, or `None` if
    /// there is nothing queued.
    pub fn queued_action(&self) -> Option<&SentryAction> {
        self.script.get(self.next_index)
    }

    /// Takes the sentry's turn: run the queued action, apply its noise, then
    /// queue the next one.
    ///
    /// Everything the action does has landed by the time this returns, so
    /// whoever starts the next player turn is working from the updated meter.
    /// Returns `None` when nothing was queued and the turn was skipped.
    pub fn perform_queued_action(&mut self) -> Option<SentryAction> {
        let action = self.script.get(self.next_index)?.clone();
        self.next_index = (self.next_index + 1) % self.script.len();
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_sentry_has_its_first_action_queued() {
        let sentry = Sentry::warden_7();
        assert_eq!(sentry.name(), "WARDEN-7");
        assert_eq!(sentry.queued_action().unwrap().name, "Packet Sniff");
    }

    #[test]
    fn performing_the_queued_action_reports_it_and_advances_the_queue() {
        let mut sentry = Sentry::new(
            "WARDEN-7",
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
            ],
        );

        let action = sentry.perform_queued_action().unwrap();
        assert_eq!(action.name, "Packet Sniff");
        assert_eq!(action.noise, 1);
        assert_eq!(sentry.queued_action().unwrap().name, "Trace Sweep");

        sentry.perform_queued_action().unwrap();
        assert_eq!(
            sentry.queued_action().unwrap().name,
            "Packet Sniff",
            "queue wraps back to the start"
        );
    }

    #[test]
    fn a_sentry_with_nothing_queued_reports_no_action() {
        let mut sentry = Sentry::new("WARDEN-7", Vec::new());
        assert_eq!(sentry.queued_action(), None);
        assert_eq!(sentry.perform_queued_action(), None);
    }

    #[test]
    fn a_renamed_sentry_reports_the_new_name() {
        let mut sentry = Sentry::warden_7();
        sentry.set_name("BLACKGATE");
        assert_eq!(sentry.name(), "BLACKGATE");
    }
}

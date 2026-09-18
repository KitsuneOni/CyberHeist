//! The sentry: the security construct that takes a turn after the player.
//!
//! Covers the "security system takes its own turn" acceptance tests from
//! Trello card 30:
//! 1. The sentry performs its queued action when its phase begins, and the
//!    result is in the game state before the next player turn starts.
//! 2. If that action pushes the noise meter to its cap, the meter stops at the
//!    cap and the run ends in detection failure.
//!
//! Also covers the sentry having its own health, so damage cards like Strike
//! can bring it down and end the encounter that way:
//! 3. Damage applied to the sentry is clamped between 0 and its max health,
//!    and the actual amount lost (not the authored amount) is reported back.
//! 4. A sentry at 0 health is defeated and takes no further turns.
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

/// A named security construct and the actions it works through.
/// Noise is owned by the shared meter, not by an encounter; health belongs to
/// the sentry itself, since it's specific to this construct rather than
/// shared across the encounter the way noise is.
///
/// Actions come from an authored list that the sentry cycles, so a turn plays
/// out the same way every time and is easy to test. Picking actions in
/// response to what the player is doing is a later card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentry {
    name: String,
    script: Vec<SentryAction>,
    next_index: usize,
    health: i32,
    max_health: i32,
    corruption: i32,
    corruption_boost: i32,
}

impl Sentry {
    /// Builds a sentry with `script` as its action queue, at a default
    /// health of 50/50. Use `with_health` to set a different amount.
    ///
    /// An empty script is allowed and means the sentry has nothing queued, so
    /// its turn does nothing at all.
    pub fn new(name: &str, script: Vec<SentryAction>) -> Self {
        Self {
            name: name.to_string(),
            script,
            next_index: 0,
            health: 50,
            max_health: 50,
            corruption: 0,
            corruption_boost: 0,
        }
    }

    /// Sets both current and max health to `max_health`. Chainable so
    /// built-in sentries like `warden_7()` can stay one-liners.
    pub fn with_health(mut self, max_health: i32) -> Self {
        self.health = max_health;
        self.max_health = max_health;
        self
    }

    /// The sentry guarding the current combat encounter, with three cycling
    /// actions. The shared meter determines when those actions cause detection.
    pub fn warden_7() -> Self {
        Self::new(
            "WARDEN-7",
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
                SentryAction::new("Lockdown Probe", 3),
            ],
        )
        .with_health(40)
    }

    /// Renames the sentry, for an encounter that fields a different construct
    /// with the same behaviour.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn health(&self) -> i32 {
        self.health
    }

    pub fn max_health(&self) -> i32 {
        self.max_health
    }

    pub fn corruption(&self) -> i32 {
        self.corruption
    }

    pub fn corruption_boost(&self) -> i32 {
        self.corruption_boost
    }

    /// True once health has been brought down to 0. A defeated sentry takes
    /// no further turns (enforced by the caller, e.g. `SentryNode`).
    pub fn is_defeated(&self) -> bool {
        self.health <= 0
    }

    /// Applies damage, clamped so health stays within `0..=max_health`.
    /// Returns how much health was actually lost, which may be less than
    /// `amount` if the sentry didn't have that much health left — mirrors
    /// `NoiseLevel::add`'s clamped-delta contract so callers report the real
    /// effect rather than the authored amount.
    ///
    /// A negative `amount` would heal the sentry, clamped at `max_health`;
    /// nothing currently does this, but the clamp makes it safe either way.
    pub fn take_damage(&mut self, amount: i32) -> i32 {
        let before = self.health;
        self.health = self.health.saturating_sub(amount).clamp(0, self.max_health);
        before - self.health
    }

    pub fn add_corruption(&mut self, amount: i32) -> i32 {
        let actual_amount = if amount > 0 {
            amount + self.corruption_boost
        } else {
            amount
        };
        let before = self.corruption;
        self.corruption = (self.corruption + actual_amount).max(0);
        self.corruption - before
    }

    pub fn add_corruption_boost(&mut self, amount: i32) -> i32 {
        let before = self.corruption_boost;
        self.corruption_boost = (self.corruption_boost + amount).max(0);
        self.corruption_boost - before
    }

    pub fn clear_corruption_boost(&mut self) -> i32 {
        let cleared = self.corruption_boost;
        self.corruption_boost = 0;
        cleared
    }

    pub fn resolve_corruption_tick(&mut self) -> i32 {
        if self.corruption <= 0 {
            return 0;
        }
        let damage = self.corruption;
        let dealt = self.take_damage(damage);
        self.corruption = (self.corruption - 1).max(0);
        dealt
    }

    /// The action the sentry will take when its turn begins, or `None` if
    /// there is nothing queued.
    pub fn queued_action(&self) -> Option<&SentryAction> {
        self.script.get(self.next_index)
    }

    /// Returns the announced action and queues the next one. The caller applies
    /// its authored noise to the shared meter and reports the actual clamped
    /// delta separately. Returns `None` when nothing was queued.
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
    fn each_announced_action_runs_once_and_the_queue_wraps_without_stale_intent() {
        use crate::noise_meter::NoiseLevel;

        let script = vec![
            SentryAction::new("Packet Sniff", 1),
            SentryAction::new("Trace Sweep", 2),
            SentryAction::new("Lockdown Probe", 3),
        ];
        let mut sentry = Sentry::new("WARDEN-7", script.clone());
        let mut meter = NoiseLevel::new(100);

        for index in 0..7 {
            let expected = &script[index % script.len()];
            let before = sentry.clone();
            assert_eq!(sentry.queued_action(), Some(expected));
            assert_eq!(sentry.queued_action(), Some(expected));
            assert_eq!(sentry, before, "reading intent cannot advance the queue");

            let action = sentry.perform_queued_action().unwrap();
            let noise_before = meter.noise();
            assert_eq!(&action, expected);
            assert_eq!(meter.add(action.noise), expected.noise);
            assert_eq!(meter.noise(), noise_before + expected.noise);
            assert!(!meter.is_at_cap());
            assert_eq!(
                sentry.queued_action(),
                Some(&script[(index + 1) % script.len()])
            );
        }
    }

    #[test]
    fn authored_intent_and_actual_clamped_change_are_distinct_at_cap() {
        use crate::noise_meter::NoiseLevel;

        let mut sentry = Sentry::new("WARDEN-7", vec![SentryAction::new("Lockdown Probe", 3)]);
        let mut meter = NoiseLevel::new(100);
        meter.add(99);
        let announced = sentry.queued_action().unwrap().clone();
        let action = sentry.perform_queued_action().unwrap();

        assert_eq!(action, announced);
        assert_eq!(action.noise, 3);
        assert_eq!(meter.add(action.noise), 1);
        assert_eq!(meter.noise(), 100);
        assert!(meter.is_at_cap());
        assert_eq!(sentry.queued_action(), Some(&announced));
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

    #[test]
    fn a_fresh_sentry_starts_at_full_health() {
        let sentry = Sentry::warden_7();
        assert_eq!(sentry.health(), sentry.max_health());
        assert_eq!(sentry.health(), 40);
        assert!(!sentry.is_defeated());
    }

    #[test]
    fn with_health_sets_both_current_and_max() {
        let sentry = Sentry::warden_7().with_health(10);
        assert_eq!(sentry.health(), 10);
        assert_eq!(sentry.max_health(), 10);
    }

    #[test]
    fn damage_clamps_at_zero_and_reports_actual_amount_lost() {
        let mut sentry = Sentry::warden_7().with_health(10);
        assert_eq!(sentry.take_damage(6), 6);
        assert_eq!(sentry.health(), 4);
        assert!(!sentry.is_defeated());

        assert_eq!(sentry.take_damage(10), 4, "clamped, not the full 10");
        assert_eq!(sentry.health(), 0);
        assert!(sentry.is_defeated());

        assert_eq!(sentry.take_damage(5), 0, "already dead, no further loss");
        assert_eq!(sentry.health(), 0);
    }

    #[test]
    fn damage_does_not_overshoot_max_health_when_negative() {
        let mut sentry = Sentry::warden_7().with_health(10);
        sentry.take_damage(5);
        assert_eq!(sentry.health(), 5);
        assert_eq!(
            sentry.take_damage(-100),
            -5,
            "clamped to max, not the full heal"
        );
        assert_eq!(sentry.health(), 10);
    }
}

#[cfg(test)]
mod stress {
    use super::*;
    use crate::noise_meter::NoiseLevel;

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn authored_actions_of_any_loudness_survive_repeated_turns() {
        let mut seed = 0xC0FF_EE01_u32;
        for cap in [0, 1, 100, 1_000] {
            let mut script = vec![
                SentryAction::new("Full Sweep", i32::MAX),
                SentryAction::new("Total Blackout", i32::MIN),
            ];
            for index in 0..7 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                script.push(SentryAction::new(&format!("Action {index}"), seed as i32));
            }
            let mut sentry = Sentry::new("WARDEN-7", script);
            let mut meter = NoiseLevel::new(cap);
            for _ in 0..50_000 {
                let announced = sentry.queued_action().cloned().unwrap();
                let action = sentry.perform_queued_action().unwrap();
                assert_eq!(action, announced);
                let before = meter.noise();
                let delta = meter.add(action.noise);
                assert_eq!(meter.noise() - before, delta);
                assert!((0..=cap).contains(&meter.noise()));
                assert_eq!(meter.is_at_cap(), meter.noise() == cap);
            }
        }
    }

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn damage_of_any_size_keeps_health_within_bounds() {
        let mut seed = 0xDEAD_BEEF_u32;
        for max_health in [0, 1, 40, 1_000] {
            let mut sentry = Sentry::warden_7().with_health(max_health);
            for _ in 0..50_000 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let amount = seed as i32;
                let before = sentry.health();
                let lost = sentry.take_damage(amount);
                assert_eq!(sentry.health(), before - lost);
                assert!((0..=max_health).contains(&sentry.health()));
                assert_eq!(sentry.is_defeated(), sentry.health() == 0);
            }
        }
    }
}

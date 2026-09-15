//! Thin Godot adapter for the sentry in `sentry.rs`.
//!
//! Holds one encounter's sentry and its noise meter. The scene connects
//! `DrawPhase`'s `security_phase` signal to `on_security_phase`, so the sentry
//! takes its turn at the point the player ends theirs.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::sentry::Sentry;

#[derive(GodotClass)]
#[class(base=Node)]
struct SentryNode {
    /// Which construct is guarding this encounter. Set it in the Inspector to
    /// field a different one, leave it blank to keep the built-in WARDEN-7.
    #[export]
    sentry_name: GString,

    sentry: Sentry,
    base: Base<Node>,
}

#[godot_api]
impl INode for SentryNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            sentry_name: GString::new(),
            sentry: Sentry::warden_7(),
            base,
        }
    }

    fn ready(&mut self) {
        // The name is data, so a scene can put a different construct in the
        // way without any rule changes.
        let name = self.sentry_name.to_string();
        if !name.is_empty() {
            self.sentry.set_name(&name);
        }
    }
}

#[godot_api]
impl SentryNode {
    /// Emitted once the sentry has taken its turn, carrying the same details
    /// `take_turn` returns. The combat scene listens for this to update the
    /// meter and to send the player to the detection screen when the run has
    /// failed.
    #[signal]
    fn turn_resolved(outcome: VarDictionary);

    /// Runs the sentry's turn in response to the player ending theirs, then
    /// announces what happened.
    ///
    /// `DrawPhase` emits `security_phase` partway through `end_turn`, so this
    /// runs before the next hand is dealt and the meter is already up to date
    /// when the new turn begins.
    #[func]
    fn on_security_phase(&mut self, finished_turn: i32) {
        let outcome = self.take_turn();
        godot_print!(
            "{} acted after turn {finished_turn}: {} (noise {} / {})",
            self.sentry.name(),
            outcome.at("action"),
            self.sentry.noise(),
            self.sentry.max_noise(),
        );
        self.signals().turn_resolved().emit(&outcome);
    }

    /// The name of the construct guarding this encounter. Not called `name`
    /// because every Godot node already has one of those.
    #[func]
    fn construct_name(&self) -> GString {
        GString::from(self.sentry.name())
    }

    #[func]
    fn noise(&self) -> i32 {
        self.sentry.noise()
    }

    #[func]
    fn max_noise(&self) -> i32 {
        self.sentry.max_noise()
    }

    /// Name of the action the sentry will take next, or an empty string if it
    /// has nothing queued.
    #[func]
    fn queued_action_name(&self) -> GString {
        match self.sentry.queued_action() {
            Some(action) => GString::from(action.name.as_str()),
            None => GString::new(),
        }
    }

    /// Noise the queued action would add, so the player can weigh it up before
    /// deciding to end the turn.
    #[func]
    fn queued_action_noise(&self) -> i32 {
        self.sentry.queued_action().map_or(0, |action| action.noise)
    }

    /// Adds noise from something other than the sentry, such as a loud card.
    /// Returns how much actually went on the meter.
    #[func]
    fn add_noise(&mut self, noise: i32) -> i32 {
        self.sentry.add_noise(noise)
    }

    /// Takes the sentry's turn and reports what it did:
    /// `{ok, sentry, action, noise_added, noise, max_noise, run_failed}`.
    ///
    /// `ok` is false only when there was nothing queued, in which case the
    /// turn was skipped and the meter has not moved.
    #[func]
    fn take_turn(&mut self) -> VarDictionary {
        match self.sentry.take_turn() {
            Some(turn) => vdict! {
                "ok" => true,
                "sentry" => turn.sentry_name.as_str(),
                "action" => turn.action_name.as_str(),
                "noise_added" => turn.noise_added,
                "noise" => turn.noise,
                "max_noise" => self.sentry.max_noise(),
                "run_failed" => turn.run_failed,
            },
            None => vdict! {
                "ok" => false,
                "sentry" => self.sentry.name(),
                "action" => "",
                "noise_added" => 0,
                "noise" => self.sentry.noise(),
                "max_noise" => self.sentry.max_noise(),
                "run_failed" => self.sentry.is_detected(),
            },
        }
    }
}

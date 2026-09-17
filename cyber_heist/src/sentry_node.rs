//! Thin Godot adapter for the sentry in `sentry.rs`.
//!
//! Holds one encounter's intent queue; noise belongs to NoiseMeterGlobal.
//! The scene connects `DrawPhase`'s `security_phase` signal to `on_security_phase`, so the sentry
//! takes its turn at the point the player ends theirs.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::noise_meter::NoiseMeter;
use crate::sentry::Sentry;

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct SentryNode {
    /// Which construct is guarding this encounter. Set it in the Inspector to
    /// field a different one, leave it blank to keep the built-in WARDEN-7.
    #[export]
    sentry_name: GString,

    sentry: Sentry,
    noise_meter: Option<Gd<NoiseMeter>>,
    base: Base<Node>,
}

#[godot_api]
impl INode for SentryNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            sentry_name: GString::new(),
            sentry: Sentry::warden_7(),
            noise_meter: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.noise_meter = Some(
            self.base()
                .get_node_as::<NoiseMeter>("/root/NoiseMeterGlobal"),
        );
        // The name is data, so a scene can put a different construct in the
        // way without any rule changes.
        let name = self.sentry_name.to_string();
        if !name.is_empty() {
            self.sentry.set_name(&name);
        }
    }
}

impl SentryNode {
    fn noise_meter(&self) -> Gd<NoiseMeter> {
        // A handle, not a second meter: reads remain valid between detachment
        // by FlowCoordinator and the old screen's deferred free.
        self.noise_meter
            .as_ref()
            .expect("SentryNode must be ready")
            .clone()
    }

    pub(crate) fn sentry_mut(&mut self) -> &mut Sentry {
        &mut self.sentry
    }
}

#[godot_api]
impl SentryNode {
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
        let outcome = self.perform_queued_action();
        godot_print!(
            "{} acted after turn {finished_turn}: {} (noise {} / {})",
            self.sentry.name(),
            outcome.at("action"),
            outcome.at("noise"),
            outcome.at("max_noise"),
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
        self.noise_meter().bind().get_noise()
    }

    #[func]
    fn max_noise(&self) -> i32 {
        self.noise_meter().bind().get_max_noise()
    }

    #[func]
    fn health(&self) -> i32 {
        self.sentry.health()
    }

    #[func]
    fn max_health(&self) -> i32 {
        self.sentry.max_health()
    }

    #[func]
    fn is_defeated(&self) -> bool {
        self.sentry.is_defeated()
    }

    /// Applies damage from a played card. Returns actual health lost, clamped —
    /// mirrors `add_noise`'s contract.
    #[func]
    fn take_damage(&mut self, amount: i32) -> i32 {
        self.sentry.take_damage(amount)
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
        self.noise_meter().bind_mut().add_noise(noise)
    }

    /// Takes the sentry's turn and reports what it did:
    /// `{ok, sentry, action, noise_added, noise, max_noise, run_failed}`.
    ///
    /// `ok` is false when nothing is queued, the meter is already full or the
    /// screen is detached. In those cases neither intent nor noise changes.
    #[func]
    fn perform_queued_action(&mut self) -> VarDictionary {
        let meter = self.noise_meter();

        let action = if self.base().is_inside_tree()
            && !self.base().is_queued_for_deletion()
            && !meter.bind().is_at_cap()
            && !self.sentry.is_defeated()
        {
            self.sentry.perform_queued_action()
        } else {
            None
        };
        match action {
            Some(action) => {
                let mut meter = meter;
                let noise_added = meter.bind_mut().add_noise(action.noise);
                let noise = meter.bind().get_noise();
                let max_noise = meter.bind().get_max_noise();
                let run_failed = meter.bind().is_at_cap();

                vdict! {
                    "ok" => true,
                    "sentry" => self.sentry.name(),
                    "action" => action.name.as_str(),
                    "noise_added" => noise_added,
                    "noise" => noise,
                    "max_noise" => max_noise,
                    "run_failed" => run_failed,
                }
            }
            None => {
                let noise = meter.bind().get_noise();
                let max_noise = meter.bind().get_max_noise();
                let run_failed = meter.bind().is_at_cap();

                vdict! {
                    "ok" => false,
                    "sentry" => self.sentry.name(),
                    "action" => "",
                    "noise_added" => 0,
                    "noise" => noise,
                    "max_noise" => max_noise,
                    "run_failed" => run_failed,
                }
            }
        }
    }
}

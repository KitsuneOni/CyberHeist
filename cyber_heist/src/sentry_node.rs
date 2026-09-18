//! Thin Godot adapter for the sentry in `sentry.rs`.
//!
//! Holds one encounter's intent queue; noise belongs to NoiseMeterGlobal.
//! The scene connects `DrawPhase`'s `security_phase` signal to `on_security_phase`, so the sentry
//! takes its turn at the point the player ends theirs.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::noise_meter::NoiseMeter;
use crate::run_state_node::RunStateNode;
use crate::sentry::Sentry;

/// Where the coordinator keeps the run. Looked up the same way as the shared
/// meter, and optional for the same reason: the combat scene stays runnable on
/// its own, it just falls back to first-zone security when there is no run.
const RUN_STATE_PATH: &str = "/root/FlowCoordinator/RunState";

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
        let mut meter = self
            .base()
            .get_node_as::<NoiseMeter>("/root/NoiseMeterGlobal");

        // Which construct is on the other side of this encounter is decided by
        // where the player has got to on the contract, not by the scene: a
        // zone's boss is a harder build of the security around it.
        self.sentry = match self.encounter_profile() {
            Some((zone, is_boss)) => Sentry::for_encounter(zone, is_boss),
            None => Sentry::warden_7(),
        };

        // Resistance belongs to whoever is being faced, so it is set here and
        // lives on the one shared meter rather than in a second copy.
        meter
            .bind_mut()
            .set_resistance_percent(self.sentry.detection_resistance());
        self.noise_meter = Some(meter);

        // The name is data, so a scene can put a different construct in the
        // way without any rule changes.
        let name = self.sentry_name.to_string();
        if !name.is_empty() {
            self.sentry.set_name(&name);
        }
    }
}

impl SentryNode {
    /// `(zone, is_boss)` for the encounter being played, or `None` when there
    /// is no run behind this screen.
    fn encounter_profile(&self) -> Option<(usize, bool)> {
        let run_state = self
            .base()
            .try_get_node_as::<RunStateNode>(RUN_STATE_PATH)?;
        let profile = run_state.bind().active_encounter_profile();
        if !profile.at("ok").booleanize() {
            return None;
        }

        let zone = usize::try_from(profile.at("zone").to::<i64>()).unwrap_or(0);
        Some((zone, profile.at("is_boss").booleanize()))
    }

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

    /// What the queued action does beyond the noise, e.g. "-2 energy", or an
    /// empty string when it only makes noise. Announced with the intent, so a
    /// boss ability is something the player can plan around rather than a
    /// surprise after they have committed to ending the turn.
    #[func]
    fn queued_action_ability(&self) -> GString {
        match self.sentry.queued_action() {
            Some(action) => GString::from(&action.ability.describe()),
            None => GString::new(),
        }
    }

    /// Whether this construct has an action that does more than make noise.
    #[func]
    fn has_ability(&self) -> bool {
        self.sentry.has_ability()
    }

    /// How hard this construct resists the player lowering detection.
    #[func]
    fn detection_resistance(&self) -> i32 {
        self.sentry.detection_resistance()
    }

    /// Adds noise from something other than the sentry, such as a loud card.
    /// Returns how much actually went on the meter.
    #[func]
    fn add_noise(&mut self, noise: i32) -> i32 {
        self.noise_meter().bind_mut().add_noise(noise)
    }

    #[func]
    fn corruption(&self) -> i32 {
        self.sentry.corruption()
    }

    #[func]
    fn corruption_boost(&self) -> i32 {
        self.sentry.corruption_boost()
    }

    /// Takes the sentry's turn and reports what it did:
    /// `{ok, sentry, action, noise_added, noise, max_noise, run_failed}`.
    ///
    /// `ok` is false when nothing is queued, the meter is already full or the
    /// screen is detached. In those cases neither intent nor noise changes.
    #[func]
    fn perform_queued_action(&mut self) -> VarDictionary {
        let meter = self.noise_meter();

        let corruption_damage = self.sentry.resolve_corruption_tick();
        let corruption = self.sentry.corruption();

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
                    // Applied by the combat screen to the turn this one opens,
                    // since the player's energy belongs to DrawPhase.
                    "energy_drain" => action.ability.energy_drain(),
                    "ability" => action.ability.describe().as_str(),
                    "corruption_damage" => corruption_damage,
                    "corruption" => corruption,
                    "defeated" => self.sentry.is_defeated(),
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
                    "energy_drain" => 0,
                    "ability" => "",
                    "corruption_damage" => corruption_damage,
                    "corruption" => corruption,
                    "defeated" => self.sentry.is_defeated(),
                }
            }
        }
    }
}

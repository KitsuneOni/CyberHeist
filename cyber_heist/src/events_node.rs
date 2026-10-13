//! Thin Godot adapter for the random event model in `events.rs`.
//!
//! Holds the event currently on screen so that resolving a choice applies the
//! outcome exactly once, then hands the result back for the scene to display.
//! The credits it moves are also recorded on the run, so the end screen can
//! count what the run earned.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::PlayerState;
use crate::events::{EventDefinition, random_event};
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};

#[derive(GodotClass)]
#[class(base=Node)]
struct EventNode {
    /// The event currently being presented, if one has been rolled.
    current: Option<EventDefinition>,
    /// Whether the player has already committed to a choice for `current`.
    resolved: bool,
    base: Base<Node>,
}

#[godot_api]
impl INode for EventNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            current: None,
            resolved: false,
            base,
        }
    }
}

#[godot_api]
impl EventNode {
    /// Rolls a random event and returns it for display:
    /// `{ok, id, title, description, choices: [{label, preview}]}`.
    #[func]
    fn roll_event(&mut self) -> VarDictionary {
        let mut rng = rand::rng();
        let event = random_event(&mut rng);

        let mut choices: Array<VarDictionary> = Array::new();
        for choice in &event.choices {
            choices.push(&vdict! {
                "label" => choice.label.as_str(),
                "preview" => choice.preview.as_str(),
            });
        }

        let payload = vdict! {
            "ok" => true,
            "id" => event.id.as_str(),
            "title" => event.title.as_str(),
            "description" => event.description.as_str(),
            "choices" => &choices,
        };

        self.current = Some(event);
        self.resolved = false;
        payload
    }

    /// Commits to the choice at `index`, applies its outcome to the run, and
    /// returns what happened:
    /// `{ok, summary, credits, upgrade, credits_total}`.
    #[func]
    fn choose(&mut self, index: i64) -> VarDictionary {
        let Some(event) = self.current.as_ref() else {
            return failure("no event has been rolled yet");
        };
        if self.resolved {
            return failure("this event has already been resolved");
        }

        let Ok(index) = usize::try_from(index) else {
            return failure("choice index is invalid");
        };

        let mut rng = rand::rng();
        let Some(effect) = event.resolve(index, &mut rng) else {
            return failure("that choice is not available on this event");
        };

        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>("/root/PlayerStateGlobal")
        else {
            return failure("PlayerStateGlobal is not available to apply the outcome");
        };

        let credits_total = {
            let mut player_state = player_state.bind_mut();
            player_state.add_credits(effect.credits);
            if let Some(upgrade) = effect.upgrade.as_deref() {
                player_state.add_upgrade(GString::from(upgrade));
            }
            player_state.money()
        };
        self.record_credit_change_on_run(effect.credits);

        self.resolved = true;

        let upgrade = effect.upgrade.clone().unwrap_or_default();
        vdict! {
            "ok" => true,
            "summary" => effect.summary.as_str(),
            "credits" => effect.credits,
            "upgrade" => upgrade.as_str(),
            "credits_total" => credits_total,
        }
    }
}

impl EventNode {
    /// Tells the run about the credits this event moved, so they count
    /// towards what the run earned. Skipped when the event screen is open on
    /// its own with no run behind it, as `SentryNode` does.
    fn record_credit_change_on_run(&self, credits: i64) {
        let Some(mut run_state) = self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH)
        else {
            return;
        };
        run_state.bind_mut().record_credit_change(credits);
    }
}

fn failure(message: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => message,
    }
}

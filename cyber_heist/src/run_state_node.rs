//! Thin Godot adapter for the plain-Rust contract lifecycle model.

use godot::builtin::{VarDictionary, Variant};
use godot::prelude::*;

use crate::run_state::{RunState, RunStateError};

#[derive(GodotClass)]
#[class(base=Node)]
struct RunStateNode {
    state: RunState,
    base: Base<Node>,
}

#[godot_api]
impl INode for RunStateNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            state: RunState::new(),
            base,
        }
    }
}

#[godot_api]
impl RunStateNode {
    #[func]
    fn start_encounter(&mut self, encounter_id: GString, encounter_type: GString) -> VarDictionary {
        transition_result(
            self.state
                .start_encounter(&encounter_id.to_string(), &encounter_type.to_string()),
        )
    }

    #[func]
    fn complete_encounter(&mut self) -> VarDictionary {
        transition_result(self.state.complete_encounter())
    }

    #[func]
    fn report_caught(&mut self) -> VarDictionary {
        transition_result(self.state.report_caught())
    }

    #[func]
    fn finish_caught(&mut self) -> VarDictionary {
        transition_result(self.state.finish_caught())
    }

    #[func]
    fn snapshot(&self) -> VarDictionary {
        let active_encounter = match self.state.active_encounter() {
            Some(encounter) => Variant::from(vdict! {
                "id" => encounter.id(),
                "type" => encounter.encounter_type().as_str(),
            }),
            None => Variant::nil(),
        };

        vdict! {
            "phase" => self.state.phase().as_str(),
            "active_encounter" => active_encounter,
        }
    }
}

fn transition_result(result: Result<(), RunStateError>) -> VarDictionary {
    match result {
        Ok(()) => vdict! { "ok" => true },
        Err(error) => {
            let message = error.to_string();
            vdict! {
                "ok" => false,
                "error" => message.as_str(),
            }
        }
    }
}

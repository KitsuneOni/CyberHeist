//! Thin Godot adapter for the plain-Rust contract lifecycle model.

use godot::builtin::{VarDictionary, Variant};
use godot::prelude::*;

use crate::contract_map::{EncounterNode, EncounterSelection};
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
    fn selectable_encounters(&self) -> Array<VarDictionary> {
        let mut encounters = Array::new();
        for node in self.state.selectable_encounters() {
            encounters.push(&encounter_dictionary(node));
        }
        encounters
    }

    #[func]
    fn current_contract_node(&self) -> VarDictionary {
        encounter_dictionary(self.state.current_contract_node())
    }

    #[func]
    fn encounter_option(&self, node_id: i64) -> VarDictionary {
        let node_id = match u32::try_from(node_id) {
            Ok(node_id) => node_id,
            Err(_) => return transition_error("encounter identifier is invalid"),
        };
        match self.state.encounter_option(node_id) {
            Ok(selection) => selection_dictionary(selection),
            Err(error) => transition_error(&error.to_string()),
        }
    }

    #[func]
    fn select_encounter(&mut self, node_id: i64) -> VarDictionary {
        let node_id = match u32::try_from(node_id) {
            Ok(node_id) => node_id,
            Err(_) => return transition_error("encounter identifier is invalid"),
        };
        match self.state.select_encounter(node_id) {
            Ok(selection) => selection_dictionary(selection),
            Err(error) => transition_error(&error.to_string()),
        }
    }

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
            "active_encounter" => &active_encounter,
        }
    }
}

fn transition_result(result: Result<(), RunStateError>) -> VarDictionary {
    match result {
        Ok(()) => vdict! { "ok" => true },
        Err(error) => transition_error(&error.to_string()),
    }
}

fn encounter_dictionary(node: &EncounterNode) -> VarDictionary {
    vdict! {
        "id" => i64::from(node.id()),
        "type" => node.encounter_type().as_str(),
    }
}

fn selection_dictionary(selection: EncounterSelection) -> VarDictionary {
    vdict! {
        "ok" => true,
        "id" => i64::from(selection.node_id),
        "type" => selection.encounter_type.as_str(),
    }
}

fn transition_error(message: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => message,
    }
}

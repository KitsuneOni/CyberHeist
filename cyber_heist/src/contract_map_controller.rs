//! Thin Godot adapter around the pure Rust contract-map model.

use godot::prelude::*;

use crate::contract_map::{ContractMap, EncounterNode, EncounterSelection};

#[derive(GodotClass)]
#[class(base=Node)]
pub struct ContractMapController {
    map: ContractMap,
    base: Base<Node>,
}

#[godot_api]
impl INode for ContractMapController {
    fn init(base: Base<Node>) -> Self {
        Self {
            map: ContractMap::demo(),
            base,
        }
    }
}

#[godot_api]
impl ContractMapController {
    /// Returns every node the player may choose from the current map position.
    #[func]
    fn selectable_encounters(&self) -> Array<VarDictionary> {
        let mut encounters = Array::new();

        for node in self.map.selectable_encounters() {
            let encounter = encounter_dictionary(node);
            encounters.push(&encounter);
        }

        encounters
    }

    /// Returns the node the player most recently completed or selected.
    #[func]
    fn current_encounter(&self) -> VarDictionary {
        encounter_dictionary(self.map.current_encounter())
    }

    /// Commits to one reachable encounter. The returned dictionary always has
    /// `ok` and either encounter details or a player-readable `error`.
    #[func]
    fn select_encounter(&mut self, node_id: i64) -> VarDictionary {
        let requested_node = match u32::try_from(node_id) {
            Ok(node_id) => node_id,
            Err(_) => return error_dictionary("That encounter identifier is invalid."),
        };

        match self.map.select_encounter(requested_node) {
            Ok(selection) => selection_dictionary(selection),
            Err(error) => error_dictionary(&error.to_string()),
        }
    }

    /// Demo hook standing in for combat/event/shop completion. It lets the
    /// scene return to the map and exercise the next route choice.
    #[func]
    fn complete_current_encounter(&mut self) -> VarDictionary {
        match self.map.complete_current_encounter() {
            Ok(()) => success_dictionary(),
            Err(error) => error_dictionary(&error.to_string()),
        }
    }

    #[func]
    fn reset_demo_contract(&mut self) {
        self.map = ContractMap::demo();
    }
}

fn encounter_dictionary(node: &EncounterNode) -> VarDictionary {
    let mut encounter = VarDictionary::new();
    encounter.set("id", i64::from(node.id));
    encounter.set("encounter_type", node.encounter_type.as_str());
    encounter
}

fn selection_dictionary(selection: EncounterSelection) -> VarDictionary {
    let mut result = success_dictionary();
    result.set("id", i64::from(selection.node_id));
    result.set("encounter_type", selection.encounter_type.as_str());
    result
}

fn success_dictionary() -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("ok", true);
    result
}

fn error_dictionary(message: &str) -> VarDictionary {
    let mut result = VarDictionary::new();
    result.set("ok", false);
    result.set("error", message);
    result
}

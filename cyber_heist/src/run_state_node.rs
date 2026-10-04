//! Thin Godot adapter for the plain-Rust contract lifecycle model.

use godot::builtin::{VarDictionary, Variant};
use godot::prelude::*;

use crate::contract_map::{EncounterNode, EncounterSelection};
use crate::contract_offer::ContractOffer;
use crate::encounter_text::{
    all_encounter_types, all_node_statuses, encounter_type_text, node_status_text,
};
use crate::run_state::{RunState, RunStateError};

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct RunStateNode {
    state: RunState,
    base: Base<Node>,
}

#[godot_api]
impl INode for RunStateNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            // Every launch rolls a fresh contract.
            state: RunState::generated(&mut rand::rng()),
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

    /// Every node on the contract with its status, so the map screen can show
    /// what is done, where the player is, and what lies ahead.
    #[func]
    fn map_progress(&self) -> Array<VarDictionary> {
        let mut entries = Array::new();
        for entry in self.state.node_progress() {
            let type_text = encounter_type_text(entry.encounter_type);
            let status_text = node_status_text(entry.status);

            let mut connections: Array<i64> = Array::new();
            for next_node in &entry.connections {
                connections.push(i64::from(*next_node));
            }

            entries.push(&vdict! {
                "id" => i64::from(entry.node_id),
                "type" => entry.encounter_type.as_str(),
                "type_name" => type_text.name,
                "type_marker" => type_text.marker,
                "description" => type_text.description,
                "status" => entry.status.as_str(),
                "status_marker" => status_text.marker,
                "x" => entry.x as f64,
                "y" => entry.y as f64,
                "is_entry" => entry.is_entry,
                "is_current" => entry.is_current,
                "is_target" => entry.is_target,
                "is_boss" => entry.is_boss,
                "zone" => entry.zone as i64,
                "connections" => &connections,
            });
        }
        entries
    }

    /// The map key: what each encounter marker and status marker means.
    /// Returns `{encounter_types: [...], statuses: [...]}`.
    #[func]
    fn map_key(&self) -> VarDictionary {
        let mut types: Array<VarDictionary> = Array::new();
        for encounter_type in all_encounter_types() {
            let text = encounter_type_text(encounter_type);
            types.push(&vdict! {
                "type" => encounter_type.as_str(),
                "marker" => text.marker,
                "name" => text.name,
                "description" => text.description,
            });
        }

        let mut statuses: Array<VarDictionary> = Array::new();
        for status in all_node_statuses() {
            let text = node_status_text(status);
            statuses.push(&vdict! {
                "status" => status.as_str(),
                "marker" => text.marker,
                "description" => text.description,
            });
        }

        vdict! {
            "encounter_types" => &types,
            "statuses" => &statuses,
        }
    }

    /// Completed encounters out of the total, e.g. for "2 of 5 complete".
    #[func]
    fn contract_progress(&self) -> VarDictionary {
        let progress = self.state.progress();
        vdict! {
            "completed" => progress.completed as i64,
            "total" => progress.total as i64,
        }
    }

    /// Which zone the player is in, how many have been opened up, and how
    /// many the contract has, e.g. for "Zone 2 of 3".
    #[func]
    fn zone_progress(&self) -> VarDictionary {
        let progress = self.state.zone_progress();
        vdict! {
            "current_zone" => progress.current_zone as i64,
            "unlocked_zones" => progress.unlocked_zones as i64,
            "total_zones" => progress.total_zones as i64,
        }
    }

    /// The construct profile for the encounter being played: which zone it is
    /// in and whether it is that zone's boss. `{ok: false}` between encounters.
    ///
    /// Read by `SentryNode` when a combat screen opens, so the security the
    /// player faces is decided by where they are on the contract rather than
    /// by what the scene happens to have been saved with.
    #[func]
    pub(crate) fn active_encounter_profile(&self) -> VarDictionary {
        match self.state.active_encounter() {
            Some(encounter) => vdict! {
                "ok" => true,
                "zone" => encounter.zone() as i64,
                "is_boss" => encounter.is_boss(),
            },
            None => vdict! {
                "ok" => false,
                "zone" => 0_i64,
                "is_boss" => false,
            },
        }
    }

    /// The contracts this run can choose between, shortest first. Each is
    /// `{index, name, target, credit_reward, zone_count, encounter_count}`,
    /// where `index` is what `accept_contract_offer` takes.
    #[func]
    fn contract_offers(&self) -> Array<VarDictionary> {
        let mut offers = Array::new();
        for (index, offer) in self.state.contract_offers().iter().enumerate() {
            offers.push(&offer_dictionary(index, offer));
        }
        offers
    }

    /// Takes on the offer at `index` and rebuilds the contract map from it.
    /// `{ok: true}`, or `{ok: false, error}` for an index that is not on offer,
    /// outside the hub, or once an encounter on the contract has been entered.
    /// A rejection changes nothing.
    #[func]
    fn accept_contract_offer(&mut self, index: i64) -> VarDictionary {
        let index = match usize::try_from(index) {
            Ok(index) => index,
            Err(_) => return transition_error("contract offer index is invalid"),
        };
        transition_result(self.state.accept_contract_offer(index))
    }

    /// The contract the run took on: the same fields as a `contract_offers`
    /// entry plus `ok: true`. `{ok: false}` while the run is still on the
    /// contract it started with.
    #[func]
    fn accepted_contract_offer(&self) -> VarDictionary {
        let (Some(index), Some(offer)) = (
            self.state.accepted_offer_index(),
            self.state.accepted_offer(),
        ) else {
            return vdict! { "ok" => false };
        };

        let mut dictionary = offer_dictionary(index, offer);
        dictionary.set("ok", true);
        dictionary
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

/// One contract offer, as the selection and details screens read it.
fn offer_dictionary(index: usize, offer: &ContractOffer) -> VarDictionary {
    vdict! {
        "index" => index as i64,
        "name" => offer.name.as_str(),
        "target" => offer.target.as_str(),
        "credit_reward" => offer.credit_reward,
        "zone_count" => offer.shape.zone_count as i64,
        "encounter_count" => offer.encounter_count() as i64,
    }
}

fn selection_dictionary(selection: EncounterSelection) -> VarDictionary {
    vdict! {
        "ok" => true,
        "id" => i64::from(selection.node_id),
        "type" => selection.encounter_type.as_str(),
        "zone" => selection.zone as i64,
        "is_boss" => selection.encounter_type.is_boss(),
    }
}

fn transition_error(message: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => message,
    }
}

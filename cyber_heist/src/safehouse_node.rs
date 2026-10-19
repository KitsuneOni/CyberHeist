//! Thin Godot adapter for the safehouse model in `safehouse.rs`.
//!
//! Lives on the safehouse screen, as `ShopNode` does on the shop screen. The
//! visit belongs to the run (`RunState::safehouse_visit`), opened when the
//! player enters the node, so a rebuilt screen finds the safehouse already
//! used rather than offering it again.
//!
//! Lie Low borrows the shared `NoiseMeterGlobal` level and Upgrade borrows the
//! run's deck from `RunDeckNode`, and each hands it to one `RunState` call.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_database::CardDatabase;
use crate::card_text;
use crate::noise_meter::NoiseMeter;
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::safehouse::{LIE_LOW_NOISE_REDUCTION, lie_low_reduction};
use crate::shop_node::upgradable_entries;
use crate::{CARD_DATABASE_PATH, NOISE_METER_PATH, RUN_DECK_PATH, card_detail_dictionary, failure};

#[derive(GodotClass)]
#[class(base=Node)]
struct SafehouseNode {
    base: Base<Node>,
}

#[godot_api]
impl INode for SafehouseNode {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl SafehouseNode {
    /// Where the safehouse stands, for the screen: `{in_safehouse, choice,
    /// can_lie_low, can_upgrade, reduction, noise_removed, noise, max_noise}`.
    ///
    /// `choice` is `""` while both options are open, else `"lie_low"` or
    /// `"upgrade"`. `noise_removed` is what Lie Low would take off right now,
    /// which is less than `reduction` when noise is already low.
    #[func]
    fn status(&self) -> VarDictionary {
        let meter = self.noise_meter();
        let (noise, max_noise, noise_removed) = match &meter {
            Some(meter) => {
                let meter = meter.bind();
                (
                    meter.get_noise(),
                    meter.get_max_noise(),
                    lie_low_reduction(meter.level()),
                )
            }
            None => (0, 0, 0),
        };
        let visit = self
            .run_state()
            .and_then(|run_state| run_state.bind().safehouse_visit().copied());
        let can_lie_low = match (&visit, &meter) {
            (Some(visit), Some(meter)) => visit.can_lie_low(meter.bind().level()),
            _ => false,
        };

        vdict! {
            "in_safehouse" => visit.is_some(),
            "choice" => visit.and_then(|visit| visit.choice()).map_or("", |choice| choice.as_str()),
            "can_lie_low" => can_lie_low,
            "can_upgrade" => visit.is_some_and(|visit| visit.can_upgrade()),
            "reduction" => LIE_LOW_NOISE_REDUCTION,
            "noise_removed" => noise_removed,
            "noise" => noise,
            "max_noise" => max_noise,
        }
    }

    /// Lies low. Returns `{ok: true, noise_removed, noise}` with the noise
    /// afterwards, or `{ok: false, error}` with nothing changed.
    ///
    /// `error` is `already_used`, `nothing_to_lie_low` or `not_in_safehouse`
    /// from the safehouse's rules, or `no_noise_meter` / `no_run_state` when
    /// there is no run behind the screen.
    #[func]
    fn lie_low(&mut self) -> VarDictionary {
        let Some(mut noise_meter) = self.noise_meter() else {
            return failure("no_noise_meter");
        };
        let Some(mut run_state) = self.run_state() else {
            return failure("no_run_state");
        };

        let result = {
            let mut noise_meter = noise_meter.bind_mut();
            run_state
                .bind_mut()
                .lie_low_in_safehouse(noise_meter.level_mut())
        };

        match result {
            Ok(laid_low) => vdict! {
                "ok" => true,
                "noise_removed" => laid_low.noise_removed,
                "noise" => laid_low.noise_left,
            },
            Err(error) => failure(error.code()),
        }
    }

    /// Every card in the run's deck that has an upgrade, one entry per
    /// distinct card in the order each first appears: `{id, count, card,
    /// upgraded}`, where `card` and `upgraded` are card detail dictionaries.
    /// The safehouse's upgrade is free, so there is no price.
    #[func]
    fn upgradable_cards(&self) -> Array<VarDictionary> {
        let mut cards: Array<VarDictionary> = Array::new();
        let (Some(run_deck), Some(db)) = (
            self.base().try_get_node_as::<RunDeckNode>(RUN_DECK_PATH),
            self.card_database(),
        ) else {
            return cards;
        };
        let db = db.bind();

        for (id, count, card, upgraded) in upgradable_entries(run_deck.bind().owned_card_ids(), &db)
        {
            // Shown on their strong sides, the same as the shop's upgrades.
            let before = card_detail_dictionary(&card_text::describe(card, 0));
            let after = card_detail_dictionary(&card_text::describe(upgraded, 0));
            cards.push(&vdict! {
                "id" => id.as_str(),
                "count" => count,
                "card" => &before,
                "upgraded" => &after,
            });
        }

        cards
    }

    /// Upgrades one copy of `card_id` for free. Returns `{ok: true, from_id,
    /// to_id}`, or `{ok: false, error}` with nothing changed.
    ///
    /// `error` is `already_used`, `not_in_deck`, `not_upgradable` or
    /// `not_in_safehouse` from the safehouse's rules, or `no_run_deck` /
    /// `no_run_state` / `no_card_database` when there is no run.
    #[func]
    fn upgrade_card(&mut self, card_id: GString) -> VarDictionary {
        let card_id = card_id.to_string();
        let Some(mut run_deck) = self.base().try_get_node_as::<RunDeckNode>(RUN_DECK_PATH) else {
            return failure("no_run_deck");
        };
        let Some(mut run_state) = self.run_state() else {
            return failure("no_run_state");
        };
        let Some(db) = self.card_database() else {
            return failure("no_card_database");
        };
        let upgraded_id = db.bind().upgraded_id_of(&card_id);

        let result = {
            let mut run_deck = run_deck.bind_mut();
            run_state.bind_mut().upgrade_in_safehouse(
                run_deck.deck_mut(),
                &card_id,
                upgraded_id.as_deref(),
            )
        };

        match result {
            Ok(upgrade) => vdict! {
                "ok" => true,
                "from_id" => upgrade.from_id.as_str(),
                "to_id" => upgrade.to_id.as_str(),
            },
            Err(error) => failure(error.code()),
        }
    }
}

impl SafehouseNode {
    fn noise_meter(&self) -> Option<Gd<NoiseMeter>> {
        self.base().try_get_node_as::<NoiseMeter>(NOISE_METER_PATH)
    }

    fn run_state(&self) -> Option<Gd<RunStateNode>> {
        self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH)
    }

    fn card_database(&self) -> Option<Gd<CardDatabase>> {
        self.base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
    }
}

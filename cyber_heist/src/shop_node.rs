//! Thin Godot adapter for the shop model in `shop.rs`.
//!
//! Lives on the shop screen, as `EventNode` does on the event screen, and
//! holds that visit's [`ShopVisit`]. The stock is rolled once when the node
//! enters the tree, so it stays fixed for as long as the player is in the shop.
//!
//! Buying borrows the player's credits from `PlayerStateGlobal` and the run's
//! deck from `RunDeckNode` and hands both to [`ShopVisit::buy`] in one call, so
//! the price is taken and the card added together or not at all. The spend and
//! the card are then recorded on the run for the end screen, as rewards and
//! events do.
//!
//! The two services work the same way. Scrub Logs borrows the credits and the
//! shared `NoiseMeterGlobal` level for [`ShopVisit::scrub_logs`], and a reroll
//! borrows the credits for [`ShopVisit::reroll`]. Each spend is recorded on the
//! run like a purchase.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_data::Rarity;
use crate::card_database::CardDatabase;
use crate::card_text;
use crate::noise_meter::NoiseMeter;
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::shop::{SCRUB_LOGS_NOISE_REDUCTION, SCRUB_LOGS_PRICE, ShopVisit};
use crate::{PlayerState, RUN_DECK_PATH};

const CARD_DATABASE_PATH: &str = "/root/CardDatabaseGlobal";
const PLAYER_STATE_PATH: &str = "/root/PlayerStateGlobal";
const NOISE_METER_PATH: &str = "/root/NoiseMeterGlobal";

#[derive(GodotClass)]
#[class(base=Node)]
struct ShopNode {
    /// This visit's stock. `None` only if the card database was missing when
    /// the shop opened, in which case there is nothing to sell.
    visit: Option<ShopVisit>,
    base: Base<Node>,
}

#[godot_api]
impl INode for ShopNode {
    fn init(base: Base<Node>) -> Self {
        Self { visit: None, base }
    }

    fn ready(&mut self) {
        let Some(db) = self
            .base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
        else {
            godot_error!("ShopNode: no CardDatabaseGlobal, so the shop has no stock.");
            return;
        };

        let pool: Vec<(String, Rarity)> = db
            .bind()
            .all()
            .map(|card| (card.id.clone(), card.rarity))
            .collect();
        self.visit = Some(ShopVisit::open(&pool, &mut rand::rng()));
    }
}

#[godot_api]
impl ShopNode {
    /// The stock in display order, one dictionary per slot:
    /// `{slot, id, name, type, rarity, cost_text, noise_text, description,
    ///   keywords: [{label, explanation}], price, sold, affordable}`.
    ///
    /// `affordable` is measured against the player's credits right now, so
    /// the screen can disable a buy button the moment it stops being one.
    #[func]
    fn stock(&self) -> Array<VarDictionary> {
        let mut stock: Array<VarDictionary> = Array::new();
        let Some(visit) = self.visit.as_ref() else {
            return stock;
        };
        let Some(db) = self
            .base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
        else {
            return stock;
        };
        let db = db.bind();
        let credits = self.credits();

        for (index, slot) in visit.slots().iter().enumerate() {
            let Some(card) = db.get(slot.card_id()) else {
                continue;
            };
            // Bought outside an encounter, so a card is shown on its strong
            // side, the same as a card reward.
            let detail = card_text::describe(card, 0);

            let mut keywords: Array<VarDictionary> = Array::new();
            for keyword in &detail.keywords {
                keywords.push(&vdict! {
                    "label" => keyword.label.as_str(),
                    "explanation" => keyword.explanation.as_str(),
                });
            }

            stock.push(&vdict! {
                "slot" => index as i64,
                "id" => slot.card_id(),
                "name" => detail.name.as_str(),
                "type" => detail.card_type.as_str(),
                "rarity" => detail.rarity.as_str(),
                "cost_text" => detail.cost_text.as_str(),
                "noise_text" => detail.noise_text.as_str(),
                "description" => detail.description.as_str(),
                "keywords" => &keywords,
                "price" => slot.price(),
                "sold" => slot.is_sold(),
                "affordable" => visit.can_afford(index, credits),
            });
        }

        stock
    }

    /// Buys the card in `slot`. Returns `{ok: true, id, price, credits}` with
    /// the balance after paying, or `{ok: false, error}` with nothing changed.
    ///
    /// `error` is `no_such_slot`, `already_sold` or `not_enough_credits` from
    /// the shop's own rules, or `shop_closed` / `no_player_state` /
    /// `no_run_deck` when the screen has no run to buy for.
    #[func]
    fn buy(&mut self, slot: i64) -> VarDictionary {
        let Ok(slot) = usize::try_from(slot) else {
            return failure("no_such_slot");
        };
        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        else {
            return failure("no_player_state");
        };
        let Some(mut run_deck) = self.base().try_get_node_as::<RunDeckNode>(RUN_DECK_PATH) else {
            return failure("no_run_deck");
        };
        let Some(visit) = self.visit.as_mut() else {
            return failure("shop_closed");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            let mut run_deck = run_deck.bind_mut();
            visit.buy(slot, player_state.credits_mut(), run_deck.deck_mut())
        };

        match result {
            Ok(purchase) => {
                self.record_spend_on_run(purchase.price);
                self.record_card_added_on_run();
                vdict! {
                    "ok" => true,
                    "id" => purchase.card_id.as_str(),
                    "price" => purchase.price,
                    "credits" => purchase.credits_left,
                }
            }
            Err(error) => failure(error.code()),
        }
    }

    /// The Scrub Logs service as the screen shows it:
    /// `{price, reduction, sold, affordable, noise, max_noise}`.
    ///
    /// `affordable` is measured against the player's credits right now and is
    /// false once it is sold out. `noise` lets the screen show what it would
    /// scrub and hold the button back while there is nothing to scrub.
    #[func]
    fn scrub_logs_offer(&self) -> VarDictionary {
        let (sold, affordable) = match self.visit.as_ref() {
            Some(visit) => (
                visit.scrub_logs_sold(),
                visit.can_afford_scrub_logs(self.credits()),
            ),
            None => (false, false),
        };
        let (noise, max_noise) = match self.base().try_get_node_as::<NoiseMeter>(NOISE_METER_PATH) {
            Some(meter) => (meter.bind().get_noise(), meter.bind().get_max_noise()),
            None => (0, 0),
        };

        vdict! {
            "price" => SCRUB_LOGS_PRICE,
            "reduction" => SCRUB_LOGS_NOISE_REDUCTION,
            "sold" => sold,
            "affordable" => affordable,
            "noise" => noise,
            "max_noise" => max_noise,
        }
    }

    /// Buys Scrub Logs. Returns `{ok: true, price, noise_removed, noise,
    /// credits}` with the noise and balance afterwards, or `{ok: false, error}`
    /// with nothing changed.
    ///
    /// `error` is `already_sold`, `nothing_to_scrub` or `not_enough_credits`
    /// from the shop's own rules, or `shop_closed` / `no_player_state` /
    /// `no_noise_meter` when the screen has no run to scrub for.
    #[func]
    fn scrub_logs(&mut self) -> VarDictionary {
        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        else {
            return failure("no_player_state");
        };
        let Some(mut noise_meter) = self.base().try_get_node_as::<NoiseMeter>(NOISE_METER_PATH)
        else {
            return failure("no_noise_meter");
        };
        let Some(visit) = self.visit.as_mut() else {
            return failure("shop_closed");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            let mut noise_meter = noise_meter.bind_mut();
            visit.scrub_logs(player_state.credits_mut(), noise_meter.level_mut())
        };

        match result {
            Ok(scrubbed) => {
                self.record_spend_on_run(scrubbed.price);
                vdict! {
                    "ok" => true,
                    "price" => scrubbed.price,
                    "noise_removed" => scrubbed.noise_removed,
                    "noise" => scrubbed.noise_left,
                    "credits" => scrubbed.credits_left,
                }
            }
            Err(error) => failure(error.code()),
        }
    }

    /// The reroll as the screen shows it: `{price, affordable, available}`.
    ///
    /// `price` is what the next reroll in this visit costs. `available` is
    /// false once every card is sold, when there is nothing left to reroll;
    /// `affordable` is false then too.
    #[func]
    fn reroll_offer(&self) -> VarDictionary {
        let Some(visit) = self.visit.as_ref() else {
            return vdict! {
                "price" => 0,
                "affordable" => false,
                "available" => false,
            };
        };

        vdict! {
            "price" => visit.reroll_price(),
            "affordable" => visit.can_afford_reroll(self.credits()),
            "available" => visit.has_unsold_stock(),
        }
    }

    /// Rerolls every unsold card. Returns `{ok: true, price, next_price,
    /// credits}`, or `{ok: false, error}` with nothing changed. Read `stock()`
    /// again afterwards for the new cards.
    ///
    /// `error` is `nothing_to_reroll` or `not_enough_credits` from the shop's
    /// own rules, or `shop_closed` / `no_player_state` when the screen has no
    /// run to reroll for.
    #[func]
    fn reroll(&mut self) -> VarDictionary {
        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        else {
            return failure("no_player_state");
        };
        let Some(visit) = self.visit.as_mut() else {
            return failure("shop_closed");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            visit.reroll(player_state.credits_mut(), &mut rand::rng())
        };

        match result {
            Ok(reroll) => {
                self.record_spend_on_run(reroll.price);
                vdict! {
                    "ok" => true,
                    "price" => reroll.price,
                    "next_price" => reroll.next_price,
                    "credits" => reroll.credits_left,
                }
            }
            Err(error) => failure(error.code()),
        }
    }
}

impl ShopNode {
    fn credits(&self) -> i64 {
        match self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        {
            Some(player_state) => player_state.bind().money(),
            None => 0,
        }
    }

    /// Records a shop spend on the run. A spend is a loss, which the run does
    /// not count as earned, but it is recorded like every other credit change.
    /// Skipped when the shop screen is open on its own with no run behind it.
    fn record_spend_on_run(&self, price: i64) {
        if let Some(mut run_state) = self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH) {
            run_state.bind_mut().record_credit_change(-price);
        }
    }

    /// Records a bought card on the run, so the end screen counts it.
    fn record_card_added_on_run(&self) {
        if let Some(mut run_state) = self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH) {
            run_state.bind_mut().record_card_added();
        }
    }
}

fn failure(error: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => error,
    }
}

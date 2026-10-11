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

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_data::Rarity;
use crate::card_database::CardDatabase;
use crate::card_text;
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::shop::ShopVisit;
use crate::{PlayerState, RUN_DECK_PATH};

const CARD_DATABASE_PATH: &str = "/root/CardDatabaseGlobal";
const PLAYER_STATE_PATH: &str = "/root/PlayerStateGlobal";

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
                self.record_purchase_on_run(purchase.price);
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

    /// Tells the run about the spend and the card gained, so the end screen
    /// counts the card. A spend is a loss, which the run does not count as
    /// earned, but it is recorded like every other credit change. Skipped when
    /// the shop screen is open on its own with no run behind it.
    fn record_purchase_on_run(&self, price: i64) {
        let Some(mut run_state) = self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH)
        else {
            return;
        };
        let mut run_state = run_state.bind_mut();
        run_state.record_credit_change(-price);
        run_state.record_card_added();
    }
}

fn failure(error: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => error,
    }
}

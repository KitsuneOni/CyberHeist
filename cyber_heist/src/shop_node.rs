//! Thin Godot adapter for the shop model in `shop.rs`.
//!
//! Lives on the shop screen, as `EventNode` does on the event screen. The
//! visit itself belongs to the run (`RunState::open_shop_visit`), not to this
//! node: the stock is rolled the first time the screen opens during a shop
//! encounter, and a screen rebuilt during the same visit finds the same stock
//! and sold slots rather than rolling new ones.
//!
//! Buying borrows the player's credits from `PlayerStateGlobal` and the run's
//! deck from `RunDeckNode` and hands both to `RunState::buy_from_shop` in one
//! call, which takes the price, adds the card and records both on the run's
//! stats together or not at all.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_data::Rarity;
use crate::card_database::CardDatabase;
use crate::card_text;
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::{
    CARD_DATABASE_PATH, PLAYER_STATE_PATH, PlayerState, RUN_DECK_PATH, card_detail_dictionary,
    failure,
};

#[derive(GodotClass)]
#[class(base=Node)]
struct ShopNode {
    base: Base<Node>,
}

#[godot_api]
impl INode for ShopNode {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    /// Opens the run's shop visit, rolling its stock if this is the first time
    /// the screen has opened during this shop encounter.
    fn ready(&mut self) {
        let Some(mut run_state) = self.run_state() else {
            godot_error!("ShopNode: no RunState, so there is no shop visit to open.");
            return;
        };
        let Some(db) = self.card_database() else {
            godot_error!("ShopNode: no CardDatabaseGlobal, so the shop has no stock.");
            return;
        };

        let pool: Vec<(String, Rarity)> = db
            .bind()
            .all()
            .map(|card| (card.id.clone(), card.rarity))
            .collect();
        if let Err(error) = run_state.bind_mut().open_shop_visit(&pool) {
            godot_error!("ShopNode: {error}");
        }
    }
}

#[godot_api]
impl ShopNode {
    /// The stock in display order, one dictionary per slot: the card's
    /// details (see `card_detail_dictionary`) plus `{slot, id, price, sold,
    /// affordable}`. `slot` is the position to pass to `buy`.
    ///
    /// `affordable` is measured against the player's credits right now, so
    /// the screen can disable a buy button the moment it stops being one.
    #[func]
    fn stock(&self) -> Array<VarDictionary> {
        let mut stock: Array<VarDictionary> = Array::new();
        let (Some(run_state), Some(db)) = (self.run_state(), self.card_database()) else {
            return stock;
        };
        let run_state = run_state.bind();
        let Some(visit) = run_state.shop_visit() else {
            return stock;
        };
        let db = db.bind();
        let credits = self.credits();

        for (index, slot) in visit.slots().iter().enumerate() {
            // Every stocked id came from the database, so this only skips a
            // card if the database has changed since. The screen matches rows
            // by `slot`, not by position in this list, so a skipped card does
            // not shift the others.
            let Some(card) = db.get(slot.card_id()) else {
                continue;
            };
            // Bought outside an encounter, so a card is shown on its strong
            // side, the same as a card reward.
            let mut entry = card_detail_dictionary(&card_text::describe(card, 0));
            entry.set("slot", index as i64);
            entry.set("id", slot.card_id());
            entry.set("price", slot.price());
            entry.set("sold", slot.is_sold());
            entry.set("affordable", visit.can_afford(index, credits));
            stock.push(&entry);
        }

        stock
    }

    /// Buys the card in `slot`. Returns `{ok: true, id, price, credits}` with
    /// the balance after paying, or `{ok: false, error}` with nothing changed.
    ///
    /// `error` is `no_such_slot`, `already_sold`, `not_enough_credits` or
    /// `not_in_shop` from the shop's own rules, or `no_player_state` /
    /// `no_run_deck` / `no_run_state` when the screen has no run to buy for.
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
        let Some(mut run_state) = self.run_state() else {
            return failure("no_run_state");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            let mut run_deck = run_deck.bind_mut();
            run_state.bind_mut().buy_from_shop(
                slot,
                player_state.credits_mut(),
                run_deck.deck_mut(),
            )
        };

        match result {
            Ok(purchase) => vdict! {
                "ok" => true,
                "id" => purchase.card_id.as_str(),
                "price" => purchase.price,
                "credits" => purchase.credits_left,
            },
            Err(error) => failure(error.code()),
        }
    }
}

impl ShopNode {
    fn run_state(&self) -> Option<Gd<RunStateNode>> {
        self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH)
    }

    fn card_database(&self) -> Option<Gd<CardDatabase>> {
        self.base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
    }

    fn credits(&self) -> i64 {
        match self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        {
            Some(player_state) => player_state.bind().money(),
            None => 0,
        }
    }
}

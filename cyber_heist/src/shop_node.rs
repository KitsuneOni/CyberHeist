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
//!
//! The two services work the same way. Scrub Logs borrows the credits and the
//! shared `NoiseMeterGlobal` level for `RunState::scrub_logs_in_shop`, and a
//! reroll borrows the credits for `RunState::reroll_shop`. Each records its
//! spend on the run in the same call.

use godot::builtin::VarDictionary;
use godot::prelude::*;
use std::collections::HashMap;

use crate::card_data::{CardData, Rarity};
use crate::card_database::CardDatabase;
use crate::card_text;
use crate::noise_meter::NoiseMeter;
use crate::progression_node::{PROGRESSION_PATH, ProgressionNode};
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::shop::{self, SCRUB_LOGS_NOISE_REDUCTION, SCRUB_LOGS_PRICE};
use crate::{
    CARD_DATABASE_PATH, NOISE_METER_PATH, PLAYER_STATE_PATH, PlayerState, RUN_DECK_PATH,
    card_detail_dictionary, failure,
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

        let all: Vec<(String, Rarity)> = db
            .bind()
            .all()
            .map(|card| (card.id.clone(), card.rarity))
            .collect();
        let pool = match self
            .base()
            .try_get_node_as::<ProgressionNode>(PROGRESSION_PATH)
        {
            Some(node) => node.bind().progression().open_pool(all),
            None => all,
        };
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

    /// The Scrub Logs service as the screen shows it:
    /// `{price, reduction, sold, affordable, noise, max_noise}`.
    ///
    /// `affordable` is whether it can be bought right now: not sold out, the
    /// player's credits cover it, and it would actually lower the noise. The
    /// screen enables the button and colours the price by it alone.
    #[func]
    fn scrub_logs_offer(&self) -> VarDictionary {
        let meter = self.noise_meter();
        let (noise, max_noise) = match &meter {
            Some(meter) => (meter.bind().get_noise(), meter.bind().get_max_noise()),
            None => (0, 0),
        };
        let (sold, affordable) = match (self.run_state(), &meter) {
            (Some(run_state), Some(meter)) => match run_state.bind().shop_visit() {
                Some(visit) => (
                    visit.scrub_logs_sold(),
                    visit.can_buy_scrub_logs(self.credits(), meter.bind().level()),
                ),
                None => (false, false),
            },
            _ => (false, false),
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
    /// `error` is `already_sold`, `nothing_to_scrub`, `not_enough_credits` or
    /// `not_in_shop` from the shop's own rules, or `no_player_state` /
    /// `no_noise_meter` / `no_run_state` when the screen has no run to scrub
    /// for.
    #[func]
    fn scrub_logs(&mut self) -> VarDictionary {
        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        else {
            return failure("no_player_state");
        };
        let Some(mut noise_meter) = self.noise_meter() else {
            return failure("no_noise_meter");
        };
        let Some(mut run_state) = self.run_state() else {
            return failure("no_run_state");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            let mut noise_meter = noise_meter.bind_mut();
            run_state
                .bind_mut()
                .scrub_logs_in_shop(player_state.credits_mut(), noise_meter.level_mut())
        };

        match result {
            Ok(scrubbed) => vdict! {
                "ok" => true,
                "price" => scrubbed.price,
                "noise_removed" => scrubbed.noise_removed,
                "noise" => scrubbed.noise_left,
                "credits" => scrubbed.credits_left,
            },
            Err(error) => failure(error.code()),
        }
    }

    /// The reroll as the screen shows it: `{price, affordable, available}`.
    ///
    /// `price` is what the next reroll in this visit costs. `available` is
    /// false when a reroll would change nothing: every card is sold, or the
    /// pool has no card outside the stock. `affordable` is whether it can be
    /// bought right now, so it is false then too.
    #[func]
    fn reroll_offer(&self) -> VarDictionary {
        let unavailable = vdict! {
            "price" => 0,
            "affordable" => false,
            "available" => false,
        };
        let Some(run_state) = self.run_state() else {
            return unavailable;
        };
        let run_state = run_state.bind();
        let Some(visit) = run_state.shop_visit() else {
            return unavailable;
        };

        vdict! {
            "price" => visit.reroll_price(),
            "affordable" => visit.can_buy_reroll(self.credits()),
            "available" => visit.has_something_to_reroll(),
        }
    }

    /// Rerolls every unsold card. Returns `{ok: true, price, next_price,
    /// credits}`, or `{ok: false, error}` with nothing changed. Read `stock()`
    /// again afterwards for the new cards.
    ///
    /// `error` is `nothing_to_reroll`, `not_enough_credits` or `not_in_shop`
    /// from the shop's own rules, or `no_player_state` / `no_run_state` when
    /// the screen has no run to reroll for.
    #[func]
    fn reroll(&mut self) -> VarDictionary {
        let Some(mut player_state) = self
            .base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
        else {
            return failure("no_player_state");
        };
        let Some(mut run_state) = self.run_state() else {
            return failure("no_run_state");
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            run_state.bind_mut().reroll_shop(player_state.credits_mut())
        };

        match result {
            Ok(reroll) => vdict! {
                "ok" => true,
                "price" => reroll.price,
                "next_price" => reroll.next_price,
                "credits" => reroll.credits_left,
            },
            Err(error) => failure(error.code()),
        }
    }
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
        let credits = self.credits();

        for (id, count, card, upgraded) in upgradable_entries(run_deck.bind().owned_card_ids(), &db)
        {
            let price = shop::upgrade_price_for(card.rarity);
            // Shown on their strong sides, the same as stock and rewards.
            let before = card_detail_dictionary(&card_text::describe(card, 0));
            let after = card_detail_dictionary(&card_text::describe(upgraded, 0));
            cards.push(&vdict! {
                "id" => id.as_str(),
                "count" => count,
                "price" => price,
                "affordable" => credits >= price,
                "card" => &before,
                "upgraded" => &after,
            });
        }

        cards
    }
    #[func]
    fn upgrade_card(&mut self, card_id: GString) -> VarDictionary {
        let card_id = card_id.to_string();
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
        let Some(db) = self.card_database() else {
            return failure("no_card_database");
        };

        if run_state.bind().shop_visit().is_none() {
            return failure("not_in_shop");
        }

        let (upgraded_id, rarity) = {
            let db = db.bind();
            let Some(upgraded_id) = db.upgraded_id_of(&card_id) else {
                return failure("not_upgradable");
            };
            let Some(card) = db.get(&card_id) else {
                return failure("not_upgradable");
            };
            (upgraded_id, card.rarity)
        };

        let result = {
            let mut player_state = player_state.bind_mut();
            let mut run_deck = run_deck.bind_mut();
            run_state.bind_mut().upgrade_card_in_shop(
                player_state.credits_mut(),
                run_deck.deck_mut(),
                &card_id,
                &upgraded_id,
                rarity,
            )
        };

        match result {
            Ok(upgrade) => vdict! {
                "ok" => true,
                "from_id" => upgrade.from_id.as_str(),
                "to_id" => upgrade.to_id.as_str(),
                "price" => upgrade.price,
                "credits" => upgrade.credits_left,
            },
            Err(error) => failure(error.code()),
        }
    }

    /// The card removal as the screen shows it: `{price, sold, affordable}`.
    /// `affordable` is whether it can be used right now (not already used this
    /// visit, credits cover it, and the deck would not be left empty).
    #[func]
    fn removal_offer(&self) -> VarDictionary {
        let deck_len = self
            .base()
            .try_get_node_as::<RunDeckNode>(RUN_DECK_PATH)
            .map_or(0, |run_deck| run_deck.bind().owned_card_ids().len());
        let (sold, affordable) = match self.run_state() {
            Some(run_state) => match run_state.bind().shop_visit() {
                Some(visit) => (
                    visit.removal_used(),
                    visit.can_remove_card(self.credits(), deck_len),
                ),
                None => (false, false),
            },
            None => (false, false),
        };

        vdict! {
            "price" => shop::REMOVE_CARD_PRICE,
            "sold" => sold,
            "affordable" => affordable,
        }
    }

    /// Every card in the run's deck, one entry per distinct card, in the order
    /// each first appears: `{id, count, card}`, where `card` is a card detail
    /// dictionary. Curses and upgraded cards are included.
    #[func]
    fn removable_cards(&self) -> Array<VarDictionary> {
        let mut cards: Array<VarDictionary> = Array::new();
        let (Some(run_deck), Some(db)) = (
            self.base().try_get_node_as::<RunDeckNode>(RUN_DECK_PATH),
            self.card_database(),
        ) else {
            return cards;
        };
        let db = db.bind();

        let mut order: Vec<String> = Vec::new();
        let mut counts: HashMap<String, i64> = HashMap::new();
        for id in run_deck.bind().owned_card_ids() {
            let count = counts.entry(id.clone()).or_insert(0);
            if *count == 0 {
                order.push(id);
            }
            *count += 1;
        }

        for id in order {
            let Some(card) = db.get(&id) else {
                continue;
            };
            let detail = card_detail_dictionary(&card_text::describe(card, 0));
            cards.push(&vdict! {
                "id" => id.as_str(),
                "count" => counts[&id],
                "card" => &detail,
            });
        }

        cards
    }

    /// Removes one copy of `card_id`. Returns `{ok: true, id, price, credits}`,
    /// or `{ok: false, error}` with nothing changed.
    ///
    /// `error` is `already_sold`, `deck_too_small`, `not_enough_credits`,
    /// `not_in_deck` or `not_in_shop` from the shop's rules, or
    /// `no_player_state` / `no_run_deck` / `no_run_state` when there is no run.
    #[func]
    fn remove_card(&mut self, card_id: GString) -> VarDictionary {
        let card_id = card_id.to_string();
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
            run_state.bind_mut().remove_card_in_shop(
                &card_id,
                player_state.credits_mut(),
                run_deck.deck_mut(),
            )
        };

        match result {
            Ok(removed) => vdict! {
                "ok" => true,
                "id" => removed.card_id.as_str(),
                "price" => removed.price,
                "credits" => removed.credits_left,
            },
            Err(error) => failure(error.code()),
        }
    }
}

impl ShopNode {
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

/// Every distinct card in `owned` that has an upgrade, in the order each first
/// appears: `(id, copies, card, upgraded card)`. Shared by the shop's paid
/// upgrade and the safehouse's free one, so both offer the same cards.
pub(crate) fn upgradable_entries(
    owned: Vec<String>,
    db: &CardDatabase,
) -> Vec<(String, i64, &CardData, &CardData)> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for id in owned {
        let count = counts.entry(id.clone()).or_insert(0);
        if *count == 0 {
            order.push(id);
        }
        *count += 1;
    }

    order
        .into_iter()
        .filter_map(|id| {
            let upgraded_id = db.upgraded_id_of(&id)?;
            let card = db.get(&id)?;
            let upgraded = db.get(&upgraded_id)?;
            let count = counts[&id];
            Some((id, count, card, upgraded))
        })
        .collect()
}

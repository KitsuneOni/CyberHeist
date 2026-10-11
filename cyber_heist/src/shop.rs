//! The dark web shop a player can visit between encounters (Trello #38).
//!
//! Covers two of its stories:
//!
//! - **S1, shop stock and pricing (#104):** entering a shop node rolls a fixed
//!   stock of [`STOCK_SIZE`] cards, each priced by its rarity. The stock stays
//!   the same for the whole visit; a bought slot is marked sold rather than
//!   removed, so the other cards keep their places.
//! - **S2, buying with credits (#105):** [`ShopVisit::buy`] is the one
//!   operation that spends credits. It checks the slot and the balance first
//!   and only then takes the price and adds the card to the run's deck, so a
//!   refused purchase changes nothing and credits can never go negative.
//!
//! A [`ShopVisit`] is the state of one visit. Later shop stories extend it
//! rather than restructure it:
//!
//! - A once-per-visit **Scrub Logs** service (#106) is another paid action on
//!   the visit: a `scrub_logs_used` flag beside `slots`, and a method that
//!   charges through [`charge`] before lowering noise.
//! - A paid **reroll** of unsold stock (#107) charges through [`charge`] too,
//!   then refills only the slots still for sale with [`roll_slots`]. Sold slots
//!   stay sold, which is why a slot carries its own sold state instead of
//!   being taken out of the stock.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `ShopNode` in `shop_node.rs` is the Godot side.

use rand::Rng;
use rand::seq::SliceRandom;

use crate::card_data::Rarity;
use crate::run_deck::RunDeck;

/// How many cards a shop has for sale on each visit.
pub const STOCK_SIZE: usize = 5;

/// What a card of `rarity` costs in the shop, in credits.
///
/// The rarer the card, the more it costs. A zone 1 combat pays 25 credits and
/// an elite 50 (see `encounter_reward.rs`), so a common card is a couple of
/// fights' pay and a rare one takes saving up for.
pub fn price_for(rarity: Rarity) -> i64 {
    match rarity {
        Rarity::Common => 50,
        Rarity::Uncommon => 75,
        Rarity::Rare => 100,
        Rarity::Legendary => 150,
    }
}

/// One place in the shop's stock: a card, its price and whether it has gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopSlot {
    card_id: String,
    price: i64,
    sold: bool,
}

impl ShopSlot {
    fn for_sale(card_id: String, rarity: Rarity) -> Self {
        Self {
            card_id,
            price: price_for(rarity),
            sold: false,
        }
    }

    pub fn card_id(&self) -> &str {
        &self.card_id
    }

    pub fn price(&self) -> i64 {
        self.price
    }

    pub fn is_sold(&self) -> bool {
        self.sold
    }
}

/// A card the player has just bought, for the screen to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purchase {
    pub card_id: String,
    pub price: i64,
    /// The player's credits after paying.
    pub credits_left: i64,
}

/// Why a purchase was refused. A refusal never changes credits, the deck or
/// the stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopError {
    /// There is no slot with that index in this shop.
    NoSuchSlot,
    /// The card in that slot has already been bought this visit.
    AlreadySold,
    /// The player has fewer credits than the price.
    NotEnoughCredits,
}

impl ShopError {
    /// A stable name for the Godot side to report and test against.
    pub fn code(self) -> &'static str {
        match self {
            ShopError::NoSuchSlot => "no_such_slot",
            ShopError::AlreadySold => "already_sold",
            ShopError::NotEnoughCredits => "not_enough_credits",
        }
    }
}

/// Everything about one visit to a shop node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopVisit {
    slots: Vec<ShopSlot>,
}

impl ShopVisit {
    /// Opens a shop with up to [`STOCK_SIZE`] distinct cards from `pool`, each
    /// priced by its rarity. A pool with fewer distinct cards than that gives
    /// a smaller stock rather than repeating a card.
    pub fn open(pool: &[(String, Rarity)], rng: &mut impl Rng) -> Self {
        Self {
            slots: roll_slots(pool, STOCK_SIZE, rng),
        }
    }

    /// The stock in display order, sold slots included.
    pub fn slots(&self) -> &[ShopSlot] {
        &self.slots
    }

    /// Whether `credits` would cover the card in `slot`. False for a sold or
    /// missing slot, since there is nothing there to afford.
    pub fn can_afford(&self, slot: usize, credits: i64) -> bool {
        match self.slots.get(slot) {
            Some(slot) => !slot.sold && credits >= slot.price,
            None => false,
        }
    }

    /// Buys the card in `slot`: takes its exact price from `credits`, adds the
    /// card to `deck` and marks the slot sold.
    ///
    /// Every check happens before anything changes, so a refusal leaves the
    /// credits, the deck and the stock exactly as they were.
    pub fn buy(
        &mut self,
        slot: usize,
        credits: &mut i64,
        deck: &mut RunDeck,
    ) -> Result<Purchase, ShopError> {
        let chosen = self.slots.get_mut(slot).ok_or(ShopError::NoSuchSlot)?;
        if chosen.sold {
            return Err(ShopError::AlreadySold);
        }

        charge(credits, chosen.price)?;
        deck.add(&chosen.card_id);
        chosen.sold = true;

        Ok(Purchase {
            card_id: chosen.card_id.clone(),
            price: chosen.price,
            credits_left: *credits,
        })
    }
}

/// Takes `price` from `credits`, or refuses without touching them when the
/// balance does not cover it. The single place the shop spends credits, so
/// every paid action shares the same never-negative rule.
fn charge(credits: &mut i64, price: i64) -> Result<(), ShopError> {
    if *credits < price {
        return Err(ShopError::NotEnoughCredits);
    }
    *credits -= price;
    Ok(())
}

/// Picks up to `count` distinct cards from `pool` and prices each for sale.
fn roll_slots(pool: &[(String, Rarity)], count: usize, rng: &mut impl Rng) -> Vec<ShopSlot> {
    let mut candidates: Vec<(String, Rarity)> = pool.to_vec();
    // Sorted before shuffling so the same seed always gives the same stock,
    // whatever order the card database handed the pool over in.
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates.dedup_by(|a, b| a.0 == b.0);
    candidates.shuffle(rng);
    candidates.truncate(count);

    candidates
        .into_iter()
        .map(|(card_id, rarity)| ShopSlot::for_sale(card_id, rarity))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::{HashMap, HashSet};

    fn pool() -> Vec<(String, Rarity)> {
        [
            ("strike", Rarity::Common),
            ("shield", Rarity::Common),
            ("trojan", Rarity::Common),
            ("malware", Rarity::Common),
            ("phish", Rarity::Uncommon),
            ("rootkit", Rarity::Uncommon),
            ("zero_day", Rarity::Rare),
            ("wannacry", Rarity::Rare),
            ("stuxnet", Rarity::Legendary),
        ]
        .iter()
        .map(|(id, rarity)| (id.to_string(), *rarity))
        .collect()
    }

    /// A shop whose one slot sells `card_id` for exactly `price`, for the
    /// buying scenarios that name a price.
    fn shop_selling(card_id: &str, price: i64) -> ShopVisit {
        ShopVisit {
            slots: vec![ShopSlot {
                card_id: card_id.to_string(),
                price,
                sold: false,
            }],
        }
    }

    // --- S1: shop stock and pricing ---------------------------------------

    #[test]
    fn a_shop_opens_with_five_cards_for_sale() {
        let shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(1));

        assert_eq!(shop.slots().len(), STOCK_SIZE);
        assert_eq!(STOCK_SIZE, 5);
        assert!(shop.slots().iter().all(|slot| !slot.is_sold()));
    }

    #[test]
    fn every_card_is_priced_by_its_rarity() {
        let pool = pool();
        let rarity_of: HashMap<&str, Rarity> = pool
            .iter()
            .map(|(id, rarity)| (id.as_str(), *rarity))
            .collect();
        for seed in 0..50 {
            let shop = ShopVisit::open(&pool, &mut StdRng::seed_from_u64(seed));
            for slot in shop.slots() {
                let rarity = rarity_of[slot.card_id()];
                assert_eq!(slot.price(), price_for(rarity), "seed {seed}");
            }
        }
    }

    #[test]
    fn rarer_cards_cost_more() {
        assert!(price_for(Rarity::Common) < price_for(Rarity::Uncommon));
        assert!(price_for(Rarity::Uncommon) < price_for(Rarity::Rare));
        assert!(price_for(Rarity::Rare) < price_for(Rarity::Legendary));
    }

    #[test]
    fn stock_comes_from_the_pool_and_never_repeats_a_card() {
        let pool = pool();
        let ids: HashSet<&str> = pool.iter().map(|(id, _)| id.as_str()).collect();
        for seed in 0..50 {
            let shop = ShopVisit::open(&pool, &mut StdRng::seed_from_u64(seed));
            let stocked: HashSet<&str> = shop.slots().iter().map(|slot| slot.card_id()).collect();

            assert_eq!(stocked.len(), STOCK_SIZE, "seed {seed}: repeated a card");
            assert!(stocked.is_subset(&ids), "seed {seed}: {stocked:?}");
        }
    }

    #[test]
    fn different_visits_offer_different_choices() {
        let stocks: HashSet<Vec<String>> = (0..20)
            .map(|seed| {
                let shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(seed));
                shop.slots()
                    .iter()
                    .map(|slot| slot.card_id().to_string())
                    .collect()
            })
            .collect();

        assert!(stocks.len() > 1, "every visit stocked the same cards");
    }

    #[test]
    fn the_same_seed_stocks_the_same_shop_whatever_order_the_pool_is_in() {
        let mut reversed = pool();
        reversed.reverse();

        let forwards = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(7));
        let backwards = ShopVisit::open(&reversed, &mut StdRng::seed_from_u64(7));

        assert_eq!(forwards, backwards);
    }

    #[test]
    fn a_small_pool_gives_a_smaller_stock_rather_than_repeats() {
        let pool = vec![
            ("strike".to_string(), Rarity::Common),
            ("strike".to_string(), Rarity::Common),
            ("vpn".to_string(), Rarity::Common),
        ];

        let shop = ShopVisit::open(&pool, &mut StdRng::seed_from_u64(3));

        assert_eq!(shop.slots().len(), 2);
    }

    #[test]
    fn an_empty_pool_opens_an_empty_shop() {
        let shop = ShopVisit::open(&[], &mut StdRng::seed_from_u64(4));
        assert!(shop.slots().is_empty());
    }

    /// S1 scenario 2: buying a card marks that slot sold and leaves the other
    /// cards exactly where they were.
    #[test]
    fn buying_marks_only_that_slot_sold_and_keeps_the_rest_of_the_stock() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(5));
        let before = shop.slots().to_vec();
        let mut credits = 1_000;
        let mut deck = RunDeck::default();

        shop.buy(2, &mut credits, &mut deck).expect("affordable");

        assert_eq!(shop.slots().len(), before.len());
        for (index, (now, was)) in shop.slots().iter().zip(&before).enumerate() {
            assert_eq!(now.card_id(), was.card_id(), "slot {index} changed card");
            assert_eq!(now.price(), was.price(), "slot {index} changed price");
            assert_eq!(now.is_sold(), index == 2, "slot {index} sold state");
        }
    }

    #[test]
    fn a_sold_slot_cannot_be_bought_again() {
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = 500;
        let mut deck = RunDeck::default();
        shop.buy(0, &mut credits, &mut deck)
            .expect("first purchase");

        let again = shop.buy(0, &mut credits, &mut deck);

        assert_eq!(again, Err(ShopError::AlreadySold));
        assert_eq!(credits, 425, "the second attempt charged nothing");
        assert_eq!(deck.count_of("rootkit"), 1, "and added nothing");
        assert!(!shop.can_afford(0, credits), "a sold slot is not on offer");
    }

    // --- S2: buying with credits ------------------------------------------

    /// S2 scenario 1: 120 credits buy a 75 credit card, leaving 45, and the
    /// card goes into the run's deck.
    #[test]
    fn an_affordable_purchase_takes_the_exact_price_and_adds_the_card() {
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = 120;
        let mut deck = RunDeck::default();

        let purchase = shop.buy(0, &mut credits, &mut deck).expect("affordable");

        assert_eq!(credits, 45);
        assert_eq!(deck.count_of("rootkit"), 1);
        assert_eq!(
            purchase,
            Purchase {
                card_id: "rootkit".to_string(),
                price: 75,
                credits_left: 45,
            }
        );
        assert!(shop.slots()[0].is_sold());
    }

    /// S2 scenario 2: with 40 credits a 75 credit card cannot be bought, and
    /// trying anyway changes neither the credits, the deck nor the stock.
    #[test]
    fn an_unaffordable_purchase_changes_nothing() {
        let mut shop = shop_selling("rootkit", 75);
        let stock_before = shop.clone();
        let mut credits = 40;
        let mut deck = RunDeck::default();
        deck.add("strike");

        assert!(!shop.can_afford(0, credits));
        let result = shop.buy(0, &mut credits, &mut deck);

        assert_eq!(result, Err(ShopError::NotEnoughCredits));
        assert_eq!(credits, 40);
        assert_eq!(deck.card_ids(), ["strike"]);
        assert_eq!(shop, stock_before, "the slot is still for sale");
    }

    #[test]
    fn exactly_enough_credits_is_enough_and_leaves_zero() {
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = 75;
        let mut deck = RunDeck::default();

        assert!(shop.can_afford(0, credits));
        shop.buy(0, &mut credits, &mut deck)
            .expect("exactly enough");

        assert_eq!(credits, 0);
    }

    #[test]
    fn credits_never_go_negative_even_from_a_debt() {
        // A caught fine can leave the balance below zero. Nothing is
        // affordable then, and trying must not dig the hole deeper.
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = -30;
        let mut deck = RunDeck::default();

        assert!(!shop.can_afford(0, credits));
        assert_eq!(
            shop.buy(0, &mut credits, &mut deck),
            Err(ShopError::NotEnoughCredits)
        );
        assert_eq!(credits, -30);
    }

    #[test]
    fn spending_down_never_takes_the_balance_below_zero() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(6));
        let mut credits = 160;
        let mut deck = RunDeck::default();

        for slot in 0..shop.slots().len() {
            let affordable = shop.can_afford(slot, credits);
            let result = shop.buy(slot, &mut credits, &mut deck);
            assert_eq!(result.is_ok(), affordable, "slot {slot}");
            assert!(credits >= 0, "slot {slot} left {credits}");
        }
    }

    #[test]
    fn a_slot_that_does_not_exist_is_refused() {
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = 500;
        let mut deck = RunDeck::default();

        assert_eq!(
            shop.buy(5, &mut credits, &mut deck),
            Err(ShopError::NoSuchSlot)
        );
        assert!(!shop.can_afford(5, credits));
        assert_eq!(credits, 500);
        assert!(deck.is_empty());
    }

    #[test]
    fn the_same_card_can_be_bought_on_top_of_one_already_owned() {
        let mut shop = shop_selling("strike", 50);
        let mut credits = 50;
        let mut deck = RunDeck::default();
        deck.add("strike");

        shop.buy(0, &mut credits, &mut deck).expect("affordable");

        assert_eq!(deck.count_of("strike"), 2);
    }

    #[test]
    fn every_refusal_has_a_distinct_code() {
        let codes: HashSet<&str> = [
            ShopError::NoSuchSlot,
            ShopError::AlreadySold,
            ShopError::NotEnoughCredits,
        ]
        .iter()
        .map(|error| error.code())
        .collect();

        assert_eq!(codes.len(), 3);
    }
}

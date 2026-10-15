//! The dark web shop a player can visit between encounters (Trello #38).
//!
//! Covers four of its stories:
//!
//! - **S1, shop stock and pricing (#104):** entering a shop node rolls a fixed
//!   stock of [`STOCK_SIZE`] cards, each priced by its rarity. A bought slot is
//!   marked sold rather than removed, so the other cards keep their places.
//! - **S2, buying with credits (#105):** [`ShopVisit::buy`] checks the slot
//!   and the balance first and only then takes the price and adds the card to
//!   the run's deck, so a refused purchase changes nothing and credits can
//!   never go negative.
//! - **S3, Scrub Logs (#106):** [`ShopVisit::scrub_logs`] is a once-per-visit
//!   service that lowers the shared noise meter by [`SCRUB_LOGS_NOISE_REDUCTION`]
//!   for [`SCRUB_LOGS_PRICE`] credits. Once bought it is sold out for the rest
//!   of the visit.
//! - **S4, rerolling the stock (#107):** [`ShopVisit::reroll`] replaces every
//!   card still for sale with a fresh roll. Sold slots stay sold. Each reroll
//!   in a visit costs [`REROLL_PRICE_STEP`] more than the one before, starting
//!   from [`REROLL_BASE_PRICE`].
//!
//! Every paid action spends credits through the one private [`charge`] step,
//! and every check happens before it, so a refused action changes nothing and
//! the balance can never be taken below zero.
//!
//! The run keeps the [`ShopVisit`] (see `RunState::open_shop_visit`), not the
//! shop screen, so reopening the screen during the same visit shows the same
//! stock, sold slots, Scrub Logs and reroll price.
//!
//! A [`ShopVisit`] is the state of one visit. Plain Rust with no Godot types,
//! so the rules stay unit testable per docs/rust-godot-setup.md. `ShopNode` in
//! `shop_node.rs` is the Godot side.

use std::collections::HashMap;

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::card_data::Rarity;
use crate::card_reward;
use crate::noise_meter::NoiseLevel;
use crate::run_deck::RunDeck;

/// How many cards a shop has for sale on each visit.
pub const STOCK_SIZE: usize = 5;

/// What Scrub Logs costs, in credits.
///
/// More than a common card, since 20 noise is two plays of a common recovery
/// card such as VPN without spending a turn on them, but well short of a rare.
pub const SCRUB_LOGS_PRICE: i64 = 60;

/// What removing a card from the deck costs, in credits. Once per visit.
pub const REMOVE_CARD_PRICE: i64 = 50;

/// How much noise Scrub Logs takes off the shared meter.
pub const SCRUB_LOGS_NOISE_REDUCTION: i32 = 20;

/// What the first reroll in a visit costs, in credits: one zone 1 combat's pay,
/// half the price of the cheapest card.
pub const REROLL_BASE_PRICE: i64 = 25;

/// How much more each later reroll in the same visit costs than the one
/// before it, so the second is 50, the third 75 and so on. Rerolling is a way
/// to look for one card, not to see the whole pool for a flat fee.
pub const REROLL_PRICE_STEP: i64 = 25;

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

pub fn upgrade_price_for(rarity: Rarity) -> i64 {
    match rarity {
        Rarity::Common => 40,
        Rarity::Uncommon => 60,
        Rarity::Rare => 80,
        Rarity::Legendary => 120,
    }
}

/// One place in the shop's stock: a card, its rarity and price, and whether
/// it has gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopSlot {
    card_id: String,
    rarity: Rarity,
    price: i64,
    sold: bool,
}

impl ShopSlot {
    fn for_sale(card_id: String, rarity: Rarity) -> Self {
        Self {
            card_id,
            rarity,
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

/// A card the player has just had removed from their deck, for the screen to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedCard {
    pub card_id: String,
    pub price: i64,
    /// The player's credits after paying.
    pub credits_left: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    pub from_id: String,
    pub to_id: String,
    pub price: i64,
    /// The player's credits after paying.
    pub credits_left: i64,
}

/// A Scrub Logs service the player has just paid for, for the screen to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrubbedLogs {
    pub price: i64,
    /// How much noise actually came off, which is less than
    /// [`SCRUB_LOGS_NOISE_REDUCTION`] when the meter was already lower.
    pub noise_removed: i32,
    /// The noise left on the meter afterwards.
    pub noise_left: i32,
    /// The player's credits after paying.
    pub credits_left: i64,
}

/// A reroll the player has just paid for, for the screen to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reroll {
    pub price: i64,
    /// What the next reroll in this visit will cost.
    pub next_price: i64,
    /// The player's credits after paying.
    pub credits_left: i64,
}

/// Why a shop action was refused. A refusal never changes credits, the deck,
/// the noise meter or the stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopError {
    /// There is no slot with that index in this shop.
    NoSuchSlot,
    /// The card in that slot, or the once-per-visit Scrub Logs service, has
    /// already been bought this visit.
    AlreadySold,
    /// The player has fewer credits than the price.
    NotEnoughCredits,
    /// Noise is already at zero, so Scrub Logs would take credits for nothing.
    NothingToScrub,
    /// Every card has been bought, so a reroll would take credits for nothing.
    NothingToReroll,
    /// The run is not in a shop encounter, so there is no visit to buy from.
    NotInShop,

    NotInDeck,

    /// Removing would leave the run with no cards to draw.
    DeckTooSmall,
}

impl ShopError {
    /// A stable name for the Godot side to report and test against.
    pub fn code(self) -> &'static str {
        match self {
            ShopError::NoSuchSlot => "no_such_slot",
            ShopError::AlreadySold => "already_sold",
            ShopError::NotEnoughCredits => "not_enough_credits",
            ShopError::NothingToScrub => "nothing_to_scrub",
            ShopError::NothingToReroll => "nothing_to_reroll",
            ShopError::NotInShop => "not_in_shop",
            ShopError::NotInDeck => "not_in_deck",
            ShopError::DeckTooSmall => "deck_too_small",
        }
    }
}

/// Everything about one visit to a shop node.
/// Saved with the run while the player is in a shop (see `save_game.rs`), so
/// a resumed visit keeps its stock and what has already been bought.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopVisit {
    /// The distinct cards this shop can stock, sorted by id so the same seed
    /// always rolls the same cards. Kept for rerolls.
    pool: Vec<(String, Rarity)>,
    slots: Vec<ShopSlot>,
    scrub_logs_sold: bool,
    rerolls: u32,
    removal_used: bool,
}

impl ShopVisit {
    /// Opens a shop with up to [`STOCK_SIZE`] distinct cards from `pool`, each
    /// priced by its rarity. A pool with fewer distinct cards than that gives
    /// a smaller stock rather than repeating a card.
    pub fn open(pool: &[(String, Rarity)], rng: &mut impl Rng) -> Self {
        let pool = distinct_sorted(pool);
        let slots = roll_slots(&pool, STOCK_SIZE, rng);
        Self {
            pool,
            slots,
            scrub_logs_sold: false,
            rerolls: 0,
            removal_used: false,
        }
    }

    /// The stock in display order, sold slots included.
    pub fn slots(&self) -> &[ShopSlot] {
        &self.slots
    }

    /// Whether the once-per-visit card removal has already been used.
    pub fn removal_used(&self) -> bool {
        self.removal_used
    }

    /// Whether a card removal can be bought right now. The screen enables its
    /// button by this, and `remove_card` refuses exactly when it is false.
    pub fn can_remove_card(&self, credits: i64, deck_len: usize) -> bool {
        self.removal_refusal(credits, deck_len).is_none()
    }

    /// Removes one copy of `card_id` from `deck` for good: takes
    /// [`REMOVE_CARD_PRICE`] from `credits` and uses up this visit's removal.
    /// Nothing replaces the card. Every check happens before anything changes.
    pub fn remove_card(
        &mut self,
        card_id: &str,
        credits: &mut i64,
        deck: &mut RunDeck,
    ) -> Result<RemovedCard, ShopError> {
        if let Some(refusal) = self.removal_refusal(*credits, deck.len()) {
            return Err(refusal);
        }
        if deck.count_of(card_id) == 0 {
            return Err(ShopError::NotInDeck);
        }

        charge(credits, REMOVE_CARD_PRICE)?;
        deck.remove_one(card_id);
        self.removal_used = true;

        Ok(RemovedCard {
            card_id: card_id.to_string(),
            price: REMOVE_CARD_PRICE,
            credits_left: *credits,
        })
    }

    fn removal_refusal(&self, credits: i64, deck_len: usize) -> Option<ShopError> {
        if self.removal_used {
            Some(ShopError::AlreadySold)
        } else if deck_len <= 1 {
            Some(ShopError::DeckTooSmall)
        } else if !covers(credits, REMOVE_CARD_PRICE) {
            Some(ShopError::NotEnoughCredits)
        } else {
            None
        }
    }

    /// Checks a visit read back from a save is one `open` could have rolled
    /// and buying could have left: no more than [`STOCK_SIZE`] slots, no card
    /// twice, and every slot holding its card's real rarity and priced by it.
    /// The pool a reroll draws from is checked the same way: every card real,
    /// listed once, with its real rarity. `rarity_of` looks a card up in the
    /// card database, `None` for one that does not exist.
    ///
    /// Without this an edited save could make every card free on Continue, or
    /// slip a cheap or made-up card into the stock through a reroll.
    pub fn validate_restored(
        &self,
        rarity_of: impl Fn(&str) -> Option<Rarity>,
    ) -> Result<(), String> {
        if self.slots.len() > STOCK_SIZE {
            return Err(format!("a shop has more than {STOCK_SIZE} cards"));
        }
        let mut stocked = std::collections::HashSet::new();
        for slot in &self.slots {
            if !stocked.insert(slot.card_id.as_str()) {
                return Err(format!("a shop stocks '{}' twice", slot.card_id));
            }
            let Some(rarity) = rarity_of(&slot.card_id) else {
                return Err(format!(
                    "a shop stocks a card that does not exist: '{}'",
                    slot.card_id
                ));
            };
            if slot.rarity != rarity {
                return Err(format!(
                    "'{}' is stocked with the wrong rarity",
                    slot.card_id
                ));
            }
            if slot.price != price_for(rarity) {
                return Err(format!("'{}' is not priced by its rarity", slot.card_id));
            }
        }

        let mut pooled = std::collections::HashSet::new();
        for (card_id, rarity) in &self.pool {
            if !pooled.insert(card_id.as_str()) {
                return Err(format!("a shop's pool lists '{card_id}' twice"));
            }
            if rarity_of(card_id) != Some(*rarity) {
                return Err(format!(
                    "a shop's pool has '{card_id}', which does not exist or has another rarity"
                ));
            }
        }
        Ok(())
    }

    /// Whether `credits` would cover the card in `slot`. False for a sold or
    /// missing slot, since there is nothing there to afford.
    pub fn can_afford(&self, slot: usize, credits: i64) -> bool {
        match self.slots.get(slot) {
            Some(slot) => !slot.sold && covers(credits, slot.price),
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

    /// Whether Scrub Logs has already been bought this visit.
    pub fn scrub_logs_sold(&self) -> bool {
        self.scrub_logs_sold
    }

    /// Whether Scrub Logs can be bought right now: it is not sold out, it
    /// would actually lower `noise`, and `credits` cover it. The screen
    /// enables its button by this, and [`ShopVisit::scrub_logs`] refuses
    /// exactly when it is false.
    pub fn can_buy_scrub_logs(&self, credits: i64, noise: &NoiseLevel) -> bool {
        self.scrub_logs_refusal(credits, noise).is_none()
    }

    /// Buys the Scrub Logs service: takes [`SCRUB_LOGS_PRICE`] from `credits`
    /// and lowers `noise` by [`SCRUB_LOGS_NOISE_REDUCTION`], clamped at zero by
    /// the meter itself. It can be bought once per visit.
    ///
    /// Refused, with nothing changed, when it is sold out, when it would not
    /// lower the noise at all (none to scrub, or resistance blocking all of
    /// it) or when the player cannot afford it.
    pub fn scrub_logs(
        &mut self,
        credits: &mut i64,
        noise: &mut NoiseLevel,
    ) -> Result<ScrubbedLogs, ShopError> {
        if let Some(refusal) = self.scrub_logs_refusal(*credits, noise) {
            return Err(refusal);
        }

        charge(credits, SCRUB_LOGS_PRICE)?;
        let change = noise.add(-SCRUB_LOGS_NOISE_REDUCTION);
        self.scrub_logs_sold = true;

        Ok(ScrubbedLogs {
            price: SCRUB_LOGS_PRICE,
            noise_removed: -change,
            noise_left: noise.noise(),
            credits_left: *credits,
        })
    }

    /// Why Scrub Logs cannot be bought right now, or `None` if it can.
    fn scrub_logs_refusal(&self, credits: i64, noise: &NoiseLevel) -> Option<ShopError> {
        if self.scrub_logs_sold {
            Some(ShopError::AlreadySold)
        } else if scrub_logs_reduction(noise) <= 0 {
            Some(ShopError::NothingToScrub)
        } else if !covers(credits, SCRUB_LOGS_PRICE) {
            Some(ShopError::NotEnoughCredits)
        } else {
            None
        }
    }

    /// What the next reroll in this visit costs.
    pub fn reroll_price(&self) -> i64 {
        REROLL_BASE_PRICE + REROLL_PRICE_STEP * i64::from(self.rerolls)
    }

    /// Whether a reroll would change anything: a card is still for sale, and
    /// the pool holds a card that is not already in the stock to replace it
    /// with. Without both, a reroll could only hand back the same cards.
    pub fn has_something_to_reroll(&self) -> bool {
        let any_unsold = self.slots.iter().any(|slot| !slot.sold);
        let any_unstocked = self
            .pool
            .iter()
            .any(|(card_id, _)| !self.is_stocked(card_id));
        any_unsold && any_unstocked
    }

    /// Whether the next reroll can be bought right now: there is something to
    /// reroll and `credits` cover it. The screen enables its button by this,
    /// and [`ShopVisit::reroll`] refuses exactly when it is false.
    pub fn can_buy_reroll(&self, credits: i64) -> bool {
        self.reroll_refusal(credits).is_none()
    }

    /// Rerolls the stock: takes [`ShopVisit::reroll_price`] from `credits` and
    /// replaces the card in every unsold slot, each priced by its rarity. Sold
    /// slots stay sold and every slot keeps its place, so the stock is the
    /// same size afterwards. The next reroll then costs [`REROLL_PRICE_STEP`]
    /// more.
    ///
    /// A replacement is always a card that was not in the stock before the
    /// reroll, as long as the pool has enough of them. Only a pool too small
    /// to fill every slot with a new card offers some of the old ones again.
    /// The stock never holds the same card twice.
    ///
    /// Refused, with nothing changed, when there is nothing to reroll (see
    /// [`ShopVisit::has_something_to_reroll`]) or the player cannot afford it.
    pub fn reroll(&mut self, credits: &mut i64, rng: &mut impl Rng) -> Result<Reroll, ShopError> {
        if let Some(refusal) = self.reroll_refusal(*credits) {
            return Err(refusal);
        }

        // Rolled before paying, so nothing can go wrong after the charge.
        let replacements = self.roll_replacements(rng);
        let price = self.reroll_price();
        charge(credits, price)?;
        self.rerolls += 1;
        let unsold_slots = self.slots.iter_mut().filter(|slot| !slot.sold);
        for (slot, replacement) in unsold_slots.zip(replacements) {
            *slot = replacement;
        }

        Ok(Reroll {
            price,
            next_price: self.reroll_price(),
            credits_left: *credits,
        })
    }

    /// Why the next reroll cannot be bought right now, or `None` if it can.
    fn reroll_refusal(&self, credits: i64) -> Option<ShopError> {
        if !self.has_something_to_reroll() {
            Some(ShopError::NothingToReroll)
        } else if !covers(credits, self.reroll_price()) {
            Some(ShopError::NotEnoughCredits)
        } else {
            None
        }
    }

    /// One replacement for every unsold slot, in slot order. Cards that are
    /// not in the stock come first; the cards being replaced are only offered
    /// again if the pool runs out of new ones. Those replaced cards alone are
    /// one per unsold slot, so there are always enough.
    fn roll_replacements(&self, rng: &mut impl Rng) -> Vec<ShopSlot> {
        let unsold: Vec<&ShopSlot> = self.slots.iter().filter(|slot| !slot.sold).collect();
        let unstocked: Vec<(String, Rarity)> = self
            .pool
            .iter()
            .filter(|(card_id, _)| !self.is_stocked(card_id))
            .cloned()
            .collect();

        let mut replacements = roll_slots(&unstocked, unsold.len(), rng);
        let mut replaced: Vec<ShopSlot> = unsold
            .iter()
            .map(|slot| ShopSlot::for_sale(slot.card_id.clone(), slot.rarity))
            .collect();
        replaced.shuffle(rng);
        replacements.extend(replaced);
        replacements.truncate(unsold.len());
        replacements
    }

    fn is_stocked(&self, card_id: &str) -> bool {
        self.slots.iter().any(|slot| slot.card_id == card_id)
    }
}

/// How much Scrub Logs would take off `noise` right now, after any detection
/// resistance and the meter's floor of zero. Worked out on a copy, so the
/// real meter is untouched.
pub fn scrub_logs_reduction(noise: &NoiseLevel) -> i32 {
    let mut preview = *noise;
    -preview.add(-SCRUB_LOGS_NOISE_REDUCTION)
}

/// Whether a balance of `credits` covers `price`. The one affordability rule,
/// shared by what the screen shows ([`ShopVisit::can_afford`]) and what
/// [`charge`] allows, so the two can never disagree.
fn covers(credits: i64, price: i64) -> bool {
    credits >= price
}

/// Takes `price` from `credits`, or refuses without touching them when the
/// balance does not cover it. The single place the shop spends credits, so
/// every paid action shares the same never-negative rule.
fn charge(credits: &mut i64, price: i64) -> Result<(), ShopError> {
    if !covers(*credits, price) {
        return Err(ShopError::NotEnoughCredits);
    }
    *credits -= price;
    Ok(())
}

pub fn upgrade_card(
    credits: &mut i64,
    deck: &mut RunDeck,
    card_id: &str,
    upgraded_id: &str,
    rarity: Rarity,
) -> Result<Upgrade, ShopError> {
    if deck.count_of(card_id) == 0 {
        return Err(ShopError::NotInDeck);
    }

    let price = upgrade_price_for(rarity);
    charge(credits, price)?;
    deck.replace_one(card_id, upgraded_id);

    Ok(Upgrade {
        from_id: card_id.to_string(),
        to_id: upgraded_id.to_string(),
        price,
        credits_left: *credits,
    })
}

/// The pool with each card once, sorted by id. Sorted before any shuffling so
/// the same seed always gives the same stock, whatever order the card
/// database handed the pool over in.
fn distinct_sorted(pool: &[(String, Rarity)]) -> Vec<(String, Rarity)> {
    let mut distinct: Vec<(String, Rarity)> = pool.to_vec();
    distinct.sort_by(|a, b| a.0.cmp(&b.0));
    distinct.dedup_by(|a, b| a.0 == b.0);
    distinct
}

/// Picks up to `count` distinct cards from `pool` and prices each for sale.
///
/// The cards are chosen by the same distinct-offer rule as a card reward
/// ([`card_reward::offer_from_pool`]), which also sorts the pool first, so a
/// test with a fixed seed gets the same stock whatever order the pool is in.
fn roll_slots(pool: &[(String, Rarity)], count: usize, rng: &mut impl Rng) -> Vec<ShopSlot> {
    let rarity_of: HashMap<&str, Rarity> = pool
        .iter()
        .map(|(card_id, rarity)| (card_id.as_str(), *rarity))
        .collect();
    let card_ids: Vec<String> = pool.iter().map(|(card_id, _)| card_id.clone()).collect();

    card_reward::offer_from_pool(&card_ids, count, rng)
        .into_iter()
        .map(|card_id| {
            let rarity = rarity_of[card_id.as_str()];
            ShopSlot::for_sale(card_id, rarity)
        })
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
    /// buying scenarios that name a price. Its pool is just that card.
    fn shop_selling(card_id: &str, price: i64) -> ShopVisit {
        ShopVisit {
            pool: vec![(card_id.to_string(), Rarity::Common)],
            slots: vec![ShopSlot {
                card_id: card_id.to_string(),
                rarity: Rarity::Common,
                price,
                sold: false,
            }],
            scrub_logs_sold: false,
            rerolls: 0,
            removal_used: false,
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
        let refusals = [
            ShopError::NoSuchSlot,
            ShopError::AlreadySold,
            ShopError::NotEnoughCredits,
            ShopError::NothingToScrub,
            ShopError::NothingToReroll,
            ShopError::NotInShop,
        ];
        let codes: HashSet<&str> = refusals.iter().map(|error| error.code()).collect();

        assert_eq!(codes.len(), refusals.len());
    }

    // --- S3: Scrub Logs ---------------------------------------------------

    fn noise_at(noise: i32) -> NoiseLevel {
        let mut level = NoiseLevel::new(100);
        level.add(noise);
        level
    }

    /// S3 scenario 1: at 70 noise with enough credits, Scrub Logs takes noise
    /// down by 20 to 50 and credits down by its price.
    #[test]
    fn scrub_logs_lowers_noise_by_twenty_and_takes_its_price() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(8));
        let mut noise = noise_at(70);
        let mut credits = 100;

        assert!(shop.can_buy_scrub_logs(credits, &noise));
        let scrubbed = shop
            .scrub_logs(&mut credits, &mut noise)
            .expect("affordable");

        assert_eq!(noise.noise(), 50);
        assert_eq!(credits, 100 - SCRUB_LOGS_PRICE);
        assert_eq!(
            scrubbed,
            ScrubbedLogs {
                price: SCRUB_LOGS_PRICE,
                noise_removed: 20,
                noise_left: 50,
                credits_left: 100 - SCRUB_LOGS_PRICE,
            }
        );
        assert_eq!(SCRUB_LOGS_NOISE_REDUCTION, 20);
    }

    /// S3 scenario 2: once bought, Scrub Logs is sold out for the visit and a
    /// second attempt charges nothing.
    #[test]
    fn scrub_logs_is_sold_out_after_one_use_per_visit() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(9));
        let mut noise = noise_at(70);
        let mut credits = 500;
        shop.scrub_logs(&mut credits, &mut noise)
            .expect("first use");

        assert!(shop.scrub_logs_sold());
        assert!(
            !shop.can_buy_scrub_logs(credits, &noise),
            "not on offer again"
        );
        assert_eq!(
            shop.scrub_logs(&mut credits, &mut noise),
            Err(ShopError::AlreadySold)
        );
        assert_eq!(credits, 500 - SCRUB_LOGS_PRICE, "charged only once");
        assert_eq!(noise.noise(), 50, "lowered only once");
    }

    #[test]
    fn scrub_logs_is_on_offer_again_at_the_next_shop() {
        let mut first = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(10));
        let mut noise = noise_at(70);
        let mut credits = 500;
        first
            .scrub_logs(&mut credits, &mut noise)
            .expect("first shop");

        let mut next = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(11));

        assert!(!next.scrub_logs_sold());
        next.scrub_logs(&mut credits, &mut noise)
            .expect("a new visit sells it again");
        assert_eq!(noise.noise(), 30);
    }

    #[test]
    fn scrub_logs_never_takes_noise_below_zero() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(12));
        let mut noise = noise_at(8);
        let mut credits = 100;

        let scrubbed = shop
            .scrub_logs(&mut credits, &mut noise)
            .expect("affordable");

        assert_eq!(noise.noise(), 0);
        assert_eq!(scrubbed.noise_removed, 8);
    }

    #[test]
    fn unaffordable_scrub_logs_changes_nothing() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(13));
        let before = shop.clone();
        let mut noise = noise_at(70);
        let mut credits = SCRUB_LOGS_PRICE - 1;

        assert!(!shop.can_buy_scrub_logs(credits, &noise));
        assert_eq!(
            shop.scrub_logs(&mut credits, &mut noise),
            Err(ShopError::NotEnoughCredits)
        );
        assert_eq!(credits, SCRUB_LOGS_PRICE - 1);
        assert_eq!(noise.noise(), 70);
        assert_eq!(shop, before, "still on offer");

        let mut debt = -30;
        assert_eq!(
            shop.scrub_logs(&mut debt, &mut noise),
            Err(ShopError::NotEnoughCredits)
        );
        assert_eq!(debt, -30, "credits never go further negative");
    }

    #[test]
    fn scrub_logs_will_not_charge_when_there_is_no_noise_to_scrub() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(14));
        let mut noise = noise_at(0);
        let mut credits = 100;

        assert!(
            !shop.can_buy_scrub_logs(credits, &noise),
            "not offered with nothing to scrub, so the screen shows it red"
        );
        assert_eq!(
            shop.scrub_logs(&mut credits, &mut noise),
            Err(ShopError::NothingToScrub)
        );
        assert_eq!(credits, 100);
        assert!(
            !shop.scrub_logs_sold(),
            "still on offer once there is noise"
        );
    }

    #[test]
    fn scrub_logs_will_not_charge_when_resistance_would_block_the_whole_reduction() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(31));
        let mut noise = noise_at(70);
        noise.set_resistance_percent(100);
        let mut credits = 100;

        assert_eq!(scrub_logs_reduction(&noise), 0);
        assert!(!shop.can_buy_scrub_logs(credits, &noise));
        assert_eq!(
            shop.scrub_logs(&mut credits, &mut noise),
            Err(ShopError::NothingToScrub)
        );
        assert_eq!(credits, 100);
        assert_eq!(noise.noise(), 70);
        assert!(!shop.scrub_logs_sold());
    }

    // --- S4: rerolling the stock ------------------------------------------

    /// S4 scenario 1: with 50 credits and a 25 credit reroll, rerolling
    /// replaces every unsold card with a new one and leaves 25 credits.
    #[test]
    fn a_reroll_replaces_every_unsold_card_and_takes_its_price() {
        // Enough cards that all five can be replaced by ones not yet stocked.
        let mut large_pool = pool();
        for extra in ["vpn", "proxy", "keylogger"] {
            large_pool.push((extra.to_string(), Rarity::Common));
        }
        let mut shop = ShopVisit::open(&large_pool, &mut StdRng::seed_from_u64(15));
        let before: Vec<String> = shop
            .slots()
            .iter()
            .map(|slot| slot.card_id().to_string())
            .collect();
        let mut credits = 50;

        assert_eq!(shop.reroll_price(), 25);
        assert!(shop.can_buy_reroll(credits));
        let reroll = shop
            .reroll(&mut credits, &mut StdRng::seed_from_u64(16))
            .expect("affordable");

        assert_eq!(credits, 25);
        assert_eq!(reroll.price, 25);
        assert_eq!(reroll.credits_left, 25);
        assert_eq!(shop.slots().len(), STOCK_SIZE);
        for slot in shop.slots() {
            assert!(
                !before.iter().any(|old| old == slot.card_id()),
                "{} was in the stock before the reroll",
                slot.card_id()
            );
            assert!(!slot.is_sold());
        }
    }

    #[test]
    fn a_reroll_leaves_sold_slots_sold_and_in_place() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(17));
        let mut credits = 1_000;
        let mut deck = RunDeck::default();
        shop.buy(1, &mut credits, &mut deck).expect("affordable");
        shop.buy(3, &mut credits, &mut deck).expect("affordable");
        let before = shop.slots().to_vec();

        shop.reroll(&mut credits, &mut StdRng::seed_from_u64(18))
            .expect("affordable");

        for (index, (now, was)) in shop.slots().iter().zip(&before).enumerate() {
            if was.is_sold() {
                assert_eq!(now, was, "sold slot {index} changed");
            } else {
                assert!(!now.is_sold(), "slot {index} is for sale");
                assert_ne!(
                    now.card_id(),
                    was.card_id(),
                    "slot {index} was not rerolled"
                );
            }
        }
        assert_eq!(deck.card_ids().len(), 2, "a reroll adds no cards");
    }

    #[test]
    fn rerolled_cards_are_priced_by_rarity_and_never_repeat() {
        let pool = pool();
        let rarity_of: HashMap<&str, Rarity> = pool
            .iter()
            .map(|(id, rarity)| (id.as_str(), *rarity))
            .collect();
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut shop = ShopVisit::open(&pool, &mut rng);
            let mut credits = 1_000;
            let mut deck = RunDeck::default();
            shop.buy(0, &mut credits, &mut deck).expect("affordable");

            shop.reroll(&mut credits, &mut rng).expect("affordable");

            let stocked: HashSet<&str> = shop.slots().iter().map(|slot| slot.card_id()).collect();
            assert_eq!(stocked.len(), STOCK_SIZE, "seed {seed}: repeated a card");
            for slot in shop.slots() {
                let rarity = rarity_of[slot.card_id()];
                assert_eq!(slot.price(), price_for(rarity), "seed {seed}");
            }
        }
    }

    /// S4 scenario 2: each reroll in a visit costs more than the last.
    #[test]
    fn each_reroll_in_a_visit_costs_more_than_the_last() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(19));
        let mut rng = StdRng::seed_from_u64(20);
        let mut credits = 1_000;

        let first = shop.reroll(&mut credits, &mut rng).expect("first");
        let second = shop.reroll(&mut credits, &mut rng).expect("second");

        assert_eq!(first.price, REROLL_BASE_PRICE);
        assert_eq!(first.next_price, second.price);
        assert!(second.price > first.price);
        assert_eq!(second.price, REROLL_BASE_PRICE + REROLL_PRICE_STEP);
        assert_eq!(
            shop.reroll_price(),
            REROLL_BASE_PRICE + 2 * REROLL_PRICE_STEP
        );
        assert_eq!(credits, 1_000 - first.price - second.price);
    }

    #[test]
    fn the_next_shop_starts_rerolls_at_the_base_price_again() {
        let mut first = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(21));
        let mut credits = 1_000;
        first
            .reroll(&mut credits, &mut StdRng::seed_from_u64(22))
            .expect("affordable");

        let next = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(23));

        assert_eq!(next.reroll_price(), REROLL_BASE_PRICE);
    }

    #[test]
    fn an_unaffordable_reroll_changes_nothing() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(24));
        let before = shop.clone();
        let mut credits = REROLL_BASE_PRICE - 1;

        assert!(!shop.can_buy_reroll(credits));
        assert_eq!(
            shop.reroll(&mut credits, &mut StdRng::seed_from_u64(25)),
            Err(ShopError::NotEnoughCredits)
        );
        assert_eq!(credits, REROLL_BASE_PRICE - 1);
        assert_eq!(shop, before, "same stock, same next price");
    }

    #[test]
    fn a_sold_out_stock_cannot_be_rerolled() {
        let mut shop = shop_selling("rootkit", 75);
        let mut credits = 500;
        let mut deck = RunDeck::default();
        shop.buy(0, &mut credits, &mut deck).expect("affordable");

        assert!(!shop.can_buy_reroll(credits));
        assert_eq!(
            shop.reroll(&mut credits, &mut StdRng::seed_from_u64(26)),
            Err(ShopError::NothingToReroll)
        );
        assert_eq!(credits, 425, "nothing charged");
    }

    #[test]
    fn a_pool_with_no_card_outside_the_stock_cannot_be_rerolled() {
        // Exactly STOCK_SIZE distinct cards: every one is already stocked, so a
        // reroll could only shuffle the same cards around.
        let exact: Vec<(String, Rarity)> = pool().into_iter().take(STOCK_SIZE).collect();
        let mut shop = ShopVisit::open(&exact, &mut StdRng::seed_from_u64(32));
        let before = shop.clone();
        let mut credits = 500;

        assert!(!shop.has_something_to_reroll());
        assert!(!shop.can_buy_reroll(credits));
        assert_eq!(
            shop.reroll(&mut credits, &mut StdRng::seed_from_u64(33)),
            Err(ShopError::NothingToReroll)
        );
        assert_eq!(credits, 500, "nothing charged");
        assert_eq!(shop, before, "same stock, same next price");
    }

    #[test]
    fn a_reroll_refills_every_unsold_slot_even_if_its_card_left_the_pool() {
        // The stocked card is not in the pool the visit kept, as could happen
        // if slots were ever built some other way. The replacements come from
        // the slots themselves, so the reroll still fills every unsold slot.
        let mut shop = shop_selling("rootkit", 75);
        shop.pool = vec![("vpn".to_string(), Rarity::Common)];
        let mut credits = 100;

        shop.reroll(&mut credits, &mut StdRng::seed_from_u64(34))
            .expect("one new card to bring in");

        assert_eq!(shop.slots().len(), 1);
        assert_eq!(shop.slots()[0].card_id(), "vpn");
        assert_eq!(credits, 100 - REROLL_BASE_PRICE);
    }

    #[test]
    fn a_pool_too_small_for_all_new_cards_still_fills_every_slot() {
        // Six cards and a stock of five leaves one new card for a reroll, so
        // the other four slots must take back cards from before.
        let small: Vec<(String, Rarity)> = pool().into_iter().take(6).collect();
        let mut shop = ShopVisit::open(&small, &mut StdRng::seed_from_u64(27));
        let before: HashSet<String> = shop
            .slots()
            .iter()
            .map(|slot| slot.card_id().to_string())
            .collect();
        let mut credits = 100;

        shop.reroll(&mut credits, &mut StdRng::seed_from_u64(28))
            .expect("affordable");

        let after: HashSet<&str> = shop.slots().iter().map(|slot| slot.card_id()).collect();
        assert_eq!(after.len(), STOCK_SIZE, "every slot is filled, no repeats");
        let new_cards = after.iter().filter(|id| !before.contains(**id)).count();
        assert_eq!(new_cards, 1, "the one card not in stock was brought in");
    }

    #[test]
    fn services_and_cards_share_one_balance_that_never_goes_negative() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(29));
        let mut rng = StdRng::seed_from_u64(30);
        let mut noise = noise_at(90);
        let mut credits = 160;
        let mut deck = RunDeck::default();

        shop.scrub_logs(&mut credits, &mut noise)
            .expect("60 of 160");
        shop.reroll(&mut credits, &mut rng).expect("25 of 100");
        shop.reroll(&mut credits, &mut rng).expect("50 of 75");

        assert_eq!(
            shop.reroll(&mut credits, &mut rng),
            Err(ShopError::NotEnoughCredits)
        );
        for slot in 0..shop.slots().len() {
            assert!(shop.buy(slot, &mut credits, &mut deck).is_err());
        }
        assert_eq!(credits, 160 - SCRUB_LOGS_PRICE - 25 - 50);
        assert!(credits >= 0);
    }

    // --- a visit read back from a save ------------------------------------

    fn rarity_in_pool(card_id: &str) -> Option<Rarity> {
        pool()
            .into_iter()
            .find(|(id, _)| id == card_id)
            .map(|(_, rarity)| rarity)
    }

    #[test]
    fn a_rolled_and_partly_bought_visit_passes_the_restore_check() {
        let mut shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(40));
        let mut credits = 1_000;
        shop.buy(1, &mut credits, &mut RunDeck::default())
            .expect("affordable");

        assert_eq!(shop.validate_restored(rarity_in_pool), Ok(()));
    }

    #[test]
    fn an_edited_visit_is_refused() {
        let shop = ShopVisit::open(&pool(), &mut StdRng::seed_from_u64(41));
        let free = {
            let mut edited = shop.clone();
            edited.slots[0].price = 0;
            edited
        };
        let repeated = {
            let mut edited = shop.clone();
            edited.slots[1] = edited.slots[0].clone();
            edited
        };
        let oversized = {
            let mut edited = shop.clone();
            edited
                .slots
                .push(ShopSlot::for_sale("stuxnet".to_string(), Rarity::Legendary));
            edited
        };
        let unknown = shop_selling("not_a_real_card", 50);
        let wrong_slot_rarity = {
            let mut edited = shop.clone();
            edited.slots[0].rarity = match edited.slots[0].rarity {
                Rarity::Common => Rarity::Legendary,
                _ => Rarity::Common,
            };
            edited
        };
        let unknown_in_pool = {
            let mut edited = shop.clone();
            edited
                .pool
                .push(("not_a_real_card".to_string(), Rarity::Common));
            edited
        };
        let cheapened_in_pool = {
            let mut edited = shop.clone();
            for (card_id, rarity) in &mut edited.pool {
                if card_id == "stuxnet" {
                    *rarity = Rarity::Common;
                }
            }
            edited
        };
        let repeated_in_pool = {
            let mut edited = shop.clone();
            let first = edited.pool[0].clone();
            edited.pool.push(first);
            edited
        };

        for (description, edited) in [
            ("a free card", free),
            ("a card twice", repeated),
            ("too many cards", oversized),
            ("an unknown card", unknown),
            ("a slot's rarity changed", wrong_slot_rarity),
            ("an unknown card in its pool", unknown_in_pool),
            ("a Legendary made Common in its pool", cheapened_in_pool),
            ("a card twice in its pool", repeated_in_pool),
        ] {
            assert!(
                edited.validate_restored(rarity_in_pool).is_err(),
                "a visit with {description} was not refused"
            );
        }
    }
}

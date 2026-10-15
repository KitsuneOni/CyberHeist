//! Thin Godot adapter for the run's deck in `run_deck.rs`.
//!
//! Lives under the `FlowCoordinator` autoload, so it outlives any one
//! encounter screen. That is the whole point: `DrawPhase` is rebuilt every time
//! the combat scene loads, so the run's cards cannot live there.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_data::Rarity;
use crate::card_database::CardDatabase;
use crate::card_reward;
use crate::card_text;
use crate::run_deck::RunDeck;
use crate::starter_deck;
use crate::{CARD_DATABASE_PATH, card_detail_dictionary};

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct RunDeckNode {
    deck: RunDeck,
    base: Base<Node>,
}

#[godot_api]
impl INode for RunDeckNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            deck: RunDeck::default(),
            base,
        }
    }

    fn ready(&mut self) {
        self.reset_to_starter();
    }
}

impl RunDeckNode {
    /// The run's cards, for `DrawPhase` to resolve into an encounter deck.
    pub(crate) fn owned_card_ids(&self) -> Vec<String> {
        self.deck.card_ids().to_vec()
    }

    /// The run's deck itself, for a Rust rule that adds a card as part of a
    /// larger step. `ShopNode` lends it to `RunState::buy_from_shop`, so a
    /// bought card is added in the same operation that takes its price.
    /// Every id the shop sells was drawn from the card database when its stock
    /// was rolled, so it needs no second check here.
    pub(crate) fn deck_mut(&mut self) -> &mut RunDeck {
        &mut self.deck
    }

    fn card_database(&self) -> Option<Gd<CardDatabase>> {
        self.base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
    }

    /// The reward screen's description of each card in `card_ids`, skipping
    /// any the card database does not know.
    fn describe_reward_cards<'a>(
        &self,
        card_ids: impl Iterator<Item = &'a str>,
    ) -> Array<VarDictionary> {
        let mut described: Array<VarDictionary> = Array::new();

        let Some(db) = self.card_database() else {
            godot_error!("RunDeckNode: no CardDatabaseGlobal, cannot describe cards.");
            return described;
        };
        let db = db.bind();

        for id in card_ids {
            let Some(card) = db.get(id) else {
                continue;
            };
            // Rewards are chosen outside an encounter, so a card is shown on
            // its strong side rather than whatever the last fight's noise was.
            let mut detail = card_detail_dictionary(&card_text::describe(card, 0));
            detail.set("id", id);
            described.push(&detail);
        }

        described
    }
}

#[godot_api]
impl RunDeckNode {
    /// Refills the run's deck from `starter_deck.ron`, discarding anything
    /// gained. Called when a run begins.
    #[func]
    fn reset_to_starter(&mut self) {
        let entries = starter_deck::load_starter_deck_entries();
        self.deck = RunDeck::from_starter_entries(&entries);
    }

    /// Every card the run currently owns, one entry per physical card, for
    /// `DrawPhase` to build an encounter deck from.
    #[func]
    fn card_ids(&self) -> PackedStringArray {
        self.deck
            .card_ids()
            .iter()
            .map(|id| GString::from(id.as_str()))
            .collect()
    }

    /// Whether the run owns no cards at all, which would leave an encounter
    /// with nothing to draw.
    #[func]
    fn is_empty(&self) -> bool {
        self.deck.is_empty()
    }

    #[func]
    fn size(&self) -> i32 {
        self.deck.len() as i32
    }

    #[func]
    fn count_of(&self, card_id: GString) -> i32 {
        self.deck.count_of(&card_id.to_string()) as i32
    }

    /// Adds a card to the run's deck. Returns false for an id the card
    /// database does not know, rather than storing something undrawable.
    #[func]
    fn add_card(&mut self, card_id: GString) -> bool {
        let card_id = card_id.to_string();

        let known = match self.card_database() {
            Some(db) => db.bind().get(&card_id).is_some(),
            None => {
                godot_error!("RunDeckNode: no CardDatabaseGlobal, cannot validate '{card_id}'.");
                false
            }
        };
        if !known {
            godot_error!("RunDeckNode: '{card_id}' is not a card, so it was not added.");
            return false;
        }

        self.deck.add(&card_id);
        true
    }

    /// A reward offer of up to `count` distinct cards drawn from the whole
    /// card pool, each described as `describe_cards` describes it.
    ///
    /// `guaranteed_rare` makes sure at least one of them is Rare or better,
    /// which is what clearing an elite or boss earns.
    #[func]
    fn offer_reward(&self, count: i32, guaranteed_rare: bool) -> Array<VarDictionary> {
        let Some(db) = self.card_database() else {
            godot_error!("RunDeckNode: no CardDatabaseGlobal, cannot offer a reward.");
            return Array::new();
        };

        let pool: Vec<(String, Rarity)> = db
            .bind()
            .all()
            .map(|card| (card.id.clone(), card.rarity))
            .collect();
        let count = count.max(0) as usize;
        let guaranteed = guaranteed_rare.then_some(Rarity::Rare);
        let ids = card_reward::offer_with_guarantee(&pool, count, guaranteed, &mut rand::rng());

        self.describe_reward_cards(ids.iter().map(String::as_str))
    }

    /// How many cards the reward screen offers after a fight
    /// (`card_reward::OFFER_SIZE`), which a saved reward is checked against.
    #[func]
    fn reward_offer_size() -> i64 {
        card_reward::OFFER_SIZE as i64
    }

    /// The cards of an offer already rolled, such as one restored from a save,
    /// each with what the player needs to choose between them: the card's
    /// details (see `card_detail_dictionary`) plus its `id`. An id the card
    /// database does not know is left out.
    #[func]
    fn describe_cards(&self, card_ids: PackedStringArray) -> Array<VarDictionary> {
        let ids: Vec<String> = card_ids.as_slice().iter().map(GString::to_string).collect();
        self.describe_reward_cards(ids.iter().map(String::as_str))
    }

    /// Every distinct card the run owns, for the deck viewer: the card's
    /// details (see `card_detail_dictionary`) plus its `id` and `count`, the
    /// number of copies owned. In the order the run first owned each card. An
    /// id the card database does not know is left out, as in `describe_cards`.
    #[func]
    fn deck_listing(&self) -> Array<VarDictionary> {
        let tally = self.deck.tally();
        let described = self.describe_reward_cards(tally.iter().map(|(id, _)| id.as_str()));

        // describe_reward_cards skips unknown ids, so match counts up by id
        // rather than by position.
        described
            .iter_shared()
            .map(|mut detail| {
                let id = detail
                    .get("id")
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let count = tally
                    .iter()
                    .find(|(owned, _)| *owned == id)
                    .map_or(0, |(_, count)| *count);
                detail.set("count", count as i64);
                detail
            })
            .collect()
    }
}

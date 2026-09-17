//! Thin Godot adapter for the run's deck in `run_deck.rs`.
//!
//! Lives under the `FlowCoordinator` autoload, so it outlives any one
//! encounter screen. That is the whole point: `DrawPhase` is rebuilt every time
//! the combat scene loads, so the run's cards cannot live there.

use godot::builtin::VarDictionary;
use godot::prelude::*;

use crate::card_database::CardDatabase;
use crate::card_reward;
use crate::card_text;
use crate::run_deck::RunDeck;
use crate::starter_deck;

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

    fn card_database(&self) -> Option<Gd<CardDatabase>> {
        self.base()
            .try_get_node_as::<CardDatabase>("/root/CardDatabaseGlobal")
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
    /// card pool, each with what the player needs to choose between them:
    /// `{id, name, type, rarity, cost_text, noise_text, description}`.
    #[func]
    fn offer_reward(&self, count: i32) -> Array<VarDictionary> {
        let mut offered: Array<VarDictionary> = Array::new();

        let Some(db) = self.card_database() else {
            godot_error!("RunDeckNode: no CardDatabaseGlobal, cannot offer a reward.");
            return offered;
        };
        let db = db.bind();

        let pool: Vec<String> = db.all().map(|card| card.id.clone()).collect();
        let count = count.max(0) as usize;
        let ids = card_reward::offer_from_pool(&pool, count, &mut rand::rng());

        for id in ids {
            let Some(card) = db.get(&id) else {
                continue;
            };
            // Rewards are chosen outside an encounter, so a card is shown on
            // its strong side rather than whatever the last fight's noise was.
            let detail = card_text::describe(card, 0);
            offered.push(&vdict! {
                "id" => id.as_str(),
                "name" => detail.name.as_str(),
                "type" => detail.card_type.as_str(),
                "rarity" => detail.rarity.as_str(),
                "cost_text" => detail.cost_text.as_str(),
                "noise_text" => detail.noise_text.as_str(),
                "description" => detail.description.as_str(),
            });
        }

        offered
    }
}

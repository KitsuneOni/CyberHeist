use godot::builtin::VarDictionary;
use godot::classes::FileAccess;
use godot::classes::file_access::ModeFlags;
use godot::prelude::*;

use crate::card_database::CardDatabase;
use crate::card_text;
use crate::relics::{CARDS_UNLOCKED_PER_WIN, Progression, RELIC_PRICE, Relic};
use crate::{CARD_DATABASE_PATH, failure};

pub(crate) const PROGRESSION_PATH: &str = "/root/FlowCoordinator/Progression";

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct ProgressionNode {
    progression: Progression,
    save_path: String,
    base: Base<Node>,
}

#[godot_api]
impl INode for ProgressionNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            progression: Progression::default(),
            save_path: String::new(),
            base,
        }
    }
}

impl ProgressionNode {
    pub(crate) fn progression(&self) -> &Progression {
        &self.progression
    }

    fn save(&self) {
        if self.save_path.is_empty() {
            return;
        }
        let text = match ron::to_string(&self.progression) {
            Ok(text) => text,
            Err(error) => {
                godot_error!("Progression: could not write progress: {error}");
                return;
            }
        };
        let Some(mut file) = FileAccess::open(self.save_path.as_str(), ModeFlags::WRITE) else {
            godot_error!("Progression: could not open {} to save.", self.save_path);
            return;
        };
        file.store_string(&GString::from(text.as_str()));
    }
}

#[godot_api]
impl ProgressionNode {
    #[func]
    fn use_file(&mut self, path: GString) -> VarDictionary {
        self.save_path = path.to_string();
        self.progression = Progression::default();

        if !FileAccess::file_exists(self.save_path.as_str()) {
            return vdict! { "ok" => true };
        }
        let Some(file) = FileAccess::open(self.save_path.as_str(), ModeFlags::READ) else {
            return failure("could_not_open_progress_file");
        };
        let text = file.get_as_text().to_string();
        match ron::from_str::<Progression>(&text) {
            Ok(progression) => {
                self.progression = progression.sanitized();
                vdict! { "ok" => true }
            }
            Err(error) => {
                let message = error.to_string();
                vdict! { "ok" => false, "error" => message.as_str() }
            }
        }
    }

    #[func]
    fn reset(&mut self) {
        self.progression.reset();
        self.save();
    }

    #[func]
    fn points(&self) -> i64 {
        i64::from(self.progression.points())
    }

    #[func]
    fn unlock_cards_for_win(&mut self) -> Array<VarDictionary> {
        let mut unlocked: Array<VarDictionary> = Array::new();
        let Some(db) = self
            .base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
        else {
            godot_error!("Progression: no CardDatabaseGlobal, so nothing can be unlocked.");
            return unlocked;
        };
        let db = db.bind();

        let pool: Vec<_> = db
            .all()
            .map(|card| (card.id.clone(), card.rarity))
            .collect();
        let ids =
            self.progression
                .unlock_random_cards(&pool, CARDS_UNLOCKED_PER_WIN, &mut rand::rng());
        if ids.is_empty() {
            return unlocked;
        }
        self.save();

        for id in ids {
            let Some(card) = db.get(&id) else {
                continue;
            };
            unlocked.push(&vdict! {
                "id" => id.as_str(),
                "name" => card.name.as_str(),
                "rarity" => card_text::rarity_name(card.rarity),
            });
        }
        unlocked
    }

    #[func]
    fn unlocked_card_ids(&self) -> PackedStringArray {
        self.progression
            .unlocked_cards()
            .iter()
            .map(|id| GString::from(id.as_str()))
            .collect()
    }

    #[func]
    fn award_run_completion(&mut self, credits: i64) -> i64 {
        let earned = self.progression.award_run_completion(credits);
        self.save();
        i64::from(earned)
    }

    #[func]
    fn relic_listing(&self) -> Array<VarDictionary> {
        let mut listing = Array::new();
        for relic in Relic::ALL {
            listing.push(&vdict! {
                "id" => relic.id(),
                "name" => relic.name(),
                "description" => relic.description().as_str(),
                "price" => i64::from(RELIC_PRICE),
                "owned" => self.progression.owns(relic),
                "available" => relic.is_available(),
                "affordable" => self.progression.can_buy(relic),
            });
        }
        listing
    }

    #[func]
    fn buy_relic(&mut self, id: GString) -> VarDictionary {
        let Some(relic) = Relic::from_id(&id.to_string()) else {
            return failure("unknown_relic");
        };
        match self.progression.buy(relic) {
            Ok(()) => {
                self.save();
                vdict! {
                    "ok" => true,
                    "id" => relic.id(),
                    "points" => i64::from(self.progression.points()),
                }
            }
            Err(error) => failure(error.code()),
        }
    }

    #[func]
    fn can_enter_shop(&self) -> bool {
        self.progression.can_enter_shop()
    }

    #[func]
    fn spend_points_for_shop(&mut self) -> VarDictionary {
        match self.progression.spend_points_for_shop() {
            Ok(spent) => {
                self.save();
                vdict! {
                    "ok" => true,
                    "points_spent" => i64::from(spent),
                }
            }
            Err(error) => failure(error.code()),
        }
    }

    #[func]
    fn cards_to_upgrade_at_run_start(&self) -> i64 {
        self.progression.cards_upgraded_at_run_start() as i64
    }
}

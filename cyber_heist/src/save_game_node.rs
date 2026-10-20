//! Godot adapter for the save format in `save_game.rs`.
//!
//! Lives under `FlowCoordinator` beside `RunStateNode` and `RunDeckNode`.
//! Saving gathers the run from those nodes and the shared autoloads into a
//! [`SaveGame`] and writes it as JSON; Continue puts every piece back.
//! `FlowCoordinator` decides when to save and which file to use, and holds the
//! screen results it hands in and gets back.

use std::path::PathBuf;

use godot::builtin::VarDictionary;
use godot::classes::ProjectSettings;
use godot::prelude::*;

use crate::card_data::Rarity;
use crate::card_database::CardDatabase;
use crate::card_reward;
use crate::knowledge::KnowledgeMeter;
use crate::noise_meter::NoiseMeter;
use crate::progression_node::{PROGRESSION_PATH, ProgressionNode};
use crate::run_deck::RunDeck;
use crate::run_deck_node::RunDeckNode;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode, snapshot_dictionary};
use crate::save_game::{
    CaughtPenalty, MetaProgress, PendingCardReward, SaveError, SaveGame, SavedRun, ScreenState,
    keep_unreadable_save, read_save_file, write_save_file,
};
use crate::{
    CARD_DATABASE_PATH, KNOWLEDGE_METER_PATH, NOISE_METER_PATH, PLAYER_STATE_PATH, PlayerState,
    RUN_DECK_PATH, failure,
};

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct SaveGameNode {
    /// The run read from the save at launch, waiting for Continue. Taken by
    /// `restore_saved_run`, and dropped when a new contract starts instead.
    saved_run: Option<SavedRun>,
    base: Base<Node>,
}

#[godot_api]
impl INode for SaveGameNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            saved_run: None,
            base,
        }
    }
}

#[godot_api]
impl SaveGameNode {
    /// Reads the save at `path` (a `user://` path) and applies its meta
    /// progress, the credit balance, straight away. The run is held back
    /// until Continue.
    ///
    /// Returns `{ok: true, has_run}`; `has_run` is false when there is no
    /// save yet. A corrupt, incompatible or impossible save returns
    /// `{ok: false, error}` and its run is not loaded, so the game starts
    /// fresh. The next save would replace that file, so it is first copied
    /// to `<path>.unreadable`, and if it still says what the credit balance
    /// was, that balance is kept: it is the one thing meant to carry over.
    #[func]
    fn load_save(&mut self, path: GString) -> VarDictionary {
        self.saved_run = None;
        let path = globalized(&path);

        let loaded = read_save_file(&path).and_then(|save| match save {
            Some(save) => self
                .check_against_this_build(&save.run)
                .map(|()| Some(save))
                .map_err(SaveError::InvalidRun),
            None => Ok(None),
        });
        let save = match loaded {
            Ok(Some(save)) => save,
            Ok(None) => return vdict! { "ok" => true, "has_run" => false },
            Err(error) => return self.recover_unreadable_save(&path, &error),
        };
        let Some(mut player_state) = self.player_state() else {
            return failure("PlayerStateGlobal is not available");
        };

        *player_state.bind_mut().credits_mut() = save.meta.credits;
        self.saved_run = Some(save.run);
        vdict! { "ok" => true, "has_run" => true }
    }

    /// Where the saved run waiting for Continue stands, in the shape
    /// `FlowCoordinator.run_snapshot()` uses, `{phase, active_encounter}`,
    /// plus `card_reward_waiting`: whether it was saved before a fight's card
    /// reward was chosen. The coordinator prepares the screen it belongs on
    /// before restoring anything, so a screen that cannot be prepared leaves
    /// the current run untouched. `{}` when there is no saved run.
    #[func]
    fn saved_run_snapshot(&self) -> VarDictionary {
        let Some(saved) = &self.saved_run else {
            return VarDictionary::new();
        };
        let mut snapshot = snapshot_dictionary(&saved.run_state);
        snapshot.set(
            "card_reward_waiting",
            saved.screen.pending_card_reward.is_some(),
        );
        snapshot
    }

    /// Whether `load_save` found a run that Continue can resume.
    #[func]
    fn has_saved_run(&self) -> bool {
        self.saved_run.is_some()
    }

    /// Forgets the run read at launch, for a new contract started in its
    /// place. The file itself is replaced by the next save.
    #[func]
    fn discard_saved_run(&mut self) {
        self.saved_run = None;
    }

    /// Puts the saved run back: run state (with a shop's stock), deck,
    /// upgrades, noise and knowledge. Every node is found before any is
    /// changed, so it restores all of them or none. Returns `{ok: true, pending_reward,
    /// caught_penalty}` with the screen results the coordinator was holding,
    /// each an empty dictionary when there was none, or `{ok: false, error}`
    /// with nothing changed. A won contract's reward needs neither: it is
    /// part of the run state.
    #[func]
    fn restore_saved_run(&mut self) -> VarDictionary {
        let Some(saved) = self.saved_run.take() else {
            return failure("there is no saved run to continue");
        };
        let (
            Some(mut run_state),
            Some(mut run_deck),
            Some(mut player_state),
            Some(mut noise_meter),
            Some(mut knowledge_meter),
        ) = (
            self.base().try_get_node_as::<RunStateNode>(RUN_STATE_PATH),
            self.base().try_get_node_as::<RunDeckNode>(RUN_DECK_PATH),
            self.player_state(),
            self.base().try_get_node_as::<NoiseMeter>(NOISE_METER_PATH),
            self.base()
                .try_get_node_as::<KnowledgeMeter>(KNOWLEDGE_METER_PATH),
        )
        else {
            // Kept, so Continue can be tried again.
            self.saved_run = Some(saved);
            return failure("the game's run nodes are not all available to restore into");
        };

        run_state.bind_mut().restore_state(saved.run_state);
        *run_deck.bind_mut().deck_mut() = RunDeck::from_card_ids(saved.deck);
        player_state.bind_mut().restore_upgrades(&saved.upgrades);
        noise_meter.bind_mut().restore_noise(saved.noise);
        knowledge_meter
            .bind_mut()
            .restore_knowledge(saved.knowledge);

        let mut result = screen_state_dictionary(&saved.screen);
        result.set("ok", true);
        result
    }

    /// Saves the run as it stands now, with the player's meta progress, to
    /// `path`, replacing whatever was there. `screen` holds the coordinator's
    /// screen results, `{pending_reward, caught_penalty}`, each empty when
    /// there is none.
    ///
    /// Returns `{ok: true}`, or `{ok: false, error}` with nothing written.
    /// Refused outside a safe point, i.e. during a fight or an event.
    #[func]
    fn write_save(&mut self, path: GString, screen: VarDictionary) -> VarDictionary {
        let screen = match screen_state_from(&screen) {
            Ok(screen) => screen,
            Err(error) => return failure(&error),
        };
        let save = match self.capture(screen) {
            Ok(save) => save,
            Err(error) => return failure(&error),
        };
        match write_save_file(&globalized(&path), &save) {
            Ok(()) => vdict! { "ok" => true },
            Err(error) => failure(&error.to_string()),
        }
    }
}

impl SaveGameNode {
    /// Keeps a save that would not load (see `load_save`) and reports why it
    /// would not.
    fn recover_unreadable_save(&self, path: &std::path::Path, error: &SaveError) -> VarDictionary {
        let mut message = error.to_string();
        match keep_unreadable_save(path) {
            Ok(kept) => {
                message.push_str(&format!("; a copy was kept at {}", kept.kept_at.display()));
                if let (Some(credits), Some(mut player_state)) = (kept.credits, self.player_state())
                {
                    *player_state.bind_mut().credits_mut() = credits;
                    message.push_str(", and its credit balance was kept");
                }
            }
            Err(copy_error) => {
                godot_error!("SaveGameNode: could not keep the unreadable save: {copy_error}");
            }
        }
        failure(&message)
    }

    fn capture(&self, screen: ScreenState) -> Result<SaveGame, String> {
        let unavailable = |name: &str| format!("{name} is not available to save");
        let run_state = self
            .base()
            .try_get_node_as::<RunStateNode>(RUN_STATE_PATH)
            .ok_or_else(|| unavailable("RunState"))?;
        let run_deck = self
            .base()
            .try_get_node_as::<RunDeckNode>(RUN_DECK_PATH)
            .ok_or_else(|| unavailable("RunDeck"))?;
        let player_state = self
            .player_state()
            .ok_or_else(|| unavailable("PlayerStateGlobal"))?;
        let noise_meter = self
            .base()
            .try_get_node_as::<NoiseMeter>(NOISE_METER_PATH)
            .ok_or_else(|| unavailable("NoiseMeterGlobal"))?;
        let knowledge_meter = self
            .base()
            .try_get_node_as::<KnowledgeMeter>(KNOWLEDGE_METER_PATH)
            .ok_or_else(|| unavailable("KnowledgeMeterGlobal"))?;

        let run_state = run_state.bind().state().clone();
        let player_state = player_state.bind();

        let run = SavedRun {
            run_state,
            deck: run_deck.bind().owned_card_ids(),
            upgrades: player_state.upgrade_names(),
            noise: noise_meter.bind().get_noise(),
            knowledge: knowledge_meter.bind().get_knowledge(),
            screen,
        };
        let meta = MetaProgress {
            credits: player_state.money(),
        };
        SaveGame::new(meta, run).map_err(|error| error.to_string())
    }

    /// The checks a save cannot make without the card database: every card
    /// it names, in the deck, a shop's stock or a card reward's offer, exists
    /// in this build; a shop's cards are priced by their rarity; and a reward
    /// that guarantees a Rare offers one. (Noise and knowledge are checked
    /// against their caps by `SavedRun::validate`, which needs no nodes.)
    fn check_against_this_build(&self, run: &SavedRun) -> Result<(), String> {
        let database = self
            .base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
            .ok_or("CardDatabaseGlobal is not available to check its cards")?;
        let database = database.bind();
        let rarity_of = |card_id: &str| database.get(card_id).map(|card| card.rarity);

        if let Some(visit) = run.run_state.shop_visit() {
            visit.validate_restored(rarity_of)?;
        }
        if let Some(reward) = &run.screen.pending_card_reward
            && reward.guaranteed_rare
            && !reward.offered_card_ids.is_empty()
        {
            // The guarantee is the best card that was open when the offer was
            // rolled, not always a Rare (see `attainable_guarantee`).
            let all: Vec<(String, Rarity)> = database
                .all()
                .map(|card| (card.id.clone(), card.rarity))
                .collect();
            let open = match self
                .base()
                .try_get_node_as::<ProgressionNode>(PROGRESSION_PATH)
            {
                Some(node) => node.bind().progression().open_pool(all),
                None => all,
            };
            if let Some(minimum) = card_reward::attainable_guarantee(&open, Rarity::Rare)
                && !reward
                    .offered_card_ids
                    .iter()
                    .any(|id| rarity_of(id).is_some_and(|rarity| rarity >= minimum))
            {
                return Err("a card reward's guaranteed card is missing from its offer".to_string());
            }
        }

        let shop_cards = run
            .run_state
            .shop_visit()
            .into_iter()
            .flat_map(|visit| visit.slots())
            .map(|slot| slot.card_id());
        let reward_cards = run
            .screen
            .pending_card_reward
            .iter()
            .flat_map(|reward| reward.offered_card_ids.iter().map(String::as_str));
        let deck_cards = run.deck.iter().map(String::as_str);
        if let Some(unknown) = deck_cards
            .chain(shop_cards)
            .chain(reward_cards)
            .find(|id| database.get(id).is_none())
        {
            return Err(format!("it names a card that does not exist: '{unknown}'"));
        }
        Ok(())
    }

    fn player_state(&self) -> Option<Gd<PlayerState>> {
        self.base()
            .try_get_node_as::<PlayerState>(PLAYER_STATE_PATH)
    }
}

/// `user://save.json` and friends as a path on disk.
fn globalized(path: &GString) -> PathBuf {
    let absolute = ProjectSettings::singleton().globalize_path(path);
    PathBuf::from(absolute.to_string())
}

/// The coordinator's screen results as GDScript holds them, each an empty
/// dictionary when there is none.
fn screen_state_dictionary(screen: &ScreenState) -> VarDictionary {
    let pending_reward = match &screen.pending_card_reward {
        Some(reward) => {
            let offered: Array<GString> = reward
                .offered_card_ids
                .iter()
                .map(|id| GString::from(id.as_str()))
                .collect();
            vdict! {
                "credits" => reward.credits,
                "guaranteed_rare" => reward.guaranteed_rare,
                "offered_card_ids" => &offered,
            }
        }
        None => VarDictionary::new(),
    };
    let caught_penalty = match &screen.caught_penalty {
        Some(penalty) => {
            let lost: Array<GString> = penalty
                .lost_upgrades
                .iter()
                .map(|name| GString::from(name.as_str()))
                .collect();
            vdict! {
                "fine" => penalty.fine,
                "lost_upgrades" => &lost,
            }
        }
        None => VarDictionary::new(),
    };

    vdict! {
        "pending_reward" => &pending_reward,
        "caught_penalty" => &caught_penalty,
    }
}

/// Reads the screen results `FlowCoordinator` hands to `write_save`.
fn screen_state_from(screen: &VarDictionary) -> Result<ScreenState, String> {
    let pending_card_reward = match sub_dictionary(screen, "pending_reward")? {
        Some(reward) => Some(PendingCardReward {
            credits: field(&reward, "credits")?,
            guaranteed_rare: field(&reward, "guaranteed_rare")?,
            // Missing until the reward screen has rolled its offer.
            offered_card_ids: if reward.contains_key("offered_card_ids") {
                string_list(&reward, "offered_card_ids")?
            } else {
                Vec::new()
            },
        }),
        None => None,
    };
    let caught_penalty = match sub_dictionary(screen, "caught_penalty")? {
        Some(penalty) => Some(CaughtPenalty {
            fine: field(&penalty, "fine")?,
            lost_upgrades: string_list(&penalty, "lost_upgrades")?,
        }),
        None => None,
    };

    Ok(ScreenState {
        pending_card_reward,
        caught_penalty,
    })
}

/// The dictionary under `key`, or `None` when it is missing or empty.
fn sub_dictionary(dictionary: &VarDictionary, key: &str) -> Result<Option<VarDictionary>, String> {
    let Some(value) = dictionary.get(key) else {
        return Ok(None);
    };
    let value: VarDictionary = value
        .try_to()
        .map_err(|_| format!("'{key}' is not a dictionary"))?;
    Ok((!value.is_empty()).then_some(value))
}

fn field<T: FromGodot>(dictionary: &VarDictionary, key: &str) -> Result<T, String> {
    dictionary
        .get(key)
        .ok_or_else(|| format!("'{key}' is missing"))?
        .try_to()
        .map_err(|_| format!("'{key}' has the wrong type"))
}

/// A list of names, typed or untyped, as GDScript may hold either. Anything
/// in it that is not a string (a null id, say) is refused here, at save time,
/// rather than written out as text that would make the save unreadable later.
fn string_list(dictionary: &VarDictionary, key: &str) -> Result<Vec<String>, String> {
    let value = dictionary
        .get(key)
        .ok_or_else(|| format!("'{key}' is missing"))?;
    if let Ok(names) = value.try_to::<Array<GString>>() {
        return Ok(names.iter_shared().map(|name| name.to_string()).collect());
    }
    let names: VarArray = value
        .try_to()
        .map_err(|_| format!("'{key}' is not a list"))?;
    names
        .iter_shared()
        .map(|name| {
            name.try_to::<GString>()
                .map(|name| name.to_string())
                .map_err(|_| format!("'{key}' holds something that is not a name: {name}"))
        })
        .collect()
}

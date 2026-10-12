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

use crate::card_database::CardDatabase;
use crate::contract_map::EncounterType;
use crate::knowledge::KnowledgeMeter;
use crate::noise_meter::NoiseMeter;
use crate::run_deck::RunDeck;
use crate::run_deck_node::RunDeckNode;
use crate::run_state::ContractPhase;
use crate::run_state_node::{RUN_STATE_PATH, RunStateNode};
use crate::save_game::{
    CaughtPenalty, ContractPayout, MetaProgress, PendingCardReward, SaveGame, SavedRun,
    ScreenState, read_save_file, write_save_file,
};
use crate::shop::ShopVisit;
use crate::{PlayerState, RUN_DECK_PATH};

/// Where `FlowCoordinator` keeps this node. `ShopNode` looks it up here and
/// treats it as optional, so the shop screen still runs on its own.
pub(crate) const SAVE_GAME_PATH: &str = "/root/FlowCoordinator/SaveGame";

const PLAYER_STATE_PATH: &str = "/root/PlayerStateGlobal";
const NOISE_METER_PATH: &str = "/root/NoiseMeterGlobal";
const KNOWLEDGE_METER_PATH: &str = "/root/KnowledgeMeterGlobal";
const CARD_DATABASE_PATH: &str = "/root/CardDatabaseGlobal";

#[derive(GodotClass)]
#[class(base=Node)]
pub(crate) struct SaveGameNode {
    /// The run read from the save at launch, waiting for Continue. Taken by
    /// `restore_saved_run`, and dropped when a new contract starts instead.
    saved_run: Option<SavedRun>,
    /// The stock of the shop screen now open, kept up to date by `ShopNode`
    /// so a save made in the shop includes it.
    open_shop: Option<ShopVisit>,
    /// The stock of a shop being resumed by Continue, for the shop screen's
    /// `ShopNode` to take instead of rolling a new one.
    resumed_shop: Option<ShopVisit>,
    base: Base<Node>,
}

#[godot_api]
impl INode for SaveGameNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            saved_run: None,
            open_shop: None,
            resumed_shop: None,
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
    /// `{ok: false, error}` and loads nothing, so the game starts fresh.
    #[func]
    fn load_save(&mut self, path: GString) -> VarDictionary {
        self.saved_run = None;

        let save = match read_save_file(&globalized(&path)) {
            Ok(Some(save)) => save,
            Ok(None) => return vdict! { "ok" => true, "has_run" => false },
            Err(error) => return failure(&error.to_string()),
        };
        if let Err(error) = self.check_against_this_build(&save.run) {
            return failure(&format!("the saved run is invalid: {error}"));
        }
        let Some(mut player_state) = self.player_state() else {
            return failure("PlayerStateGlobal is not available");
        };

        *player_state.bind_mut().credits_mut() = save.meta.credits;
        self.saved_run = Some(save.run);
        vdict! { "ok" => true, "has_run" => true }
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

    /// Puts the saved run back: run state, deck, upgrades, noise, knowledge
    /// and, in a shop, its stock. Returns `{ok: true, pending_reward,
    /// contract_payout, caught_penalty}` with the screen results the
    /// coordinator was holding, each an empty dictionary when there was none,
    /// or `{ok: false, error}` with nothing changed.
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
        self.resumed_shop = saved.shop_visit;

        let mut result = screen_state_dictionary(&saved.screen);
        result.set("ok", true);
        result
    }

    /// Saves the run as it stands now, with the player's meta progress, to
    /// `path`, replacing whatever was there. `screen` holds the coordinator's
    /// screen results, `{pending_reward, contract_payout, caught_penalty}`,
    /// each empty when there is none.
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
    /// Called by `ShopNode` when the shop opens and after every purchase, so
    /// a save made in the shop has its current stock.
    pub(crate) fn track_open_shop(&mut self, visit: &ShopVisit) {
        self.open_shop = Some(visit.clone());
    }

    /// The stock of the shop Continue is resuming, if any. Taken, so only the
    /// first shop screen after Continue reopens it.
    pub(crate) fn take_resumed_shop(&mut self) -> Option<ShopVisit> {
        self.resumed_shop.take()
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
        let in_shop = run_state.phase() == ContractPhase::EncounterActive
            && run_state
                .active_encounter()
                .is_some_and(|encounter| encounter.encounter_type() == EncounterType::Shop);
        let player_state = player_state.bind();

        let run = SavedRun {
            run_state,
            deck: run_deck.bind().owned_card_ids(),
            upgrades: player_state.upgrade_names(),
            noise: noise_meter.bind().get_noise(),
            knowledge: knowledge_meter.bind().get_knowledge(),
            shop_visit: if in_shop {
                self.open_shop.clone()
            } else {
                None
            },
            screen,
        };
        let meta = MetaProgress {
            credits: player_state.money(),
        };
        SaveGame::new(meta, run).map_err(|error| error.to_string())
    }

    /// The checks a save cannot make on its own: every card it names exists
    /// in this build's card database, and noise and knowledge are within the
    /// meters' caps.
    fn check_against_this_build(&self, run: &SavedRun) -> Result<(), String> {
        let database = self
            .base()
            .try_get_node_as::<CardDatabase>(CARD_DATABASE_PATH)
            .ok_or("CardDatabaseGlobal is not available to check its cards")?;
        let database = database.bind();

        let shop_cards = run
            .shop_visit
            .iter()
            .flat_map(|visit| visit.slots())
            .map(|slot| slot.card_id());
        let deck_cards = run.deck.iter().map(String::as_str);
        if let Some(unknown) = deck_cards
            .chain(shop_cards)
            .find(|id| database.get(id).is_none())
        {
            return Err(format!("it names a card that does not exist: '{unknown}'"));
        }

        if let Some(meter) = self.base().try_get_node_as::<NoiseMeter>(NOISE_METER_PATH)
            && run.noise > meter.bind().get_max_noise()
        {
            return Err(format!("noise {} is above the meter's cap", run.noise));
        }
        if let Some(meter) = self
            .base()
            .try_get_node_as::<KnowledgeMeter>(KNOWLEDGE_METER_PATH)
            && run.knowledge > meter.bind().get_max_knowledge()
        {
            return Err(format!("knowledge {} is above its cap", run.knowledge));
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
        Some(reward) => vdict! {
            "credits" => reward.credits,
            "guaranteed_rare" => reward.guaranteed_rare,
        },
        None => VarDictionary::new(),
    };
    let contract_payout = match &screen.contract_payout {
        Some(payout) => vdict! {
            "credits" => payout.credits,
            "balance_before" => payout.balance_before,
            "balance_after" => payout.balance_after,
        },
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
        "contract_payout" => &contract_payout,
        "caught_penalty" => &caught_penalty,
    }
}

/// Reads the screen results `FlowCoordinator` hands to `write_save`.
fn screen_state_from(screen: &VarDictionary) -> Result<ScreenState, String> {
    let pending_card_reward = match sub_dictionary(screen, "pending_reward")? {
        Some(reward) => Some(PendingCardReward {
            credits: field(&reward, "credits")?,
            guaranteed_rare: field(&reward, "guaranteed_rare")?,
        }),
        None => None,
    };
    let contract_payout = match sub_dictionary(screen, "contract_payout")? {
        Some(payout) => Some(ContractPayout {
            credits: field(&payout, "credits")?,
            balance_before: field(&payout, "balance_before")?,
            balance_after: field(&payout, "balance_after")?,
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
        contract_payout,
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

/// A list of names, typed or untyped, as GDScript may hold either.
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
    Ok(names.iter_shared().map(|name| name.to_string()).collect())
}

fn failure(message: &str) -> VarDictionary {
    vdict! {
        "ok" => false,
        "error" => message,
    }
}

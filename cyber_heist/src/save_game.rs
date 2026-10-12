//! Saving the game between sessions (story #10).
//!
//! Plain Rust, so the save format and its rules can be tested without Godot.
//! `save_game_node.rs` gathers the pieces from the scene tree, hands them to
//! this module and puts them back again on Continue.
//!
//! A save is one JSON file holding two things:
//!
//! - **Meta progress**: what the player keeps between runs. Today that is the
//!   credit balance; upgrades only last for the contract they were earned on
//!   and zones are unlocked per contract, so both belong to the run.
//! - **The run**: everything needed to put the current run back exactly as it
//!   was at its last safe point (see [`SavedRun`]).
//!
//! A run is only saved at a safe point: on the map, in a shop, on the caught
//! screen or on the victory screen. Never in the middle of a fight or an
//! event, so quitting during one resumes from the safe point before it.
//!
//! Every save carries a `version`. A file written with a different version,
//! one that is not valid JSON, or one describing a run the game could never
//! have reached is refused with a [`SaveError`] rather than half-loaded.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::contract_map::EncounterType;
use crate::run_state::{ContractPhase, RunState};
use crate::shop::ShopVisit;

/// The layout of the save file. Bump it whenever a change to anything saved
/// would stop an older file reading back correctly; older files are then
/// refused, and the player starts fresh, rather than being misread.
pub const SAVE_VERSION: u32 = 1;

/// Everything written to the save file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveGame {
    /// Which layout this file uses. Always [`SAVE_VERSION`] when written.
    pub version: u32,
    pub meta: MetaProgress,
    pub run: SavedRun,
}

/// What the player keeps from one run to the next.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetaProgress {
    /// The player's credit balance. Contract rewards and leftover pay carry
    /// over into the next run, so the balance is the player's, not the run's.
    pub credits: i64,
}

/// The current run, as it stood at its last safe point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRun {
    /// The contract lifecycle: phase, contract offers and the accepted one,
    /// the map with its completed and locked nodes and open zones, and the
    /// run's stats.
    pub run_state: RunState,
    /// Card ids the run's deck owns, starter cards and gains alike.
    pub deck: Vec<String>,
    /// Upgrades earned on this contract, in the order they were gained.
    pub upgrades: Vec<String>,
    /// Shared detection noise, which carries from one encounter to the next.
    pub noise: i32,
    /// What the player has learned about the target.
    pub knowledge: i32,
    /// The stock of the shop the player is in. Present exactly when the run
    /// was saved during a shop visit.
    pub shop_visit: Option<ShopVisit>,
    /// What the screen the run was saved on needs to show the same thing
    /// again when it is resumed.
    pub screen: ScreenState,
}

/// Results `FlowCoordinator` holds for the screen after an encounter. They
/// were already applied to the run when it was saved, so a resumed screen
/// shows them again rather than paying or charging a second time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenState {
    /// The card reward still waiting to be chosen after a fight.
    pub pending_card_reward: Option<PendingCardReward>,
    /// What winning the contract paid, for the victory screen.
    pub contract_payout: Option<ContractPayout>,
    /// The fine taken and upgrades lost on being caught.
    pub caught_penalty: Option<CaughtPenalty>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCardReward {
    /// Credits the fight paid, already in the balance.
    pub credits: i64,
    /// Whether the offer must include a Rare (elites and bosses).
    pub guaranteed_rare: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractPayout {
    pub credits: i64,
    pub balance_before: i64,
    pub balance_after: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaughtPenalty {
    pub fine: i64,
    pub lost_upgrades: Vec<String>,
}

/// Why a save could not be written or read back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The file is not JSON, or not a save in this layout.
    Corrupt(String),
    /// The file was written with a different save layout.
    IncompatibleVersion { found: u64, expected: u32 },
    /// The file parses, but the run it describes breaks the game's rules or
    /// was not at a safe point.
    InvalidRun(String),
    /// The file could not be read or written.
    Io(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(reason) => write!(formatter, "the save file is corrupt: {reason}"),
            Self::IncompatibleVersion { found, expected } => write!(
                formatter,
                "the save file is version {found}, but this build reads version {expected}"
            ),
            Self::InvalidRun(reason) => write!(formatter, "the saved run is invalid: {reason}"),
            Self::Io(reason) => write!(formatter, "the save file could not be accessed: {reason}"),
        }
    }
}

impl SaveGame {
    /// A save of `run` and `meta` at the current version. Refused unless the
    /// run is at a safe point and holds together, so a save that could not be
    /// read back is never written.
    pub fn new(meta: MetaProgress, run: SavedRun) -> Result<Self, SaveError> {
        run.validate()?;
        Ok(Self {
            version: SAVE_VERSION,
            meta,
            run,
        })
    }

    pub fn to_json(&self) -> Result<String, SaveError> {
        serde_json::to_string_pretty(self).map_err(|error| SaveError::Corrupt(error.to_string()))
    }

    /// Reads a save back from its JSON.
    ///
    /// The version is checked before anything else, so a save from another
    /// layout is reported as incompatible rather than as corrupt. The run is
    /// then checked as `new` checks it.
    pub fn from_json(json: &str) -> Result<Self, SaveError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| SaveError::Corrupt(error.to_string()))?;

        let found = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| SaveError::Corrupt("it has no version".to_string()))?;
        if found != u64::from(SAVE_VERSION) {
            return Err(SaveError::IncompatibleVersion {
                found,
                expected: SAVE_VERSION,
            });
        }

        let save: SaveGame =
            serde_json::from_value(value).map_err(|error| SaveError::Corrupt(error.to_string()))?;
        save.run.validate()?;
        Ok(save)
    }
}

impl SavedRun {
    /// Whether the run is at a safe point and describes a state the game
    /// could have reached.
    pub fn validate(&self) -> Result<(), SaveError> {
        self.run_state
            .validate_restored()
            .map_err(|error| SaveError::InvalidRun(error.to_string()))?;
        self.validate_safe_point()?;
        self.validate_screen_state()?;

        if self.noise < 0 {
            return Err(invalid("noise is negative"));
        }
        if self.knowledge < 0 {
            return Err(invalid("knowledge is negative"));
        }
        if let Some(visit) = &self.shop_visit
            && visit.slots().iter().any(|slot| slot.price() < 0)
        {
            return Err(invalid("a shop price is negative"));
        }
        Ok(())
    }

    /// The only encounter a run can be saved in is a shop, and then the
    /// shop's stock has to be saved with it.
    fn validate_safe_point(&self) -> Result<(), SaveError> {
        let in_shop = self.run_state.phase() == ContractPhase::EncounterActive
            && self
                .run_state
                .active_encounter()
                .is_some_and(|encounter| encounter.encounter_type() == EncounterType::Shop);

        if self.run_state.phase() == ContractPhase::EncounterActive && !in_shop {
            return Err(invalid(
                "it was saved during a fight or an event, which is not a safe point",
            ));
        }
        if in_shop != self.shop_visit.is_some() {
            return Err(invalid(
                "a shop's stock is saved exactly when the run is in a shop",
            ));
        }
        Ok(())
    }

    /// Each screen result belongs to the phase whose screen shows it.
    fn validate_screen_state(&self) -> Result<(), SaveError> {
        let phase = self.run_state.phase();
        let screen = &self.screen;

        if screen.contract_payout.is_some() && phase != ContractPhase::ContractWon {
            return Err(invalid("a contract payout is saved for a contract not won"));
        }
        if screen.caught_penalty.is_some() && phase != ContractPhase::Caught {
            return Err(invalid("a caught penalty is saved for a run not caught"));
        }
        // The victory screen also shows the final boss's own payout.
        let reward_phase = matches!(phase, ContractPhase::Hub | ContractPhase::ContractWon);
        if screen.pending_card_reward.is_some() && !reward_phase {
            return Err(invalid("a card reward is saved outside the hub"));
        }
        Ok(())
    }
}

fn invalid(reason: &str) -> SaveError {
    SaveError::InvalidRun(reason.to_string())
}

/// Reads the save at `path`. `Ok(None)` when there is no save there yet,
/// which is how every first launch starts.
pub fn read_save_file(path: &Path) -> Result<Option<SaveGame>, SaveError> {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(SaveError::Io(error.to_string())),
    };
    SaveGame::from_json(&json).map(Some)
}

/// Writes `save` to `path`, replacing any save already there.
///
/// The JSON goes to a temporary file beside it first and is then renamed over
/// the old save, so the game closing mid-write leaves the previous save
/// intact rather than a half-written one.
pub fn write_save_file(path: &Path, save: &SaveGame) -> Result<(), SaveError> {
    let json = save.to_json()?;
    let temporary = temporary_path(path);
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| SaveError::Io(error.to_string()))?;
    }

    fs::write(&temporary, json).map_err(|error| SaveError::Io(error.to_string()))?;
    fs::rename(&temporary, path).map_err(|error| SaveError::Io(error.to_string()))
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use serde_json::{Value, json};

    use super::*;
    use crate::card_data::Rarity;

    /// A run that has accepted an offer, won its first fight and banked
    /// some credits and a card, the way a few minutes of play leaves it.
    fn run_played_partway() -> RunState {
        let mut run = RunState::generated(&mut StdRng::seed_from_u64(7));
        run.accept_contract_offer(1).expect("second offer");

        let first = run.selectable_encounters()[0].id();
        run.select_encounter(first).expect("first encounter");
        run.complete_encounter().expect("complete it");
        run.record_credit_change(25);
        run.record_card_added();
        run
    }

    fn saved_run(run_state: RunState) -> SavedRun {
        SavedRun {
            run_state,
            deck: vec![
                "strike".to_string(),
                "strike".to_string(),
                "vpn".to_string(),
            ],
            upgrades: vec!["Lucky Charm".to_string()],
            noise: 23,
            knowledge: 2,
            shop_visit: None,
            screen: ScreenState::default(),
        }
    }

    fn save_of(run_state: RunState) -> SaveGame {
        SaveGame::new(MetaProgress { credits: 140 }, saved_run(run_state))
            .expect("a run at the hub can be saved")
    }

    /// The JSON of a valid save, as a value a test can then tamper with.
    fn save_json_value() -> Value {
        let json = save_of(run_played_partway()).to_json().unwrap();
        serde_json::from_str(&json).unwrap()
    }

    /// Steps into the first shop a fresh run can reach, if any.
    fn run_in_a_shop() -> RunState {
        for seed in 0..200 {
            let mut run = RunState::generated(&mut StdRng::seed_from_u64(seed));
            let shop = run
                .selectable_encounters()
                .into_iter()
                .find(|node| node.encounter_type() == EncounterType::Shop)
                .map(|node| node.id());
            if let Some(shop) = shop {
                run.select_encounter(shop).expect("enter the shop");
                return run;
            }
        }
        panic!("no seed offered a shop straight away");
    }

    fn shop_visit() -> ShopVisit {
        let pool = vec![
            ("vpn".to_string(), Rarity::Common),
            ("proxy_chain".to_string(), Rarity::Rare),
        ];
        ShopVisit::open(&pool, &mut StdRng::seed_from_u64(3))
    }

    #[test]
    fn a_run_saved_partway_reads_back_exactly() {
        let save = save_of(run_played_partway());

        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(restored, save);
        assert_eq!(restored.run.run_state.phase(), ContractPhase::Hub);
        assert_eq!(restored.run.run_state.progress().completed, 1);
        assert_eq!(
            restored.run.run_state.accepted_offer_index(),
            Some(1),
            "the accepted contract is remembered"
        );
    }

    /// The point of restoring exactly: the resumed run carries on the way
    /// the original would have.
    #[test]
    fn a_restored_run_plays_on_like_the_original() {
        let mut original = run_played_partway();
        let mut restored = SaveGame::from_json(&save_of(original.clone()).to_json().unwrap())
            .unwrap()
            .run
            .run_state;

        let next = original.selectable_encounters()[0].id();
        original.select_encounter(next).unwrap();
        restored.select_encounter(next).unwrap();
        original.complete_encounter().unwrap();
        restored.complete_encounter().unwrap();

        assert_eq!(restored, original);
        assert_eq!(restored.node_progress(), original.node_progress());
    }

    #[test]
    fn a_save_records_its_version() {
        let value = save_json_value();

        assert_eq!(value["version"], json!(SAVE_VERSION));
    }

    #[test]
    fn a_save_from_another_version_is_refused_as_incompatible() {
        let mut value = save_json_value();
        value["version"] = json!(SAVE_VERSION + 1);

        assert_eq!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::IncompatibleVersion {
                found: u64::from(SAVE_VERSION) + 1,
                expected: SAVE_VERSION,
            })
        );
    }

    /// An older layout is told apart from a corrupt file even when the rest
    /// of it no longer parses, since the version is read first.
    #[test]
    fn an_old_layout_is_incompatible_rather_than_corrupt() {
        let old_save = json!({ "version": 0, "credits": 50 });

        assert!(matches!(
            SaveGame::from_json(&old_save.to_string()),
            Err(SaveError::IncompatibleVersion { found: 0, .. })
        ));
    }

    #[test]
    fn a_save_without_a_version_is_corrupt() {
        let mut value = save_json_value();
        value.as_object_mut().unwrap().remove("version");

        assert!(matches!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn text_that_is_not_a_save_is_corrupt() {
        let truncated = {
            let json = save_of(run_played_partway()).to_json().unwrap();
            json[..json.len() / 2].to_string()
        };

        for text in ["", "not json", "{", "[1, 2, 3]", "null", truncated.as_str()] {
            assert!(
                matches!(SaveGame::from_json(text), Err(SaveError::Corrupt(_))),
                "{text:?} was not reported as corrupt"
            );
        }
    }

    #[test]
    fn a_save_missing_part_of_the_run_is_corrupt() {
        let mut value = save_json_value();
        value["run"].as_object_mut().unwrap().remove("deck");

        assert!(matches!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::Corrupt(_))
        ));
    }

    /// What a tampering does to a save, and how the test describes it.
    type Tampering = (&'static str, fn(&mut Value));

    /// A file can be edited by hand or damaged on disk and still be valid
    /// JSON. Each of these would break a rule the game relies on, some badly
    /// enough to panic, so each is refused.
    #[test]
    fn a_save_describing_an_impossible_run_is_refused() {
        let tamperings: [Tampering; 6] = [
            ("current node off the map", |value| {
                value["run"]["run_state"]["contract_map"]["current_node"] = json!(999);
            }),
            ("completed node off the map", |value| {
                value["run"]["run_state"]["contract_map"]["completed_nodes"] = json!([0, 999]);
            }),
            ("accepted offer that was never offered", |value| {
                value["run"]["run_state"]["accepted_offer_index"] = json!(7);
            }),
            ("caught without an encounter", |value| {
                value["run"]["run_state"]["phase"] = json!("caught");
            }),
            ("more zones open than the contract has", |value| {
                value["run"]["run_state"]["contract_map"]["unlocked_zones"] = json!(99);
            }),
            ("negative noise", |value| {
                value["run"]["noise"] = json!(-5);
            }),
        ];

        for (description, tamper) in tamperings {
            let mut value = save_json_value();
            tamper(&mut value);

            assert!(
                matches!(
                    SaveGame::from_json(&value.to_string()),
                    Err(SaveError::InvalidRun(_))
                ),
                "a save with {description} was not refused"
            );
        }
    }

    #[test]
    fn a_run_in_the_middle_of_a_fight_is_not_saved() {
        let mut run = RunState::new();
        run.select_encounter(1).expect("node 1 is a fight");

        assert!(matches!(
            SaveGame::new(MetaProgress::default(), saved_run(run)),
            Err(SaveError::InvalidRun(_))
        ));
    }

    #[test]
    fn a_shop_visit_is_saved_with_its_stock_and_sales() {
        let mut visit = shop_visit();
        let mut credits = 500;
        let mut deck = crate::run_deck::RunDeck::default();
        visit.buy(0, &mut credits, &mut deck).expect("buy slot 0");

        let mut run = saved_run(run_in_a_shop());
        run.shop_visit = Some(visit.clone());
        let save = SaveGame::new(MetaProgress { credits }, run).expect("a shop is a safe point");

        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(restored.run.shop_visit, Some(visit));
        assert!(restored.run.shop_visit.unwrap().slots()[0].is_sold());
    }

    #[test]
    fn a_shop_visit_without_its_stock_is_refused() {
        let run = saved_run(run_in_a_shop());

        assert!(matches!(
            SaveGame::new(MetaProgress::default(), run),
            Err(SaveError::InvalidRun(_))
        ));
    }

    #[test]
    fn screen_results_are_kept_only_for_their_own_screen() {
        let mut run = saved_run(run_played_partway());
        run.screen.caught_penalty = Some(CaughtPenalty {
            fine: 50,
            lost_upgrades: Vec::new(),
        });

        assert!(matches!(
            SaveGame::new(MetaProgress::default(), run),
            Err(SaveError::InvalidRun(_))
        ));
    }

    #[test]
    fn a_caught_run_keeps_its_penalty_so_it_is_not_charged_twice() {
        let mut caught = RunState::new();
        caught.select_encounter(1).unwrap();
        caught.report_caught().unwrap();
        let mut run = saved_run(caught);
        run.screen.caught_penalty = Some(CaughtPenalty {
            fine: 50,
            lost_upgrades: vec!["Lucky Charm".to_string()],
        });
        let save = SaveGame::new(MetaProgress::default(), run).expect("caught is a safe point");

        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(restored.run.run_state.phase(), ContractPhase::Caught);
        assert_eq!(
            restored.run.screen.caught_penalty,
            save.run.screen.caught_penalty
        );
    }

    /// A private, empty directory for one file test.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cyber_heist_save_test_{}_{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reading_where_there_is_no_save_finds_none() {
        let dir = scratch_dir("missing");

        assert_eq!(read_save_file(&dir.join("save.json")), Ok(None));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_written_save_reads_back_and_a_later_one_replaces_it() {
        let dir = scratch_dir("round_trip");
        let path = dir.join("save.json");
        let first = save_of(run_played_partway());
        let second = save_of(RunState::generated(&mut StdRng::seed_from_u64(11)));

        write_save_file(&path, &first).unwrap();
        assert_eq!(read_save_file(&path), Ok(Some(first)));

        write_save_file(&path, &second).unwrap();
        assert_eq!(read_save_file(&path), Ok(Some(second)));
        assert!(
            !temporary_path(&path).exists(),
            "the temporary file is renamed into place, not left behind"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_on_disk_is_reported_rather_than_read() {
        let dir = scratch_dir("corrupt");
        let path = dir.join("save.json");
        fs::write(&path, "{\"version\": 1, \"meta\": ").unwrap();

        assert!(matches!(read_save_file(&path), Err(SaveError::Corrupt(_))));

        fs::remove_dir_all(dir).unwrap();
    }
}

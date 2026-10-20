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

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::card_reward;
use crate::encounter_reward;
use crate::knowledge::MAX_KNOWLEDGE;
use crate::noise_meter::MAX_NOISE;
use crate::run_state::{ContractPhase, RunState};

/// The layout of the save file. Bump it whenever a change to anything saved
/// would stop an older file reading back correctly; older files are then
/// refused, and the player starts fresh, rather than being misread.
///
/// Version 2 keeps a won contract's reward in the run state
/// (`contract_reward`). Version 1 kept a claimed flag there and the payout
/// beside the screen, so a won run saved by it cannot be read back.
///
/// Version 3 keeps a shop's stock in the run state (`shop_visit`), where the
/// shop itself now keeps it, rather than beside the run.
///
/// Version 4 adds safehouse nodes to generated contracts, so an offer's seed
/// lays out a different map than it did before and an older save's map would
/// no longer match the contract it claims to be on.
pub const SAVE_VERSION: u32 = 4;

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
    /// whether the contract has started (until it has, the player can still
    /// change it), the map with its completed and locked nodes and open
    /// zones, the reward recorded for a won contract, the run's stats and, in
    /// a shop, its stock.
    pub run_state: RunState,
    /// Card ids the run's deck owns, starter cards and gains alike.
    pub deck: Vec<String>,
    /// Upgrades earned on this contract, in the order they were gained.
    pub upgrades: Vec<String>,
    /// Shared detection noise, which carries from one encounter to the next.
    pub noise: i32,
    /// What the player has learned about the target.
    pub knowledge: i32,
    /// What the screen the run was saved on needs to show the same thing
    /// again when it is resumed.
    pub screen: ScreenState,
}

/// Results `FlowCoordinator` holds for the screen after an encounter. They
/// were already applied to the run when it was saved, so a resumed screen
/// shows them again rather than paying or charging a second time.
///
/// A won contract's reward is not among them: run state records it, so it is
/// saved with the run (see `RunState::contract_reward`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenState {
    /// The card reward still waiting to be chosen after a fight.
    pub pending_card_reward: Option<PendingCardReward>,
    /// The fine taken and upgrades lost on being caught.
    pub caught_penalty: Option<CaughtPenalty>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCardReward {
    /// Credits the fight paid, already in the balance.
    pub credits: i64,
    /// Whether the offer must include a Rare (elites and bosses).
    pub guaranteed_rare: bool,
    /// The cards the reward screen offered, so a resumed reward offers the
    /// same ones rather than a free reroll. Empty when no offer was shown,
    /// as on the victory screen, which offers no card.
    pub offered_card_ids: Vec<String>,
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

        // Checked against the caps the meters are built with, not the meter
        // nodes, so the check holds however early in launch the save is read.
        if !(0..=MAX_NOISE).contains(&self.noise) {
            return Err(invalid("noise is outside the meter's range"));
        }
        if !(0..=MAX_KNOWLEDGE).contains(&self.knowledge) {
            return Err(invalid("knowledge is outside its range"));
        }
        Ok(())
    }

    /// The only encounter a run can be saved in is a shop, and then the
    /// shop's stock has to be saved with it: it is opened as the shop screen
    /// appears, before the save made on entering, so a shop saved without it
    /// would roll new stock on Continue.
    fn validate_safe_point(&self) -> Result<(), SaveError> {
        let in_shop = self.run_state.is_in_shop();

        if self.run_state.phase() == ContractPhase::EncounterActive && !in_shop {
            return Err(invalid(
                "it was saved during a fight or an event, which is not a safe point",
            ));
        }
        if in_shop && self.run_state.shop_visit().is_none() {
            return Err(invalid("a run in a shop is saved without the shop's stock"));
        }
        Ok(())
    }

    /// Each screen result belongs to the phase whose screen shows it.
    fn validate_screen_state(&self) -> Result<(), SaveError> {
        let phase = self.run_state.phase();
        let screen = &self.screen;

        if screen.caught_penalty.is_some() && phase != ContractPhase::Caught {
            return Err(invalid("a caught penalty is saved for a run not caught"));
        }
        // The victory screen also shows the final boss's own payout.
        let reward_phase = matches!(phase, ContractPhase::Hub | ContractPhase::ContractWon);
        if screen.pending_card_reward.is_some() && !reward_phase {
            return Err(invalid("a card reward is saved outside the hub"));
        }
        if let Some(reward) = &screen.pending_card_reward {
            self.validate_pending_card_reward(reward)?;
        }
        Ok(())
    }

    /// A card reward waits for the fight just completed, so its payout and
    /// Rare guarantee are that fight's, and its offer is the reward screen's
    /// size at most, with no card twice. (That the guaranteed Rare is really
    /// there needs the card database; `SaveGameNode` checks it.)
    fn validate_pending_card_reward(&self, reward: &PendingCardReward) -> Result<(), SaveError> {
        let Some((encounter_type, zone)) = self.run_state.last_completed_encounter() else {
            return Err(invalid("a card reward is saved before any fight was won"));
        };
        if !encounter_type.is_fight() {
            return Err(invalid(
                "a card reward is saved after an encounter that was not a fight",
            ));
        }
        if reward.credits != encounter_reward::credits_for(encounter_type, zone) {
            return Err(invalid(
                "a card reward's credits are not what its fight paid",
            ));
        }
        let guarantees_rare = encounter_reward::guaranteed_rarity(encounter_type).is_some();
        if reward.guaranteed_rare != guarantees_rare {
            return Err(invalid("a card reward's Rare guarantee is not its fight's"));
        }

        if reward.offered_card_ids.len() > card_reward::OFFER_SIZE {
            return Err(invalid(
                "a card reward offers more cards than the reward screen",
            ));
        }
        let distinct: BTreeSet<&String> = reward.offered_card_ids.iter().collect();
        if distinct.len() != reward.offered_card_ids.len() {
            return Err(invalid("a card reward offers the same card twice"));
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
/// The JSON goes to a temporary file beside it first, which is flushed to disk
/// before it is renamed over the old save, and the directory is then flushed
/// so the rename itself is on disk. The game closing mid-write, or the
/// machine losing power, leaves either the previous save or the new one,
/// never a half-written file.
pub fn write_save_file(path: &Path, save: &SaveGame) -> Result<(), SaveError> {
    let json = save.to_json()?;
    let temporary = temporary_path(path);
    let directory = path.parent();
    if let Some(directory) = directory {
        fs::create_dir_all(directory).map_err(io_error)?;
    }

    let mut file = fs::File::create(&temporary).map_err(io_error)?;
    file.write_all(json.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);

    fs::rename(&temporary, path).map_err(io_error)?;
    if let Some(directory) = directory {
        sync_directory(directory)?;
    }
    Ok(())
}

/// Flushes a directory's entries, such as a rename into it, to disk. Windows
/// cannot open a directory as a file and commits a rename itself, so there
/// is nothing to do there.
fn sync_directory(directory: &Path) -> Result<(), SaveError> {
    if cfg!(windows) {
        return Ok(());
    }
    fs::File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

fn io_error(error: io::Error) -> SaveError {
    SaveError::Io(error.to_string())
}

/// What could be kept of a save that would not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadableSave {
    /// Where the file was copied before anything could overwrite it.
    pub kept_at: PathBuf,
    /// The credit balance, if the file still says what it was. It is what
    /// carries between runs, so it is worth keeping even when the run cannot
    /// be continued, as with a save from another version.
    pub credits: Option<i64>,
}

/// Keeps a save that would not load: copies it beside itself with
/// `.unreadable` added, replacing any earlier copy, and reads back its credit
/// balance if the file still holds one. The next save then replaces the
/// original without losing it.
pub fn keep_unreadable_save(path: &Path) -> Result<UnreadableSave, SaveError> {
    let mut kept_at = path.as_os_str().to_owned();
    kept_at.push(".unreadable");
    let kept_at = PathBuf::from(kept_at);
    fs::copy(path, &kept_at).map_err(io_error)?;

    let credits = fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|value| value.get("meta")?.get("credits")?.as_i64());
    Ok(UnreadableSave { kept_at, credits })
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
    use crate::contract_map::EncounterType;

    /// A run that has accepted an offer, won its first fight and banked
    /// some credits and a card, the way a few minutes of play leaves it.
    fn run_played_partway() -> RunState {
        for seed in 7..200 {
            let mut run = RunState::generated(&mut StdRng::seed_from_u64(seed));
            run.accept_contract_offer(1).expect("second offer");

            let Some(fight) = run
                .selectable_encounters()
                .into_iter()
                .find(|node| node.encounter_type() == EncounterType::Combat)
                .map(|node| node.id())
            else {
                continue;
            };
            run.select_encounter(fight).expect("first fight");
            run.complete_encounter().expect("complete it");
            run.record_credit_change(25);
            run.record_card_added();
            return run;
        }
        panic!("no seed offered a fight straight away");
    }

    /// A run that accepted its third offer and beat that contract's final
    /// boss, so it is on the victory screen with the reward recorded.
    fn run_won() -> RunState {
        let mut run = RunState::generated(&mut StdRng::seed_from_u64(9));
        run.accept_contract_offer(2).expect("third offer");

        // Bounded so a contract that cannot be won fails rather than hangs.
        for _ in 0..run.progress().total {
            let next = run.selectable_encounters()[0].id();
            run.select_encounter(next)
                .expect("the hub offers an encounter");
            run.complete_encounter().expect("the encounter is active");
            if run.phase() == ContractPhase::ContractWon {
                return run;
            }
        }
        panic!("the contract was not won");
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

    /// Steps into the first shop a fresh run on an accepted contract can
    /// reach, without opening it.
    fn run_in_a_shop() -> RunState {
        for seed in 0..200 {
            let mut run = RunState::generated(&mut StdRng::seed_from_u64(seed));
            run.accept_contract_offer(0).expect("first offer");
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

    fn shop_pool() -> Vec<(String, Rarity)> {
        vec![
            ("vpn".to_string(), Rarity::Common),
            ("proxy_chain".to_string(), Rarity::Rare),
        ]
    }

    /// What the reward screen holds after `run`'s last fight, offering
    /// `card_ids`.
    fn pending_reward_for(run: &RunState, card_ids: &[&str]) -> PendingCardReward {
        let (encounter_type, zone) = run.last_completed_encounter().expect("a fight was won");
        PendingCardReward {
            credits: encounter_reward::credits_for(encounter_type, zone),
            guaranteed_rare: encounter_reward::guaranteed_rarity(encounter_type).is_some(),
            offered_card_ids: card_ids.iter().map(|id| id.to_string()).collect(),
        }
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

    /// Continuing on the victory screen shows what the contract paid, which
    /// run state recorded when it was won. The credits themselves are in the
    /// saved balance, so nothing is paid again.
    #[test]
    fn a_won_contract_is_saved_with_the_reward_it_recorded() {
        let won = run_won();
        let reward = won.accepted_offer().unwrap().credit_reward;
        let save = save_of(won);

        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(restored, save);
        assert_eq!(restored.run.run_state.phase(), ContractPhase::ContractWon);
        assert_eq!(restored.run.run_state.contract_reward(), Some(reward));
    }

    /// Accepting an offer does not commit the run to it until an encounter
    /// is entered, and that has to survive quitting and continuing.
    #[test]
    fn a_contract_not_yet_started_can_still_be_changed_after_continuing() {
        let mut run = RunState::generated(&mut StdRng::seed_from_u64(13));
        run.accept_contract_offer(2).expect("third offer");
        run.accept_contract_offer(0).expect("changed to the first");

        let mut restored = SaveGame::from_json(&save_of(run).to_json().unwrap())
            .unwrap()
            .run
            .run_state;

        assert_eq!(restored.accepted_offer_index(), Some(0));
        assert!(!restored.needs_contract_choice(), "it goes to the map");
        assert!(restored.can_accept_contract_offer(), "with Change contract");
        restored
            .accept_contract_offer(1)
            .expect("the contract can be changed again");
    }

    #[test]
    fn a_contract_under_way_still_cannot_be_changed_after_continuing() {
        let mut restored = SaveGame::from_json(&save_of(run_played_partway()).to_json().unwrap())
            .unwrap()
            .run
            .run_state;

        assert!(!restored.can_accept_contract_offer());
        assert!(restored.accept_contract_offer(0).is_err());
    }

    #[test]
    fn a_won_save_claiming_a_different_reward_is_refused() {
        let json = save_of(run_won()).to_json().unwrap();
        let mut value: Value = serde_json::from_str(&json).unwrap();
        let reward = value["run"]["run_state"]["contract_reward"]
            .as_i64()
            .unwrap();
        value["run"]["run_state"]["contract_reward"] = json!(reward * 2);

        assert!(matches!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::InvalidRun(_))
        ));
    }

    #[test]
    fn a_won_save_without_its_reward_is_refused() {
        let json = save_of(run_won()).to_json().unwrap();
        let mut value: Value = serde_json::from_str(&json).unwrap();
        value["run"]["run_state"]["contract_reward"] = Value::Null;

        assert!(matches!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::InvalidRun(_))
        ));
    }

    /// Version 1 kept a claimed flag and a separate payout instead of the
    /// recorded reward. Such a file is refused before its run is read.
    #[test]
    fn a_version_1_save_is_refused_as_incompatible() {
        let json = save_of(run_won()).to_json().unwrap();
        let mut value: Value = serde_json::from_str(&json).unwrap();
        value["version"] = json!(1);
        let run_state = value["run"]["run_state"].as_object_mut().unwrap();
        run_state.remove("contract_reward");
        run_state.insert("contract_reward_claimed".to_string(), json!(true));
        value["run"]["screen"]["contract_payout"] =
            json!({ "credits": 500, "balance_before": 100, "balance_after": 600 });

        assert_eq!(
            SaveGame::from_json(&value.to_string()),
            Err(SaveError::IncompatibleVersion {
                found: 1,
                expected: SAVE_VERSION,
            })
        );
    }

    /// Story #116: a continued run still knows which events it has shown, so
    /// it keeps avoiding them.
    #[test]
    fn a_continued_run_keeps_the_events_it_has_seen() {
        let mut run = run_played_partway();
        let mut rng = StdRng::seed_from_u64(5);
        let seen: Vec<String> = (0..3).map(|_| run.roll_event(&mut rng).id).collect();
        let save = save_of(run);

        let restored = SaveGame::from_json(&save.to_json().unwrap()).expect("it reads back");
        assert_eq!(restored, save);

        for _ in 0..20 {
            let next = restored.run.run_state.clone().roll_event(&mut rng);
            assert!(!seen.contains(&next.id), "'{}' was shown again", next.id);
        }
    }

    /// A save made before events were tracked has no seen events, and still
    /// loads as a run that has seen none.
    #[test]
    fn a_save_from_before_seen_events_still_loads() {
        let save = save_of(run_played_partway());
        let mut value: Value = serde_json::from_str(&save.to_json().unwrap()).unwrap();
        value["run"]["run_state"]
            .as_object_mut()
            .unwrap()
            .remove("seen_events")
            .expect("the field is saved");

        assert_eq!(SaveGame::from_json(&value.to_string()), Ok(save));
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
        let tamperings: [Tampering; 14] = [
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
            ("noise above the meter's cap", |value| {
                value["run"]["noise"] = json!(500);
            }),
            // A valid graph, but not the accepted offer's: turning its fights
            // into shops would make the contract free to walk.
            ("a map that is not the accepted contract's", |value| {
                let nodes = &mut value["run"]["run_state"]["contract_map"]["nodes"];
                let fight = nodes
                    .as_object_mut()
                    .expect("nodes are keyed by id")
                    .values_mut()
                    .filter(|node| node["encounter_type"] == json!("combat"))
                    .last()
                    .expect("the map has a fight");
                fight["encounter_type"] = json!("shop");
            }),
            ("a contract under way that was never accepted", |value| {
                value["run"]["run_state"]["accepted_offer_index"] = Value::Null;
            }),
            // Sizing a map this long overflows, so it must never reach the
            // hub, which lists every offer.
            ("an offer with endless zones", |value| {
                value["run"]["run_state"]["contract_offers"][0]["shape"]["zone_length"] =
                    json!(u64::MAX);
            }),
            ("an offer with more zones than there are bosses", |value| {
                value["run"]["run_state"]["contract_offers"][0]["shape"]["zone_count"] = json!(99);
            }),
            ("an offer paying more than its shape", |value| {
                value["run"]["run_state"]["contract_offers"][0]["credit_reward"] = json!(1_000_000);
            }),
            ("a contract reward on a contract not won", |value| {
                value["run"]["run_state"]["contract_reward"] = json!(500);
            }),
            // Change contract would then replace a map with progress on it.
            ("progress on a contract marked as not started", |value| {
                value["run"]["run_state"]["contract_started"] = json!(false);
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
        let mut shop = run_in_a_shop();
        shop.open_shop_visit(&shop_pool(), &mut StdRng::seed_from_u64(3))
            .expect("in a shop");
        let mut credits = 500;
        let mut deck = crate::run_deck::RunDeck::default();
        shop.buy_from_shop(0, &mut credits, &mut deck)
            .expect("buy slot 0");
        let visit = shop.shop_visit().cloned();

        let save = SaveGame::new(MetaProgress { credits }, saved_run(shop))
            .expect("a shop is a safe point");
        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(restored.run.run_state.shop_visit().cloned(), visit);
        assert!(restored.run.run_state.shop_visit().unwrap().slots()[0].is_sold());
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

    /// Continuing on a card reward must offer the cards that were on the
    /// screen, or quitting and continuing would be a free reroll.
    #[test]
    fn a_pending_card_reward_keeps_the_cards_it_offered() {
        let played = run_played_partway();
        let reward =
            pending_reward_for(&played, &["wannacry", "social_engineering", "triple_worm"]);
        let mut run = saved_run(played);
        run.screen.pending_card_reward = Some(reward);
        let save = SaveGame::new(MetaProgress::default(), run).expect("the hub is a safe point");

        let restored = SaveGame::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(
            restored.run.screen.pending_card_reward,
            save.run.screen.pending_card_reward
        );
    }

    #[test]
    fn a_card_reward_offering_a_card_twice_is_refused() {
        let played = run_played_partway();
        let reward = pending_reward_for(&played, &["vpn", "vpn"]);
        let mut run = saved_run(played);
        run.screen.pending_card_reward = Some(reward);

        assert!(matches!(
            SaveGame::new(MetaProgress::default(), run),
            Err(SaveError::InvalidRun(_))
        ));
    }

    /// A card reward's payout, guarantee and size are its fight's, so an
    /// edited reward cannot pay more or offer more on Continue.
    #[test]
    fn an_edited_card_reward_is_refused() {
        let played = run_played_partway();
        let honest = pending_reward_for(&played, &["vpn"]);
        let edits: [(&str, PendingCardReward); 3] = [
            (
                "more credits than the fight paid",
                PendingCardReward {
                    credits: honest.credits + 1,
                    ..honest.clone()
                },
            ),
            (
                "a Rare guarantee the fight did not earn",
                PendingCardReward {
                    guaranteed_rare: !honest.guaranteed_rare,
                    ..honest.clone()
                },
            ),
            (
                "more cards than the reward screen offers",
                pending_reward_for(&played, &["vpn", "strike", "rootkit", "phish"]),
            ),
        ];

        for (description, reward) in edits {
            let mut run = saved_run(played.clone());
            run.screen.pending_card_reward = Some(reward);
            assert!(
                matches!(
                    SaveGame::new(MetaProgress::default(), run),
                    Err(SaveError::InvalidRun(_))
                ),
                "a reward with {description} was not refused"
            );
        }
    }

    /// The caps are constants rather than read from the meter nodes, so the
    /// check holds even when the save is read before those nodes exist.
    #[test]
    fn noise_or_knowledge_above_its_cap_is_refused() {
        let mut too_noisy = saved_run(run_played_partway());
        too_noisy.noise = MAX_NOISE + 1;
        let mut too_knowing = saved_run(run_played_partway());
        too_knowing.knowledge = MAX_KNOWLEDGE + 1;

        for run in [too_noisy, too_knowing] {
            assert!(matches!(
                SaveGame::new(MetaProgress::default(), run),
                Err(SaveError::InvalidRun(_))
            ));
        }
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

    /// A save that will not load is copied aside before the next save could
    /// replace it, and its balance is kept when the file still states it.
    #[test]
    fn an_unreadable_save_is_kept_with_its_credit_balance() {
        let dir = scratch_dir("unreadable");
        let path = dir.join("save.json");
        let mut old_version = save_json_value();
        old_version["version"] = json!(SAVE_VERSION + 1);
        old_version["meta"]["credits"] = json!(345);
        fs::write(&path, old_version.to_string()).unwrap();
        assert!(read_save_file(&path).is_err());

        let kept = keep_unreadable_save(&path).expect("the file can be copied");

        assert_eq!(kept.credits, Some(345));
        assert_eq!(
            fs::read_to_string(&kept.kept_at).unwrap(),
            old_version.to_string()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_corrupt_save_is_kept_even_without_a_balance_to_recover() {
        let dir = scratch_dir("unreadable_corrupt");
        let path = dir.join("save.json");
        fs::write(&path, "{\"version\": 1, \"meta\": ").unwrap();

        let kept = keep_unreadable_save(&path).expect("the file can be copied");

        assert_eq!(kept.credits, None);
        assert!(kept.kept_at.is_file());
        fs::remove_dir_all(dir).unwrap();
    }
}

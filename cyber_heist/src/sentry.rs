//! The sentry: the security construct that takes a turn after the player.
//!
//! Covers the "security system takes its own turn" acceptance tests from
//! Trello card 30:
//! 1. The sentry performs its queued action when its phase begins, and the
//!    result is in the game state before the next player turn starts.
//! 2. If that action pushes the noise meter to its cap, the meter stops at the
//!    cap and the run ends in detection failure.
//!
//! Every sentry has a name, so a later encounter can put a different construct
//! in the way without any of these rules changing.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `sentry_node.rs` adapts this for scenes.

/// Something an action does to the player beyond adding noise.
///
/// Ordinary security only ever makes noise. An ability is what marks an
/// encounter out as more than a louder version of the last one, which is why
/// only bosses carry them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentryAbility {
    /// Noise and nothing else. Every standard sentry action.
    None,
    /// Takes energy off the player's next turn. The turn they had planned is
    /// spent before they get to it, rather than merely being noisier.
    DrainEnergy(i32),
}

impl SentryAbility {
    /// Energy this ability takes, or zero when it takes none.
    pub fn energy_drain(self) -> i32 {
        match self {
            Self::None => 0,
            Self::DrainEnergy(amount) => amount.max(0),
        }
    }

    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    /// How the ability reads beside the action on the intent line, or an
    /// empty string when there is nothing extra to announce.
    pub fn describe(self) -> String {
        match self {
            Self::None => String::new(),
            Self::DrainEnergy(amount) => format!("-{} energy", amount.max(0)),
        }
    }
}

/// One action a sentry can take on its turn.
///
/// The player gets to see the queued one while it is still their turn, which
/// is the whole point: you plan against a threat you can read, rather than
/// being surprised by a dice roll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentryAction {
    /// Player-facing name, shown while the action is queued and again once it
    /// has been performed.
    pub name: String,
    /// Noise this action adds to the meter when it runs.
    pub noise: i32,
    /// What it does on top of the noise. `None` for standard security.
    pub ability: SentryAbility,
}

impl SentryAction {
    pub fn new(name: &str, noise: i32) -> Self {
        Self {
            name: name.to_string(),
            noise,
            ability: SentryAbility::None,
        }
    }

    pub fn with_ability(name: &str, noise: i32, ability: SentryAbility) -> Self {
        Self {
            name: name.to_string(),
            noise,
            ability,
        }
    }
}

/// A named security construct and the actions it works through.
/// Noise is owned by the shared meter, not by an encounter.
///
/// Actions come from an authored list that the sentry cycles, so a turn plays
/// out the same way every time and is easy to test. Picking actions in
/// response to what the player is doing is a later card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentry {
    name: String,
    script: Vec<SentryAction>,
    next_index: usize,
    /// How well this construct resists the player pulling detection back
    /// down, as a percentage. 0 means a recovery card lands in full.
    detection_resistance: i32,
}

/// The actions every standard construct works through, before the zone's
/// difficulty is applied to them. Zone 1 fields these as authored, which is
/// what WARDEN-7 has always done.
const STANDARD_SCRIPT: [(&str, i32); 3] = [
    ("Packet Sniff", 1),
    ("Trace Sweep", 2),
    ("Lockdown Probe", 3),
];

/// Bosses by zone. A contract longer than this list falls back to a numbered
/// name, so adding zones cannot leave a boss nameless.
const BOSS_NAMES: [&str; 3] = ["ICEBREAKER", "BLACK MONOLITH", "THE ARCHITECT"];

/// Detection resistance a standard construct carries, by zone. Deeper zones
/// push back harder on recovery, and the zone's boss then goes further still.
const STANDARD_RESISTANCE_PER_ZONE: i32 = 10;

/// What a boss adds to the resistance of the standard security around it.
const BOSS_RESISTANCE_BONUS: i32 = 30;

/// What a boss adds to the loudest standard action in its zone. Every boss
/// action clears that bar, so a boss turn is always worse than a normal one.
const BOSS_NOISE_BONUS: i32 = 2;

impl Sentry {
    /// Builds a sentry with `script` as its action queue.
    ///
    /// An empty script is allowed and means the sentry has nothing queued, so
    /// its turn does nothing at all.
    pub fn new(name: &str, script: Vec<SentryAction>) -> Self {
        Self {
            name: name.to_string(),
            script,
            next_index: 0,
            detection_resistance: 0,
        }
    }

    /// The construct guarding an ordinary encounter in `zone`.
    ///
    /// Zone 0 is WARDEN-7 exactly as authored; deeper zones field the same
    /// script scaled up, so a boss can be measured against the security the
    /// player has been dealing with in that zone rather than a fixed bar.
    pub fn standard_for_zone(zone: usize) -> Self {
        let step = Self::zone_step(zone);
        let script = STANDARD_SCRIPT
            .iter()
            .map(|(name, noise)| SentryAction::new(name, noise * step))
            .collect();

        let mut sentry = Self::new(&format!("WARDEN-{}", 7 + 2 * zone), script);
        sentry.detection_resistance = Self::standard_resistance(zone);
        sentry
    }

    /// The boss closing `zone`.
    ///
    /// Louder than every standard action in the same zone, harder to recover
    /// against, and carrying a lockdown that no standard construct has.
    pub fn boss_for_zone(zone: usize) -> Self {
        let floor = Self::loudest_standard_noise(zone) + BOSS_NOISE_BONUS;
        let script = vec![
            SentryAction::new("Trace Lock", floor),
            SentryAction::with_ability(
                "Grid Lockdown",
                floor + 2,
                SentryAbility::DrainEnergy(1 + zone as i32),
            ),
            SentryAction::new("Full Spectrum Sweep", floor + 1),
        ];

        let name = BOSS_NAMES
            .get(zone)
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("OVERSEER-{}", zone + 1));

        let mut sentry = Self::new(&name, script);
        sentry.detection_resistance = Self::standard_resistance(zone) + BOSS_RESISTANCE_BONUS;
        sentry
    }

    /// The construct for an encounter, whichever kind it is.
    pub fn for_encounter(zone: usize, is_boss: bool) -> Self {
        if is_boss {
            Self::boss_for_zone(zone)
        } else {
            Self::standard_for_zone(zone)
        }
    }

    fn zone_step(zone: usize) -> i32 {
        i32::try_from(zone).unwrap_or(i32::MAX - 1) + 1
    }

    fn standard_resistance(zone: usize) -> i32 {
        Self::zone_step(zone)
            .saturating_sub(1)
            .saturating_mul(STANDARD_RESISTANCE_PER_ZONE)
    }

    /// The loudest single action standard security takes in `zone`, which is
    /// the bar a boss has to clear.
    pub fn loudest_standard_noise(zone: usize) -> i32 {
        let step = Self::zone_step(zone);
        STANDARD_SCRIPT
            .iter()
            .map(|(_, noise)| noise * step)
            .max()
            .unwrap_or(0)
    }

    /// How well this construct resists the player lowering detection, as a
    /// percentage applied to recovery only.
    pub fn detection_resistance(&self) -> i32 {
        self.detection_resistance
    }

    /// Whether this construct has any action that does more than make noise.
    pub fn has_ability(&self) -> bool {
        self.script.iter().any(|action| !action.ability.is_none())
    }

    /// The construct guarding a first-zone encounter, with three cycling
    /// actions. The shared meter determines when those actions cause detection.
    pub fn warden_7() -> Self {
        Self::standard_for_zone(0)
    }

    /// Renames the sentry, for an encounter that fields a different construct
    /// with the same behaviour.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The action the sentry will take when its turn begins, or `None` if
    /// there is nothing queued.
    pub fn queued_action(&self) -> Option<&SentryAction> {
        self.script.get(self.next_index)
    }

    /// Returns the announced action and queues the next one. The caller applies
    /// its authored noise to the shared meter and reports the actual clamped
    /// delta separately. Returns `None` when nothing was queued.
    pub fn perform_queued_action(&mut self) -> Option<SentryAction> {
        let action = self.script.get(self.next_index)?.clone();
        self.next_index = (self.next_index + 1) % self.script.len();
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_sentry_has_its_first_action_queued() {
        let sentry = Sentry::warden_7();
        assert_eq!(sentry.name(), "WARDEN-7");
        assert_eq!(sentry.queued_action().unwrap().name, "Packet Sniff");
    }

    #[test]
    fn performing_the_queued_action_reports_it_and_advances_the_queue() {
        let mut sentry = Sentry::new(
            "WARDEN-7",
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
            ],
        );

        let action = sentry.perform_queued_action().unwrap();
        assert_eq!(action.name, "Packet Sniff");
        assert_eq!(action.noise, 1);
        assert_eq!(sentry.queued_action().unwrap().name, "Trace Sweep");

        sentry.perform_queued_action().unwrap();
        assert_eq!(
            sentry.queued_action().unwrap().name,
            "Packet Sniff",
            "queue wraps back to the start"
        );
    }

    #[test]
    fn each_announced_action_runs_once_and_the_queue_wraps_without_stale_intent() {
        use crate::noise_meter::NoiseLevel;

        let script = vec![
            SentryAction::new("Packet Sniff", 1),
            SentryAction::new("Trace Sweep", 2),
            SentryAction::new("Lockdown Probe", 3),
        ];
        let mut sentry = Sentry::new("WARDEN-7", script.clone());
        let mut meter = NoiseLevel::new(100);

        for index in 0..7 {
            let expected = &script[index % script.len()];
            let before = sentry.clone();
            assert_eq!(sentry.queued_action(), Some(expected));
            assert_eq!(sentry.queued_action(), Some(expected));
            assert_eq!(sentry, before, "reading intent cannot advance the queue");

            let action = sentry.perform_queued_action().unwrap();
            let noise_before = meter.noise();
            assert_eq!(&action, expected);
            assert_eq!(meter.add(action.noise), expected.noise);
            assert_eq!(meter.noise(), noise_before + expected.noise);
            assert!(!meter.is_at_cap());
            assert_eq!(
                sentry.queued_action(),
                Some(&script[(index + 1) % script.len()])
            );
        }
    }

    #[test]
    fn authored_intent_and_actual_clamped_change_are_distinct_at_cap() {
        use crate::noise_meter::NoiseLevel;

        let mut sentry = Sentry::new("WARDEN-7", vec![SentryAction::new("Lockdown Probe", 3)]);
        let mut meter = NoiseLevel::new(100);
        meter.add(99);
        let announced = sentry.queued_action().unwrap().clone();
        let action = sentry.perform_queued_action().unwrap();

        assert_eq!(action, announced);
        assert_eq!(action.noise, 3);
        assert_eq!(meter.add(action.noise), 1);
        assert_eq!(meter.noise(), 100);
        assert!(meter.is_at_cap());
        assert_eq!(sentry.queued_action(), Some(&announced));
    }

    #[test]
    fn a_sentry_with_nothing_queued_reports_no_action() {
        let mut sentry = Sentry::new("WARDEN-7", Vec::new());
        assert_eq!(sentry.queued_action(), None);
        assert_eq!(sentry.perform_queued_action(), None);
    }

    /// The zone-1 construct is WARDEN-7 exactly as it was authored. The
    /// smoke test drives a real encounter against these numbers, so a change
    /// here is a change to what CI is checking.
    #[test]
    fn the_first_zone_still_fields_warden_7_unchanged() {
        let standard = Sentry::standard_for_zone(0);

        assert_eq!(standard.name(), "WARDEN-7");
        assert_eq!(standard.detection_resistance(), 0);
        assert_eq!(
            standard.script,
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::new("Trace Sweep", 2),
                SentryAction::new("Lockdown Probe", 3),
            ]
        );
        assert_eq!(standard, Sentry::warden_7());
    }

    /// Card scenario 1: the boss generates more noise than the standard
    /// security in the same zone. Every one of its actions clears the bar,
    /// not just its loudest.
    #[test]
    fn every_boss_action_is_louder_than_any_standard_action_in_its_zone() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let boss = Sentry::boss_for_zone(zone);

            let loudest_standard = standard
                .script
                .iter()
                .map(|action| action.noise)
                .max()
                .expect("standard security has a script");

            for action in &boss.script {
                assert!(
                    action.noise > loudest_standard,
                    "zone {zone}: boss action {} makes {} noise, \
                     which does not beat the standard {loudest_standard}",
                    action.name,
                    action.noise
                );
            }
        }
    }

    /// Card scenario 1: the boss resists detection better than the standard
    /// security in the same zone.
    #[test]
    fn a_boss_resists_detection_harder_than_its_zone_does() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let boss = Sentry::boss_for_zone(zone);

            assert!(
                boss.detection_resistance() > standard.detection_resistance(),
                "zone {zone}: boss resists {} against a standard {}",
                boss.detection_resistance(),
                standard.detection_resistance()
            );
            assert!(
                (0..100).contains(&boss.detection_resistance()),
                "zone {zone}: resistance has to leave recovery worth something"
            );
        }
    }

    /// Card scenario 1: at least one ability the player will not have met in
    /// a regular encounter.
    #[test]
    fn only_a_boss_carries_an_ability() {
        for zone in 0..4 {
            assert!(
                !Sentry::standard_for_zone(zone).has_ability(),
                "zone {zone}: standard security only makes noise"
            );

            let boss = Sentry::boss_for_zone(zone);
            assert!(boss.has_ability(), "zone {zone}: the boss needs an ability");

            let abilities: Vec<SentryAbility> = boss
                .script
                .iter()
                .map(|action| action.ability)
                .filter(|ability| !ability.is_none())
                .collect();
            assert_eq!(
                abilities,
                vec![SentryAbility::DrainEnergy(1 + zone as i32)],
                "zone {zone}: the lockdown takes energy, and deeper zones take more"
            );
        }
    }

    /// An ability the player has already seen would not be a boss ability, so
    /// nothing a boss does may share a name with the standard script either.
    #[test]
    fn a_boss_script_shares_no_action_with_the_standard_one() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let standard_names: Vec<&str> = standard
                .script
                .iter()
                .map(|action| action.name.as_str())
                .collect();

            for action in &Sentry::boss_for_zone(zone).script {
                assert!(
                    !standard_names.contains(&action.name.as_str()),
                    "zone {zone}: {} is not a boss-only action",
                    action.name
                );
            }
        }
    }

    #[test]
    fn a_drained_turn_is_announced_with_the_intent() {
        let boss = Sentry::boss_for_zone(0);
        let lockdown = boss
            .script
            .iter()
            .find(|action| !action.ability.is_none())
            .expect("the boss has a lockdown");

        assert_eq!(lockdown.ability.energy_drain(), 1);
        assert_eq!(lockdown.ability.describe(), "-1 energy");
        assert_eq!(SentryAbility::None.energy_drain(), 0);
        assert_eq!(SentryAbility::None.describe(), "");
        assert_eq!(
            SentryAbility::DrainEnergy(-3).energy_drain(),
            0,
            "a nonsense drain takes nothing rather than handing energy out"
        );
    }

    #[test]
    fn for_encounter_fields_the_boss_only_when_the_encounter_is_one() {
        for zone in 0..3 {
            assert_eq!(
                Sentry::for_encounter(zone, false),
                Sentry::standard_for_zone(zone)
            );
            assert_eq!(
                Sentry::for_encounter(zone, true),
                Sentry::boss_for_zone(zone)
            );
        }
    }

    /// Deeper zones field harder security, so a boss is measured against the
    /// encounters around it rather than against a fixed bar.
    #[test]
    fn security_gets_harder_the_deeper_the_zone() {
        for zone in 1..4 {
            assert!(
                Sentry::loudest_standard_noise(zone) > Sentry::loudest_standard_noise(zone - 1),
                "zone {zone} should be noisier than the one before it"
            );
            assert!(
                Sentry::boss_for_zone(zone).detection_resistance()
                    > Sentry::boss_for_zone(zone - 1).detection_resistance()
            );
            assert_ne!(
                Sentry::boss_for_zone(zone).name(),
                Sentry::boss_for_zone(zone - 1).name(),
                "each zone's boss is its own construct"
            );
        }
    }

    /// A contract longer than the authored boss list still names its bosses.
    #[test]
    fn a_zone_past_the_authored_names_still_has_a_named_boss() {
        let boss = Sentry::boss_for_zone(BOSS_NAMES.len());
        assert!(!boss.name().is_empty());
        assert!(boss.has_ability());
    }

    #[test]
    fn a_renamed_sentry_reports_the_new_name() {
        let mut sentry = Sentry::warden_7();
        sentry.set_name("BLACKGATE");
        assert_eq!(sentry.name(), "BLACKGATE");
    }
}

#[cfg(test)]
mod stress {
    use super::*;
    use crate::noise_meter::NoiseLevel;

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn authored_actions_of_any_loudness_survive_repeated_turns() {
        let mut seed = 0xC0FF_EE01_u32;
        for cap in [0, 1, 100, 1_000] {
            let mut script = vec![
                SentryAction::new("Full Sweep", i32::MAX),
                SentryAction::new("Total Blackout", i32::MIN),
            ];
            for index in 0..7 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                script.push(SentryAction::new(&format!("Action {index}"), seed as i32));
            }
            let mut sentry = Sentry::new("WARDEN-7", script);
            let mut meter = NoiseLevel::new(cap);
            for _ in 0..50_000 {
                let announced = sentry.queued_action().cloned().unwrap();
                let action = sentry.perform_queued_action().unwrap();
                assert_eq!(action, announced);
                let before = meter.noise();
                let delta = meter.add(action.noise);
                assert_eq!(meter.noise() - before, delta);
                assert!((0..=cap).contains(&meter.noise()));
                assert_eq!(meter.is_at_cap(), meter.noise() == cap);
            }
        }
    }
}

//! The sentry: the security construct that takes a turn after the player.
//!
//! Covers the "security system takes its own turn" acceptance tests from
//! Trello card 30:
//! 1. The sentry performs its queued action when its phase begins, and the
//!    result is in the game state before the next player turn starts.
//! 2. If that action pushes the noise meter to its cap, the meter stops at the
//!    cap and the run ends in detection failure.
//!
//! Also covers the sentry having its own health, so damage cards like Strike
//! can bring it down and end the encounter that way:
//! 3. Damage applied to the sentry is clamped between 0 and its max health,
//!    and the actual amount lost (not the authored amount) is reported back.
//! 4. A sentry at 0 health is defeated and takes no further turns.
//!
//! Every sentry has a name, so a later encounter can put a different construct
//! in the way without any of these rules changing.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `sentry_node.rs` adapts this for scenes.

/// Something an action does to the player beyond adding noise.
///
/// An ability is what marks an encounter out as more than a louder version of
/// the last one. Standard security has a couple, so no fight is noise alone;
/// elites add their own on top, and each boss has a signature no one else
/// uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentryAbility {
    /// Noise and nothing else.
    None,
    /// Takes energy off the player's next turn. The turn they had planned is
    /// spent before they get to it, rather than merely being noisier.
    DrainEnergy(i32),
    /// Restores the construct's own integrity. An elite comes back from the
    /// damage the player has done, so a fight dragged out costs them.
    RestoreIntegrity(i32),
    /// Wipes every point of corruption off the construct. ICEBREAKER's
    /// signature: a corruption deck has to cash its stacks in before it lands.
    Purge,
    /// Raises a firewall that absorbs this much plain damage during the
    /// player's next turn. Standard security's Firewall Up, and a bigger one
    /// for BLACK MONOLITH's signature: penetrating damage and corruption go
    /// straight through it.
    Firewall(i32),
    /// Plants this many Injected Malware junk cards in the player's discard pile.
    /// They only last the encounter: the combat deck is rebuilt from the
    /// run's deck each time, and Malware is never added to that.
    InjectMalware(i32),
}

/// The card id Inject Malware plants, authored in cursed_cards.ron. Not
/// "malware": that is the player's own attack card in cards.ron.
pub const MALWARE_CARD_ID: &str = "injected_malware";

impl SentryAbility {
    /// Energy this ability takes, or zero when it takes none.
    pub fn energy_drain(self) -> i32 {
        match self {
            Self::DrainEnergy(amount) => amount.max(0),
            Self::None
            | Self::RestoreIntegrity(_)
            | Self::Purge
            | Self::Firewall(_)
            | Self::InjectMalware(_) => 0,
        }
    }

    /// Integrity this ability restores to the construct, or zero when it
    /// restores none. A negative amount restores nothing rather than dealing
    /// damage.
    pub fn integrity_restored(self) -> i32 {
        match self {
            Self::RestoreIntegrity(amount) => amount.max(0),
            Self::None
            | Self::DrainEnergy(_)
            | Self::Purge
            | Self::Firewall(_)
            | Self::InjectMalware(_) => 0,
        }
    }

    /// Whether this ability wipes the construct's corruption.
    pub fn purges_corruption(self) -> bool {
        matches!(self, Self::Purge)
    }

    /// Damage the firewall this ability raises will absorb, or zero when it
    /// raises none. A negative amount raises nothing.
    pub fn firewall_raised(self) -> i32 {
        match self {
            Self::Firewall(amount) => amount.max(0),
            Self::None
            | Self::DrainEnergy(_)
            | Self::RestoreIntegrity(_)
            | Self::Purge
            | Self::InjectMalware(_) => 0,
        }
    }

    /// Malware cards this ability plants, or zero when it plants none. A
    /// negative count plants nothing rather than taking cards away.
    pub fn malware_planted(self) -> i32 {
        match self {
            Self::InjectMalware(count) => count.max(0),
            Self::None
            | Self::DrainEnergy(_)
            | Self::RestoreIntegrity(_)
            | Self::Purge
            | Self::Firewall(_) => 0,
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
            Self::RestoreIntegrity(amount) => format!("+{} integrity", amount.max(0)),
            Self::Purge => "clears corruption".to_string(),
            Self::Firewall(amount) => format!("blocks {} damage", amount.max(0)),
            Self::InjectMalware(count) => {
                format!("adds {} Injected Malware to your discard", count.max(0))
            }
        }
    }
}

/// How hard the security guarding an encounter is, from the ordinary
/// construct on most nodes up to the boss closing a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityTier {
    Standard,
    /// Guards an elite node: tougher than standard security, with an ability
    /// of its own, and a better payout for beating it.
    Elite,
    Boss,
}

/// What a hit that a firewall can stand in front of actually did: how much the
/// firewall absorbed, and how much integrity was lost behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockedDamage {
    pub blocked: i32,
    pub dealt: i32,
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

    /// What the intent icon shows for this action (story #103): the kind of
    /// move, named for the screen to pick its icon, and the one number that
    /// matters most for it. An action that only makes noise shows its noise;
    /// one with an ability shows what the ability does, since the noise is
    /// already on the intent line. A purge has no amount, so it shows 0.
    pub fn intent(&self) -> (&'static str, i32) {
        match self.ability {
            SentryAbility::None => ("noise", self.noise.max(0)),
            SentryAbility::DrainEnergy(amount) => ("drain_energy", amount.max(0)),
            SentryAbility::RestoreIntegrity(amount) => ("restore_integrity", amount.max(0)),
            SentryAbility::Purge => ("purge", 0),
            SentryAbility::Firewall(amount) => ("firewall", amount.max(0)),
            SentryAbility::InjectMalware(count) => ("inject_malware", count.max(0)),
        }
    }
}

/// A named security construct and the actions it works through.
/// Noise is owned by the shared meter, not by an encounter; health belongs to
/// the sentry itself, since it's specific to this construct rather than
/// shared across the encounter the way noise is.
///
/// Actions come from an authored list that the sentry cycles, so a turn plays
/// out the same way every time and is easy to test. Standard and elite
/// security also carry an alert set, one louder action for each slot, which
/// they use instead while noise is at or above `ALERT_NOISE`. The player sees
/// it as an escalated intent, so going loud has a cost beyond the meter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentry {
    name: String,
    script: Vec<SentryAction>,
    next_index: usize,
    /// How well this construct resists the player pulling detection back
    /// down, as a percentage. 0 means a recovery card lands in full.
    detection_resistance: i32,
    health: i32,
    max_health: i32,
    corruption: i32,
    corruption_boost: i32,
    /// Plain damage still to be absorbed before integrity, raised by a
    /// Firewall and dropped when the construct's next turn starts.
    firewall: i32,
    /// 1, or 2 once the construct has escalated.
    phase: u32,
    /// The action set it switches to below half integrity. Empty for every
    /// construct but THE ARCHITECT, which is what "has no second phase" means.
    second_phase: Vec<SentryAction>,
    /// What it does instead of each action in `script` while noise is at or
    /// above `ALERT_NOISE`, slot for slot. Empty for a construct that does
    /// not react to noise, which is every boss.
    alert_script: Vec<SentryAction>,
}

/// Noise at which standard and elite security switch to their alert set. The
/// meter tops out at 100, so this is the last stretch before detection.
pub const ALERT_NOISE: i32 = 60;

/// The actions every standard construct works through, before the zone's
/// difficulty is applied to their noise. Zone 1 fields these as authored, as
/// WARDEN-7. Not every one is noise alone: Firewall Up shields the construct
/// and Inject Malware clogs the player's deck, so even an ordinary node asks
/// for more than outlasting the meter.
const STANDARD_SCRIPT: [(&str, i32, SentryAbility); 5] = [
    ("Packet Sniff", 1, SentryAbility::None),
    (
        "Firewall Up",
        1,
        SentryAbility::Firewall(STANDARD_FIREWALL_BLOCK),
    ),
    ("Trace Sweep", 2, SentryAbility::None),
    ("Inject Malware", 1, SentryAbility::InjectMalware(1)),
    ("Lockdown Probe", 3, SentryAbility::None),
];

/// Plain damage standard security's Firewall Up absorbs: a Strike and a bit,
/// short of the boss's.
const STANDARD_FIREWALL_BLOCK: i32 = 8;

/// What standard security does instead of each `STANDARD_SCRIPT` action once
/// noise reaches `ALERT_NOISE`, in the same order. Every one is louder than
/// the action it replaces, and the two with abilities hit harder.
const ALERT_SCRIPT: [(&str, SentryAbility); STANDARD_SCRIPT.len()] = [
    ("Deep Packet Inspection", SentryAbility::None),
    (
        "Firewall Surge",
        SentryAbility::Firewall(ALERT_FIREWALL_BLOCK),
    ),
    ("Active Trace", SentryAbility::None),
    ("Malware Flood", SentryAbility::InjectMalware(2)),
    ("Lockdown Sweep", SentryAbility::None),
];

/// What an alert action adds to the noise of the action it replaces. Kept to
/// one, so an alert turn in a zone stays quieter than its boss's turn.
const ALERT_NOISE_BONUS: i32 = 1;

/// Plain damage Firewall Surge absorbs: more than Firewall Up, still short of
/// BLACK MONOLITH's.
const ALERT_FIREWALL_BLOCK: i32 = 10;

/// Bosses by zone. A contract longer than this list falls back to a numbered
/// name, so adding zones cannot leave a boss nameless. Contract offers read its
/// length so they never send the player deeper than the bosses written here.
pub(crate) const BOSS_NAMES: [&str; 3] = ["ICEBREAKER", "BLACK MONOLITH", "THE ARCHITECT"];

/// Detection resistance a standard construct carries, by zone. Deeper zones
/// push back harder on recovery, and the zone's boss then goes further still.
const STANDARD_RESISTANCE_PER_ZONE: i32 = 10;

/// What a boss adds to the resistance of the standard security around it.
const BOSS_RESISTANCE_BONUS: i32 = 30;

/// What a boss adds to the loudest standard action in its zone. Every boss
/// action clears that bar, so a boss turn is always worse than a normal one.
const BOSS_NOISE_BONUS: i32 = 2;

/// Integrity by tier. Standard security is 40, so an elite is a longer fight
/// than an ordinary node and the boss closing the zone is longer again.
const STANDARD_HEALTH: i32 = 40;
const ELITE_HEALTH: i32 = 60;

/// Integrity of the first zone's boss, and how much more each boss deeper in
/// the contract has than the one before it.
const BOSS_HEALTH_BASE: i32 = 80;
const BOSS_HEALTH_PER_ZONE: i32 = 20;

/// Plain damage BLACK MONOLITH's Firewall absorbs: two Strikes' worth, half
/// again what standard security's Firewall Up stands up.
const FIREWALL_BLOCK: i32 = 12;

/// THE ARCHITECT's zone. It closes the longest contract on offer, so it is
/// the final boss, and the only one with a second phase.
const ARCHITECT_ZONE: usize = 2;

/// Elites by zone, falling back to a numbered name the same way bosses do.
const ELITE_NAMES: [&str; 3] = ["BASTION", "HYDRA", "CERBERUS"];

/// What an elite adds to the resistance of the standard security around it.
/// Half the boss's bonus, so an elite sits between the two.
const ELITE_RESISTANCE_BONUS: i32 = 15;

/// What an elite adds to each standard action it runs.
const ELITE_NOISE_BONUS: i32 = 1;

/// What an elite calls each standard action, in the same order, so the
/// player can tell at a glance they are not facing ordinary security.
const ELITE_SCRIPT: [&str; STANDARD_SCRIPT.len()] = [
    "Deep Scan",
    "Hardened Firewall",
    "Hardened Trace",
    "Payload Drop",
    "Counter Probe",
];

/// What an elite calls each alert action, in the same order as `ALERT_SCRIPT`,
/// followed by its alert Reinforce.
const ELITE_ALERT_SCRIPT: [&str; STANDARD_SCRIPT.len()] = [
    "Total Scan",
    "Fortified Firewall",
    "Relentless Trace",
    "Payload Barrage",
    "Counter Lockdown",
];

/// Integrity an elite's Reinforce restores in the first zone, and how much
/// more it restores for every zone deeper.
const ELITE_REINFORCE_BASE: i32 = 8;
const ELITE_REINFORCE_PER_ZONE: i32 = 4;

impl Sentry {
    /// Builds a sentry with `script` as its action queue, at a default
    /// health of 50/50. Use `with_health` to set a different amount.
    ///
    /// An empty script is allowed and means the sentry has nothing queued, so
    /// its turn does nothing at all.
    pub fn new(name: &str, script: Vec<SentryAction>) -> Self {
        Self {
            name: name.to_string(),
            script,
            next_index: 0,
            detection_resistance: 0,
            health: 50,
            max_health: 50,
            corruption: 0,
            corruption_boost: 0,
            firewall: 0,
            phase: 1,
            second_phase: Vec::new(),
            alert_script: Vec::new(),
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
            .map(|(name, noise, ability)| SentryAction::with_ability(name, noise * step, *ability))
            .collect();

        let mut sentry =
            Self::new(&format!("WARDEN-{}", 7 + 2 * zone), script).with_health(STANDARD_HEALTH);
        sentry.detection_resistance = Self::standard_resistance(zone);
        sentry.alert_script = STANDARD_SCRIPT
            .iter()
            .zip(ALERT_SCRIPT)
            .map(|((_, noise, _), (name, ability))| {
                SentryAction::with_ability(name, noise * step + ALERT_NOISE_BONUS, ability)
            })
            .collect();
        sentry
    }

    /// The construct guarding an elite node in `zone`.
    ///
    /// The zone's standard script run a little louder, abilities and all,
    /// then a Reinforce that restores some of the integrity the player has
    /// knocked off it. More
    /// integrity than standard security to start with, and harder to recover
    /// against, but short of the boss on both counts.
    pub fn elite_for_zone(zone: usize) -> Self {
        let step = Self::zone_step(zone);
        let mut script: Vec<SentryAction> = STANDARD_SCRIPT
            .iter()
            .zip(ELITE_SCRIPT)
            .map(|((_, noise, ability), name)| {
                SentryAction::with_ability(name, noise * step + ELITE_NOISE_BONUS, *ability)
            })
            .collect();
        let reinforce = ELITE_REINFORCE_BASE
            .saturating_add(ELITE_REINFORCE_PER_ZONE.saturating_mul(step.saturating_sub(1)));
        script.push(SentryAction::with_ability(
            "Reinforce",
            ELITE_NOISE_BONUS,
            SentryAbility::RestoreIntegrity(reinforce),
        ));

        // The alert set runs the standard alert actions under the elite's
        // names, then a louder Reinforce that restores the same amount.
        let mut alert_script: Vec<SentryAction> = STANDARD_SCRIPT
            .iter()
            .zip(ALERT_SCRIPT)
            .zip(ELITE_ALERT_SCRIPT)
            .map(|(((_, noise, _), (_, ability)), name)| {
                SentryAction::with_ability(
                    name,
                    noise * step + ELITE_NOISE_BONUS + ALERT_NOISE_BONUS,
                    ability,
                )
            })
            .collect();
        alert_script.push(SentryAction::with_ability(
            "Emergency Reinforce",
            ELITE_NOISE_BONUS + ALERT_NOISE_BONUS,
            SentryAbility::RestoreIntegrity(reinforce),
        ));

        let name = ELITE_NAMES
            .get(zone)
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("SENTINEL-{}", zone + 1));

        let mut sentry = Self::new(&name, script).with_health(ELITE_HEALTH);
        sentry.detection_resistance = Self::standard_resistance(zone) + ELITE_RESISTANCE_BONUS;
        sentry.alert_script = alert_script;
        sentry
    }

    /// The boss closing `zone`.
    ///
    /// Louder than every standard action in the same zone, harder to recover
    /// against, and carrying a signature ability that no standard construct
    /// has. Each boss also has more integrity than the one closing the zone
    /// before.
    pub fn boss_for_zone(zone: usize) -> Self {
        let floor = Self::loudest_standard_noise(zone) + BOSS_NOISE_BONUS;
        let script = vec![
            SentryAction::new("Trace Lock", floor),
            Self::boss_signature(zone, floor + 2),
            SentryAction::new("Full Spectrum Sweep", floor + 1),
        ];

        let name = BOSS_NAMES
            .get(zone)
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("OVERSEER-{}", zone + 1));

        let health = BOSS_HEALTH_BASE.saturating_add(
            BOSS_HEALTH_PER_ZONE.saturating_mul(Self::zone_step(zone).saturating_sub(1)),
        );
        let mut sentry = Self::new(&name, script).with_health(health);
        sentry.detection_resistance = Self::standard_resistance(zone) + BOSS_RESISTANCE_BONUS;
        if zone == ARCHITECT_ZONE {
            sentry.second_phase = Self::architect_second_phase(zone, floor);
        }
        sentry
    }

    /// What THE ARCHITECT switches to below half integrity: every other
    /// boss's signature in turn, so the player has to answer all of them at
    /// once, and a lockdown louder than its first.
    fn architect_second_phase(zone: usize, floor: i32) -> Vec<SentryAction> {
        vec![
            SentryAction::with_ability("Rewrite Protocol", floor + 1, SentryAbility::Purge),
            SentryAction::with_ability(
                "Fortify Core",
                floor + 1,
                SentryAbility::Firewall(FIREWALL_BLOCK),
            ),
            SentryAction::with_ability(
                "Total Lockdown",
                floor + 3,
                SentryAbility::DrainEnergy(Self::zone_step(zone)),
            ),
        ]
    }

    /// The ability that sets the boss closing `zone` apart, so each boss asks
    /// something different of the player's deck. Bosses past the authored
    /// ones keep the Grid Lockdown, one more energy per zone deep.
    fn boss_signature(zone: usize, noise: i32) -> SentryAction {
        match zone {
            0 => SentryAction::with_ability("Purge", noise, SentryAbility::Purge),
            1 => SentryAction::with_ability(
                "Firewall",
                noise,
                SentryAbility::Firewall(FIREWALL_BLOCK),
            ),
            _ => SentryAction::with_ability(
                "Grid Lockdown",
                noise,
                SentryAbility::DrainEnergy(Self::zone_step(zone)),
            ),
        }
    }

    /// The construct for an encounter, whichever kind it is.
    pub fn for_encounter(zone: usize, tier: SecurityTier) -> Self {
        match tier {
            SecurityTier::Standard => Self::standard_for_zone(zone),
            SecurityTier::Elite => Self::elite_for_zone(zone),
            SecurityTier::Boss => Self::boss_for_zone(zone),
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
            .map(|(_, noise, _)| noise * step)
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

    /// Sets both current and max health to `max_health`. Chainable so
    /// built-in sentries like `warden_7()` can stay one-liners.
    pub fn with_health(mut self, max_health: i32) -> Self {
        self.health = max_health;
        self.max_health = max_health;
        self
    }

    /// Renames the sentry, for an encounter that fields a different construct
    /// with the same behaviour.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn health(&self) -> i32 {
        self.health
    }

    pub fn max_health(&self) -> i32 {
        self.max_health
    }

    pub fn corruption(&self) -> i32 {
        self.corruption
    }

    pub fn corruption_boost(&self) -> i32 {
        self.corruption_boost
    }

    /// True once health has been brought down to 0. A defeated sentry takes
    /// no further turns (enforced by the caller, e.g. `SentryNode`).
    pub fn is_defeated(&self) -> bool {
        self.health <= 0
    }

    /// Applies damage, clamped so health stays within `0..=max_health`.
    /// Returns how much health was actually lost, which may be less than
    /// `amount` if the sentry didn't have that much health left — mirrors
    /// `NoiseLevel::add`'s clamped-delta contract so callers report the real
    /// effect rather than the authored amount.
    ///
    /// A negative `amount` would heal the sentry, clamped at `max_health`;
    /// nothing currently does this, but the clamp makes it safe either way.
    ///
    /// Every source of damage comes through here, so this is also where a
    /// construct with a second phase escalates once it is below half.
    pub fn take_damage(&mut self, amount: i32) -> i32 {
        let before = self.health;
        self.health = self.health.saturating_sub(amount).clamp(0, self.max_health);
        self.escalate_if_below_half();
        before - self.health
    }

    /// Which phase the construct is fighting in: 1, or 2 once it has
    /// escalated.
    pub fn phase(&self) -> u32 {
        self.phase
    }

    /// Whether this construct switches to a new action set below half, or
    /// already has.
    #[cfg(test)]
    pub fn has_second_phase(&self) -> bool {
        self.phase == 2 || !self.second_phase.is_empty()
    }

    /// Switches to the second action set, from its first action, the first
    /// time integrity is below half. A defeated construct does not escalate:
    /// the fight is over, not entering a new phase.
    fn escalate_if_below_half(&mut self) {
        if self.phase == 1
            && !self.second_phase.is_empty()
            && !self.is_defeated()
            && self.health.saturating_mul(2) < self.max_health
        {
            self.script = std::mem::take(&mut self.second_phase);
            self.next_index = 0;
            self.phase = 2;
        }
    }

    /// Applies damage a firewall can stand in front of, such as a card's plain
    /// Damage. The firewall absorbs what it can and wears down by that much;
    /// the rest goes through `take_damage`. Penetrating damage and corruption
    /// call `take_damage` directly, so a firewall never sees them.
    pub fn take_blockable_damage(&mut self, amount: i32) -> BlockedDamage {
        let blocked = amount.clamp(0, self.firewall);
        self.firewall -= blocked;
        BlockedDamage {
            blocked,
            dealt: self.take_damage(amount - blocked),
        }
    }

    /// Damage the firewall will still absorb this turn.
    pub fn firewall(&self) -> i32 {
        self.firewall
    }

    /// Raises the firewall to `amount`, never lowering one already up, so a
    /// second Firewall refreshes it rather than stacking. Returns the firewall
    /// now standing.
    pub fn raise_firewall(&mut self, amount: i32) -> i32 {
        self.firewall = self.firewall.max(amount);
        self.firewall
    }

    /// Removes `percent` of the firewall standing, rounded up, and returns how
    /// much went. 100 or more takes all of it.
    pub fn strip_firewall(&mut self, percent: i32) -> i32 {
        let percent = i64::from(percent.clamp(0, 100));
        let removed = ((i64::from(self.firewall) * percent + 99) / 100) as i32;
        let removed = removed.clamp(0, self.firewall);
        self.firewall -= removed;
        removed
    }

    /// Takes the firewall down at the start of the construct's turn, so it
    /// only ever covers the player turn after it went up. Returns what was
    /// still standing.
    pub fn drop_firewall(&mut self) -> i32 {
        std::mem::take(&mut self.firewall)
    }

    /// Wipes all corruption off the construct and returns how much went.
    pub fn purge_corruption(&mut self) -> i32 {
        std::mem::take(&mut self.corruption)
    }

    /// Restores integrity, clamped at `max_health`. Returns how much was
    /// actually regained, which is nothing at full health and nothing for a
    /// negative amount — the same clamped-delta contract as `take_damage`.
    pub fn restore_integrity(&mut self, amount: i32) -> i32 {
        let before = self.health;
        self.health = self
            .health
            .saturating_add(amount.max(0))
            .clamp(0, self.max_health);
        self.health - before
    }

    pub fn add_corruption(&mut self, amount: i32) -> i32 {
        let actual_amount = if amount > 0 {
            amount + self.corruption_boost
        } else {
            amount
        };
        let before = self.corruption;
        self.corruption = (self.corruption + actual_amount).max(0);
        self.corruption - before
    }

    pub fn add_corruption_boost(&mut self, amount: i32) -> i32 {
        let before = self.corruption_boost;
        self.corruption_boost = (self.corruption_boost + amount).max(0);
        self.corruption_boost - before
    }

    pub fn clear_corruption_boost(&mut self) -> i32 {
        let cleared = self.corruption_boost;
        self.corruption_boost = 0;
        cleared
    }

    pub fn resolve_corruption_tick(&mut self) -> i32 {
        if self.corruption <= 0 {
            return 0;
        }
        let damage = self.corruption;
        let dealt = self.take_damage(damage);
        self.corruption = (self.corruption - 1).max(0);
        dealt
    }

    /// Whether, at `noise`, this construct acts from its alert set. Always
    /// false for one with no alert set, such as a boss.
    pub fn is_on_alert(&self, noise: i32) -> bool {
        !self.alert_script.is_empty() && noise >= ALERT_NOISE
    }

    /// The action the sentry will take when its turn begins if noise is still
    /// `noise` then, or `None` if there is nothing queued. The slot is fixed;
    /// noise only decides whether it is the standard or the alert action, so
    /// the intent follows the meter while the player is still choosing cards.
    pub fn queued_action(&self, noise: i32) -> Option<&SentryAction> {
        let actions = if self.is_on_alert(noise) {
            &self.alert_script
        } else {
            &self.script
        };
        actions.get(self.next_index)
    }

    /// Returns the action for the queued slot at `noise`, the noise as the
    /// turn begins, and queues the next slot. The caller applies its authored
    /// noise to the shared meter and reports the actual clamped delta
    /// separately. Returns `None` when nothing was queued.
    pub fn perform_queued_action(&mut self, noise: i32) -> Option<SentryAction> {
        let action = self.queued_action(noise)?.clone();
        self.next_index = (self.next_index + 1) % self.script.len();
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intent_names_its_kind_and_the_number_that_matters() {
        let cases = [
            (SentryAbility::None, ("noise", 4)),
            (SentryAbility::DrainEnergy(2), ("drain_energy", 2)),
            (SentryAbility::RestoreIntegrity(6), ("restore_integrity", 6)),
            (SentryAbility::Purge, ("purge", 0)),
            (SentryAbility::Firewall(8), ("firewall", 8)),
            (SentryAbility::InjectMalware(1), ("inject_malware", 1)),
        ];
        for (ability, expected) in cases {
            assert_eq!(
                SentryAction::with_ability("Probe", 4, ability).intent(),
                expected,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn an_intent_never_shows_a_negative_amount() {
        assert_eq!(SentryAction::new("Quiet", -3).intent(), ("noise", 0));
        assert_eq!(
            SentryAction::with_ability("Odd", 1, SentryAbility::Firewall(-5)).intent(),
            ("firewall", 0)
        );
    }

    #[test]
    fn a_new_sentry_has_its_first_action_queued() {
        let sentry = Sentry::warden_7();
        assert_eq!(sentry.name(), "WARDEN-7");
        assert_eq!(sentry.queued_action(0).unwrap().name, "Packet Sniff");
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

        let action = sentry.perform_queued_action(0).unwrap();
        assert_eq!(action.name, "Packet Sniff");
        assert_eq!(action.noise, 1);
        assert_eq!(sentry.queued_action(0).unwrap().name, "Trace Sweep");

        sentry.perform_queued_action(0).unwrap();
        assert_eq!(
            sentry.queued_action(0).unwrap().name,
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
            assert_eq!(sentry.queued_action(0), Some(expected));
            assert_eq!(sentry.queued_action(0), Some(expected));
            assert_eq!(sentry, before, "reading intent cannot advance the queue");

            let action = sentry.perform_queued_action(0).unwrap();
            let noise_before = meter.noise();
            assert_eq!(&action, expected);
            assert_eq!(meter.add(action.noise), expected.noise);
            assert_eq!(meter.noise(), noise_before + expected.noise);
            assert!(!meter.is_at_cap());
            assert_eq!(
                sentry.queued_action(0),
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
        let announced = sentry.queued_action(0).unwrap().clone();
        let action = sentry.perform_queued_action(0).unwrap();

        assert_eq!(action, announced);
        assert_eq!(action.noise, 3);
        assert_eq!(meter.add(action.noise), 1);
        assert_eq!(meter.noise(), 100);
        assert!(meter.is_at_cap());
        assert_eq!(sentry.queued_action(0), Some(&announced));
    }

    #[test]
    fn a_sentry_with_nothing_queued_reports_no_action() {
        let mut sentry = Sentry::new("WARDEN-7", Vec::new());
        assert_eq!(sentry.queued_action(0), None);
        assert_eq!(sentry.perform_queued_action(0), None);
    }

    /// The zone-1 construct is WARDEN-7 as authored. The smoke test drives a
    /// real encounter against these numbers, so a change here is a change to
    /// what CI is checking.
    #[test]
    fn the_first_zone_fields_warden_7_as_authored() {
        let standard = Sentry::standard_for_zone(0);

        assert_eq!(standard.name(), "WARDEN-7");
        assert_eq!(standard.health(), 40);
        assert_eq!(standard.max_health(), 40);
        assert_eq!(standard.detection_resistance(), 0);
        assert_eq!(
            standard.script,
            vec![
                SentryAction::new("Packet Sniff", 1),
                SentryAction::with_ability("Firewall Up", 1, SentryAbility::Firewall(8)),
                SentryAction::new("Trace Sweep", 2),
                SentryAction::with_ability("Inject Malware", 1, SentryAbility::InjectMalware(1)),
                SentryAction::new("Lockdown Probe", 3),
            ]
        );
        assert_eq!(standard, Sentry::warden_7());
    }

    /// Runs `sentry`'s turns until `name` is the queued action, then performs
    /// it and applies its firewall the way `SentryNode` does. Panics if the
    /// script never reaches it.
    fn perform_until(sentry: &mut Sentry, name: &str) -> SentryAction {
        for _ in 0..sentry.script.len() {
            if sentry.queued_action(0).map(|action| action.name.as_str()) == Some(name) {
                let action = sentry.perform_queued_action(0).unwrap();
                sentry.raise_firewall(action.ability.firewall_raised());
                return action;
            }
            sentry.perform_queued_action(0);
        }
        panic!("{} never queues {name}", sentry.name());
    }

    /// Varied security card E4a: standard security shields itself, so even an
    /// ordinary node asks the player for more than outlasting the meter. It
    /// is the same firewall BLACK MONOLITH raises, smaller, in every zone.
    #[test]
    fn standard_security_raises_a_firewall_up_for_8() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let firewall_up = standard
                .script
                .iter()
                .find(|action| action.name == "Firewall Up")
                .unwrap_or_else(|| panic!("zone {zone}: no Firewall Up"));

            assert_eq!(
                firewall_up.ability,
                SentryAbility::Firewall(8),
                "zone {zone}"
            );
            assert_eq!(firewall_up.ability.describe(), "blocks 8 damage");
        }
    }

    /// Varied security card E4b: standard security plants a Malware junk card
    /// in the player's discard pile, announced with the intent like any other
    /// ability. `SentryNode` and `DrawPhase` do the planting; this is what
    /// tells them how many.
    #[test]
    fn standard_security_injects_malware() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let inject = standard
                .script
                .iter()
                .find(|action| action.name == "Inject Malware")
                .unwrap_or_else(|| panic!("zone {zone}: no Inject Malware"));

            assert_eq!(
                inject.ability,
                SentryAbility::InjectMalware(1),
                "zone {zone}"
            );
            assert_eq!(inject.ability.malware_planted(), 1);
            assert_eq!(
                inject.ability.describe(),
                "adds 1 Injected Malware to your discard"
            );
        }
    }

    /// Only Inject Malware plants anything, and a nonsense count plants
    /// nothing rather than taking cards away.
    #[test]
    fn only_inject_malware_plants_malware() {
        assert_eq!(SentryAbility::InjectMalware(2).malware_planted(), 2);
        assert_eq!(SentryAbility::InjectMalware(-1).malware_planted(), 0);
        assert_eq!(SentryAbility::InjectMalware(2).firewall_raised(), 0);
        assert_eq!(SentryAbility::InjectMalware(2).energy_drain(), 0);
        assert_eq!(SentryAbility::InjectMalware(2).integrity_restored(), 0);
        assert!(!SentryAbility::InjectMalware(2).purges_corruption());

        for other in [
            SentryAbility::None,
            SentryAbility::DrainEnergy(2),
            SentryAbility::RestoreIntegrity(8),
            SentryAbility::Purge,
            SentryAbility::Firewall(8),
        ] {
            assert_eq!(other.malware_planted(), 0, "{other:?}");
        }
    }

    /// Varied security card E4a, scenarios 1 and 2 as the card words them:
    /// Firewall Up leaves 8 standing, and a 10-damage hit spends all 8 before
    /// taking 2 off 30 integrity.
    #[test]
    fn firewall_up_is_used_up_before_integrity() {
        let mut warden = Sentry::warden_7();
        warden.take_damage(warden.max_health() - 30);

        perform_until(&mut warden, "Firewall Up");
        assert_eq!(warden.firewall(), 8);

        assert_eq!(
            warden.take_blockable_damage(10),
            BlockedDamage {
                blocked: 8,
                dealt: 2
            }
        );
        assert_eq!(warden.firewall(), 0);
        assert_eq!(warden.health(), 28);
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
    /// a regular encounter. Standard security has abilities of its own since
    /// E4, so the boss's signature is checked against them rather than
    /// against standard security doing nothing at all.
    #[test]
    fn a_boss_carries_one_signature_ability_standard_security_does_not() {
        for zone in 0..4 {
            let boss = Sentry::boss_for_zone(zone);
            assert!(boss.has_ability(), "zone {zone}: the boss needs an ability");

            let abilities: Vec<SentryAbility> = boss
                .script
                .iter()
                .map(|action| action.ability)
                .filter(|ability| !ability.is_none())
                .collect();
            assert_eq!(
                abilities.len(),
                1,
                "zone {zone}: one signature ability per boss"
            );

            let standard = Sentry::standard_for_zone(zone);
            assert!(
                standard
                    .script
                    .iter()
                    .all(|action| action.ability != abilities[0]),
                "zone {zone}: {:?} is already met on an ordinary node",
                abilities[0]
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
        let boss = Sentry::boss_for_zone(2);
        let lockdown = signature(&boss);

        assert_eq!(lockdown.name, "Grid Lockdown");
        assert_eq!(lockdown.ability.energy_drain(), 3);
        assert_eq!(lockdown.ability.describe(), "-3 energy");
        assert_eq!(SentryAbility::None.energy_drain(), 0);
        assert_eq!(SentryAbility::None.describe(), "");
        assert_eq!(
            SentryAbility::DrainEnergy(-3).energy_drain(),
            0,
            "a nonsense drain takes nothing rather than handing energy out"
        );
    }

    #[test]
    fn for_encounter_fields_the_construct_for_each_tier() {
        for zone in 0..3 {
            assert_eq!(
                Sentry::for_encounter(zone, SecurityTier::Standard),
                Sentry::standard_for_zone(zone)
            );
            assert_eq!(
                Sentry::for_encounter(zone, SecurityTier::Elite),
                Sentry::elite_for_zone(zone)
            );
            assert_eq!(
                Sentry::for_encounter(zone, SecurityTier::Boss),
                Sentry::boss_for_zone(zone)
            );
        }
    }

    /// Elite card scenario 1: an elite's integrity pool is higher than the
    /// standard construct's in the same zone. The boss stays the longest fight.
    #[test]
    fn an_elite_has_more_integrity_than_standard_and_less_than_the_boss() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            let elite = Sentry::elite_for_zone(zone);
            let boss = Sentry::boss_for_zone(zone);

            assert!(
                elite.max_health() > standard.max_health(),
                "zone {zone}: elite {} against standard {}",
                elite.max_health(),
                standard.max_health()
            );
            assert!(
                boss.max_health() > elite.max_health(),
                "zone {zone}: boss {} against elite {}",
                boss.max_health(),
                elite.max_health()
            );
            assert_eq!(elite.health(), elite.max_health(), "starts at full");
        }
    }

    /// Elite card scenario 1, against zone 1 exactly as the card words it.
    #[test]
    fn the_first_zone_elite_beats_warden_7() {
        let elite = Sentry::elite_for_zone(0);
        assert_eq!(elite.name(), "BASTION");
        assert_eq!(elite.max_health(), 60);
        assert!(elite.max_health() > Sentry::warden_7().max_health());
        assert!(elite.has_ability());
    }

    /// Every ability in `sentry`'s script, in order, leaving out plain noise.
    fn abilities(sentry: &Sentry) -> Vec<SentryAbility> {
        sentry
            .script
            .iter()
            .map(|action| action.ability)
            .filter(|ability| !ability.is_none())
            .collect()
    }

    /// Elite card scenario 1: an ability standard constructs do not have.
    /// Reinforce is the elite's alone; the boss has its lockdown instead. An
    /// elite is built from the standard script, so it also runs everything
    /// standard security does (E4).
    #[test]
    fn only_an_elite_reinforces() {
        for zone in 0..4 {
            let mut expected = abilities(&Sentry::standard_for_zone(zone));
            expected.push(SentryAbility::RestoreIntegrity(8 + 4 * zone as i32));
            assert_eq!(
                abilities(&Sentry::elite_for_zone(zone)),
                expected,
                "zone {zone}: the standard abilities, then one Reinforce restoring \
                 more the deeper the zone"
            );

            for other in [Sentry::standard_for_zone(zone), Sentry::boss_for_zone(zone)] {
                assert!(
                    other
                        .script
                        .iter()
                        .all(|action| action.ability.integrity_restored() == 0),
                    "zone {zone}: {} should not reinforce",
                    other.name()
                );
            }
        }
    }

    #[test]
    fn an_elite_resists_detection_between_standard_and_boss() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone).detection_resistance();
            let elite = Sentry::elite_for_zone(zone).detection_resistance();
            let boss = Sentry::boss_for_zone(zone).detection_resistance();

            assert!(
                standard < elite && elite < boss,
                "zone {zone}: {standard} < {elite} < {boss}"
            );
        }
    }

    /// An elite is something the player has not met on an ordinary node, so
    /// none of its actions share a name with the standard script.
    #[test]
    fn an_elite_script_shares_no_action_with_the_standard_one() {
        for zone in 0..4 {
            let standard = Sentry::standard_for_zone(zone);
            for action in &Sentry::elite_for_zone(zone).script {
                assert!(
                    standard
                        .script
                        .iter()
                        .all(|other| other.name != action.name),
                    "zone {zone}: {} is not an elite-only action",
                    action.name
                );
            }
        }
    }

    #[test]
    fn a_zone_past_the_authored_names_still_has_a_named_elite() {
        let elite = Sentry::elite_for_zone(ELITE_NAMES.len());
        assert!(!elite.name().is_empty());
        assert!(elite.has_ability());
    }

    #[test]
    fn reinforce_is_announced_with_the_intent() {
        let reinforce = SentryAbility::RestoreIntegrity(8);
        assert_eq!(reinforce.describe(), "+8 integrity");
        assert_eq!(reinforce.integrity_restored(), 8);
        assert_eq!(reinforce.energy_drain(), 0, "reinforcing costs no energy");
        assert_eq!(SentryAbility::DrainEnergy(2).integrity_restored(), 0);
        assert_eq!(
            SentryAbility::RestoreIntegrity(-5).integrity_restored(),
            0,
            "a nonsense restore does no damage"
        );
    }

    #[test]
    fn restoring_integrity_clamps_at_max_and_reports_what_was_regained() {
        let mut elite = Sentry::elite_for_zone(0);
        assert_eq!(elite.restore_integrity(8), 0, "nothing to regain at full");

        elite.take_damage(5);
        assert_eq!(elite.restore_integrity(8), 5, "clamped, not the full 8");
        assert_eq!(elite.health(), elite.max_health());

        elite.take_damage(20);
        assert_eq!(elite.restore_integrity(8), 8);
        assert_eq!(elite.health(), elite.max_health() - 12);

        assert_eq!(
            elite.restore_integrity(-30),
            0,
            "a negative restore deals nothing"
        );
        assert_eq!(elite.health(), elite.max_health() - 12);
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

    /// Boss card E2a: a three-zone contract meets three different bosses, in
    /// zone order, and none of them is a numbered fallback.
    #[test]
    fn each_zone_of_a_three_zone_contract_has_its_own_named_boss() {
        let names: Vec<String> = (0..3)
            .map(|zone| Sentry::boss_for_zone(zone).name().to_string())
            .collect();

        assert_eq!(names, BOSS_NAMES);
        for (i, name) in names.iter().enumerate() {
            assert!(
                !names[i + 1..].contains(name),
                "{name} guards more than one zone"
            );
        }
    }

    /// Boss card E2a: each zone's boss has more integrity than the last, so a
    /// deeper boss is a longer fight as well as a louder one. Checked past the
    /// authored names too, so a longer contract keeps climbing.
    #[test]
    fn each_zone_boss_has_more_integrity_than_the_last() {
        for zone in 1..5 {
            let boss = Sentry::boss_for_zone(zone);
            let previous = Sentry::boss_for_zone(zone - 1);
            assert!(
                boss.max_health() > previous.max_health(),
                "zone {}: {} has {} integrity against {}'s {}",
                zone + 1,
                boss.name(),
                boss.max_health(),
                previous.name(),
                previous.max_health()
            );
            assert_eq!(boss.health(), boss.max_health(), "starts at full");
        }
    }

    /// The first boss keeps the 80 integrity it shipped with in E1, so the
    /// climb starts from the balance the team has already played.
    #[test]
    fn the_first_zone_boss_keeps_its_integrity() {
        assert_eq!(Sentry::boss_for_zone(0).max_health(), 80);
    }

    /// The one action in a boss's script that does more than make noise.
    fn signature(boss: &Sentry) -> &SentryAction {
        boss.script
            .iter()
            .find(|action| !action.ability.is_none())
            .expect("every boss has a signature ability")
    }

    /// Boss card E2b: each authored boss has its own signature ability.
    /// ICEBREAKER purges corruption, BLACK MONOLITH raises a firewall and THE
    /// ARCHITECT keeps the Grid Lockdown, so no two bosses ask the same thing
    /// of the player's deck.
    #[test]
    fn each_authored_boss_has_its_own_signature_ability() {
        let signatures: Vec<(String, SentryAbility)> = (0..BOSS_NAMES.len())
            .map(|zone| {
                let boss = Sentry::boss_for_zone(zone);
                let action = signature(&boss);
                (action.name.clone(), action.ability)
            })
            .collect();

        assert_eq!(
            signatures,
            vec![
                ("Purge".to_string(), SentryAbility::Purge),
                (
                    "Firewall".to_string(),
                    SentryAbility::Firewall(FIREWALL_BLOCK)
                ),
                ("Grid Lockdown".to_string(), SentryAbility::DrainEnergy(3)),
            ]
        );
    }

    /// A contract deeper than the authored bosses falls back to the lockdown,
    /// so a numbered boss still has something to announce.
    #[test]
    fn a_boss_past_the_authored_ones_falls_back_to_the_lockdown() {
        let zone = BOSS_NAMES.len();
        let boss = Sentry::boss_for_zone(zone);
        let lockdown = signature(&boss);
        assert_eq!(lockdown.name, "Grid Lockdown");
        assert_eq!(
            lockdown.ability,
            SentryAbility::DrainEnergy(1 + zone as i32)
        );
    }

    /// Boss card E2b: what a signature ability does is shown with the intent,
    /// before the player ends the turn into it.
    #[test]
    fn signature_abilities_say_what_they_do_on_the_intent() {
        assert_eq!(SentryAbility::Purge.describe(), "clears corruption");
        assert_eq!(SentryAbility::Firewall(12).describe(), "blocks 12 damage");
        assert_eq!(
            SentryAbility::Firewall(-4).describe(),
            "blocks 0 damage",
            "a nonsense firewall blocks nothing rather than going negative"
        );
    }

    /// Only Purge purges and only Firewall blocks, so the node can apply every
    /// ability the same way without matching on which one it is.
    #[test]
    fn each_ability_reports_only_its_own_effect() {
        assert!(SentryAbility::Purge.purges_corruption());
        assert_eq!(SentryAbility::Purge.firewall_raised(), 0);
        assert_eq!(SentryAbility::Purge.energy_drain(), 0);

        assert_eq!(SentryAbility::Firewall(12).firewall_raised(), 12);
        assert_eq!(SentryAbility::Firewall(-3).firewall_raised(), 0);
        assert!(!SentryAbility::Firewall(12).purges_corruption());
        assert_eq!(SentryAbility::Firewall(12).integrity_restored(), 0);

        for other in [
            SentryAbility::None,
            SentryAbility::DrainEnergy(2),
            SentryAbility::RestoreIntegrity(8),
        ] {
            assert!(!other.purges_corruption(), "{other:?}");
            assert_eq!(other.firewall_raised(), 0, "{other:?}");
        }
    }

    /// Purge wipes every point of corruption on the construct and reports how
    /// much went, so a corruption deck has to land its damage before it does.
    #[test]
    fn purge_clears_all_corruption_and_reports_how_much() {
        let mut boss = Sentry::boss_for_zone(0);
        boss.add_corruption(5);

        assert_eq!(boss.purge_corruption(), 5);
        assert_eq!(boss.corruption(), 0);
        assert_eq!(boss.purge_corruption(), 0, "nothing left to purge");
    }

    /// A firewall soaks up plain damage before integrity, and reports what it
    /// blocked separately from what got through.
    #[test]
    fn a_firewall_absorbs_blockable_damage_until_it_runs_out() {
        let mut boss = Sentry::boss_for_zone(1);
        assert_eq!(boss.firewall(), 0, "no firewall until the ability runs");
        assert_eq!(boss.raise_firewall(12), 12);

        assert_eq!(
            boss.take_blockable_damage(8),
            BlockedDamage {
                blocked: 8,
                dealt: 0
            }
        );
        assert_eq!(boss.health(), boss.max_health());
        assert_eq!(boss.firewall(), 4);

        assert_eq!(
            boss.take_blockable_damage(10),
            BlockedDamage {
                blocked: 4,
                dealt: 6
            }
        );
        assert_eq!(boss.health(), boss.max_health() - 6);
        assert_eq!(boss.firewall(), 0);
    }

    /// Penetrating damage and corruption go through `take_damage`, which a
    /// firewall does not stand in front of.
    #[test]
    fn unblockable_damage_ignores_the_firewall() {
        let mut boss = Sentry::boss_for_zone(1);
        boss.raise_firewall(12);

        assert_eq!(boss.take_damage(6), 6);
        assert_eq!(boss.firewall(), 12, "untouched");

        boss.add_corruption(3);
        assert_eq!(boss.resolve_corruption_tick(), 3);
        assert_eq!(boss.firewall(), 12);
    }

    /// The firewall covers one player turn: it comes down when the boss's next
    /// turn starts, and raising it never stacks or goes negative.
    #[test]
    fn a_firewall_comes_down_and_does_not_stack() {
        let mut boss = Sentry::boss_for_zone(1);
        boss.raise_firewall(12);
        assert_eq!(boss.raise_firewall(12), 12, "raised again, not doubled");
        assert_eq!(
            boss.raise_firewall(-5),
            12,
            "a negative amount changes nothing"
        );

        assert_eq!(boss.drop_firewall(), 12);
        assert_eq!(boss.firewall(), 0);
        assert_eq!(boss.drop_firewall(), 0);
        assert_eq!(
            boss.take_blockable_damage(5),
            BlockedDamage {
                blocked: 0,
                dealt: 5
            }
        );
    }

    fn action_names(actions: &[SentryAction]) -> Vec<&str> {
        actions.iter().map(|action| action.name.as_str()).collect()
    }

    /// Boss card E2c: only the final boss escalates. The earlier bosses fight
    /// the same way all the way down.
    #[test]
    fn only_the_architect_has_a_second_phase() {
        for zone in 0..BOSS_NAMES.len() {
            let mut boss = Sentry::boss_for_zone(zone);
            assert_eq!(boss.phase(), 1, "zone {zone}: every boss starts in phase 1");
            assert_eq!(boss.has_second_phase(), zone == 2, "zone {zone}");

            boss.take_damage(boss.max_health() / 2 + 1);
            assert_eq!(boss.phase(), if zone == 2 { 2 } else { 1 }, "zone {zone}");
        }
        assert!(!Sentry::standard_for_zone(2).has_second_phase());
        assert!(!Sentry::elite_for_zone(2).has_second_phase());
    }

    /// Boss card E2c: THE ARCHITECT switches once its integrity drops below
    /// half, and not a point before. Exactly half is still phase 1.
    #[test]
    fn the_architect_escalates_once_below_half_integrity() {
        let mut architect = Sentry::boss_for_zone(2);
        assert_eq!(architect.max_health(), 120);

        architect.take_damage(60);
        assert_eq!(architect.health(), 60);
        assert_eq!(architect.phase(), 1, "exactly half is not below half");

        architect.take_damage(1);
        assert_eq!(architect.phase(), 2);
        assert_eq!(
            architect
                .queued_action(0)
                .map(|action| action.name.as_str()),
            Some("Rewrite Protocol"),
            "the new action set starts from its first action"
        );
    }

    /// Boss card E2c: the second phase is a new action set, sharing nothing
    /// with the first, and it carries every other boss's signature: Purge,
    /// Firewall and a harder lockdown.
    #[test]
    fn the_second_phase_is_a_new_action_set() {
        let mut architect = Sentry::boss_for_zone(2);
        let first_phase = architect.script.clone();
        architect.take_damage(61);

        assert_eq!(
            action_names(&architect.script),
            vec!["Rewrite Protocol", "Fortify Core", "Total Lockdown"]
        );
        for name in action_names(&architect.script) {
            assert!(
                !action_names(&first_phase).contains(&name),
                "{name} is already in phase 1"
            );
        }
        let abilities: Vec<SentryAbility> = architect
            .script
            .iter()
            .map(|action| action.ability)
            .collect();
        assert_eq!(
            abilities,
            vec![
                SentryAbility::Purge,
                SentryAbility::Firewall(FIREWALL_BLOCK),
                SentryAbility::DrainEnergy(3),
            ]
        );

        // Still louder than anything standard security does in its zone.
        for action in &architect.script {
            assert!(
                action.noise > Sentry::loudest_standard_noise(2),
                "{}",
                action.name
            );
        }
    }

    /// Corruption ticking it below half escalates it too, since every source
    /// of damage goes through the same check.
    #[test]
    fn a_corruption_tick_can_escalate_the_architect() {
        let mut architect = Sentry::boss_for_zone(2);
        architect.take_damage(58);
        architect.add_corruption(4);

        assert_eq!(architect.resolve_corruption_tick(), 4);
        assert_eq!(architect.health(), 58);
        assert_eq!(architect.phase(), 2);
    }

    /// The escalation happens once. Reinforcing above half does not put it
    /// back, and later damage does not restart the new action set.
    #[test]
    fn the_second_phase_holds_once_reached() {
        let mut architect = Sentry::boss_for_zone(2);
        architect.take_damage(61);
        architect.perform_queued_action(0);

        architect.restore_integrity(50);
        assert_eq!(architect.phase(), 2);

        architect.take_damage(10);
        assert_eq!(
            architect
                .queued_action(0)
                .map(|action| action.name.as_str()),
            Some("Fortify Core"),
            "the cycle carries on where it was"
        );
    }

    /// A blow that takes it straight to zero ends the fight rather than
    /// starting a phase it will never act in.
    #[test]
    fn a_killing_blow_does_not_escalate() {
        let mut architect = Sentry::boss_for_zone(2);
        architect.take_damage(architect.max_health());
        assert!(architect.is_defeated());
        assert_eq!(architect.phase(), 1);
    }

    #[test]
    fn a_renamed_sentry_reports_the_new_name() {
        let mut sentry = Sentry::warden_7();
        sentry.set_name("BLACKGATE");
        assert_eq!(sentry.name(), "BLACKGATE");
    }

    #[test]
    fn a_fresh_sentry_starts_at_full_health() {
        let sentry = Sentry::warden_7();
        assert_eq!(sentry.health(), sentry.max_health());
        assert_eq!(sentry.health(), 40);
        assert!(!sentry.is_defeated());
    }

    #[test]
    fn with_health_sets_both_current_and_max() {
        let sentry = Sentry::warden_7().with_health(10);
        assert_eq!(sentry.health(), 10);
        assert_eq!(sentry.max_health(), 10);
    }

    #[test]
    fn damage_clamps_at_zero_and_reports_actual_amount_lost() {
        let mut sentry = Sentry::warden_7().with_health(10);
        assert_eq!(sentry.take_damage(6), 6);
        assert_eq!(sentry.health(), 4);
        assert!(!sentry.is_defeated());

        assert_eq!(sentry.take_damage(10), 4, "clamped, not the full 10");
        assert_eq!(sentry.health(), 0);
        assert!(sentry.is_defeated());

        assert_eq!(sentry.take_damage(5), 0, "already dead, no further loss");
        assert_eq!(sentry.health(), 0);
    }

    #[test]
    fn damage_does_not_overshoot_max_health_when_negative() {
        let mut sentry = Sentry::warden_7().with_health(10);
        sentry.take_damage(5);
        assert_eq!(sentry.health(), 5);
        assert_eq!(
            sentry.take_damage(-100),
            -5,
            "clamped to max, not the full heal"
        );
        assert_eq!(sentry.health(), 10);
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
                let announced = sentry.queued_action(meter.noise()).cloned().unwrap();
                let action = sentry.perform_queued_action(meter.noise()).unwrap();
                assert_eq!(action, announced);
                let before = meter.noise();
                let delta = meter.add(action.noise);
                assert_eq!(meter.noise() - before, delta);
                assert!((0..=cap).contains(&meter.noise()));
                assert_eq!(meter.is_at_cap(), meter.noise() == cap);
            }
        }
    }

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn damage_of_any_size_keeps_health_within_bounds() {
        let mut seed = 0xDEAD_BEEF_u32;
        for max_health in [0, 1, 40, 1_000] {
            let mut sentry = Sentry::warden_7().with_health(max_health);
            for _ in 0..50_000 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let amount = seed as i32;
                let before = sentry.health();
                let lost = sentry.take_damage(amount);
                assert_eq!(sentry.health(), before - lost);
                assert!((0..=max_health).contains(&sentry.health()));
                assert_eq!(sentry.is_defeated(), sentry.health() == 0);
            }
        }
    }

    /// Story #101, scenario 1: at or above `ALERT_NOISE`, standard security
    /// picks from its alert set.
    #[test]
    fn standard_security_escalates_at_high_noise() {
        let sentry = Sentry::warden_7();

        assert!(sentry.is_on_alert(ALERT_NOISE));
        let action = sentry.queued_action(ALERT_NOISE).unwrap();
        assert_eq!(action.name, "Deep Packet Inspection");
        assert_eq!(action.noise, 2);
    }

    /// Story #101, scenario 2: below `ALERT_NOISE` it keeps to its standard
    /// script.
    #[test]
    fn standard_security_stays_calm_below_the_alert_noise() {
        let sentry = Sentry::warden_7();

        assert!(!sentry.is_on_alert(ALERT_NOISE - 1));
        assert_eq!(
            sentry.queued_action(ALERT_NOISE - 1).unwrap().name,
            "Packet Sniff"
        );
    }

    /// Noise picks between the standard and alert action for the slot that
    /// is queued; it never moves the queue. So the intent can follow the
    /// meter while the player plays cards, and the next slot still comes
    /// next whichever set the last action came from.
    #[test]
    fn noise_changes_the_queued_action_but_not_the_slot() {
        let mut sentry = Sentry::warden_7();

        assert_eq!(sentry.queued_action(0).unwrap().name, "Packet Sniff");
        assert_eq!(
            sentry.queued_action(ALERT_NOISE).unwrap().name,
            "Deep Packet Inspection"
        );
        assert_eq!(sentry.queued_action(0).unwrap().name, "Packet Sniff");

        let performed = sentry.perform_queued_action(ALERT_NOISE).unwrap();
        assert_eq!(performed.name, "Deep Packet Inspection");
        assert_eq!(sentry.queued_action(0).unwrap().name, "Firewall Up");
        assert_eq!(
            sentry.queued_action(ALERT_NOISE).unwrap().name,
            "Firewall Surge"
        );
    }

    fn alert_pairs(sentry: &Sentry) -> Vec<(SentryAction, SentryAction)> {
        let mut sentry = sentry.clone();
        (0..sentry.script.len())
            .map(|_| {
                let calm = sentry.queued_action(0).unwrap().clone();
                let alert = sentry.perform_queued_action(ALERT_NOISE).unwrap();
                (calm, alert)
            })
            .collect()
    }

    /// Every alert action, standard and elite, is louder than the one it
    /// replaces and has a name of its own, so being on alert is a real cost
    /// the player can see coming.
    #[test]
    fn every_alert_action_is_louder_than_the_one_it_replaces() {
        for zone in 0..4 {
            for sentry in [
                Sentry::standard_for_zone(zone),
                Sentry::elite_for_zone(zone),
            ] {
                for (calm, alert) in alert_pairs(&sentry) {
                    assert!(
                        alert.noise > calm.noise,
                        "{} in zone {zone}: {} ({}) is not louder than {} ({})",
                        sentry.name(),
                        alert.name,
                        alert.noise,
                        calm.name,
                        calm.noise
                    );
                    assert_ne!(alert.name, calm.name);
                }
            }
        }
    }

    /// Being on alert stops short of a boss turn: every standard alert action
    /// is still quieter than every action the zone's boss takes.
    #[test]
    fn a_standard_alert_turn_stays_quieter_than_the_zone_boss() {
        for zone in 0..4 {
            let boss = Sentry::boss_for_zone(zone);
            let quietest_boss = boss.script.iter().map(|action| action.noise).min().unwrap();
            for (_, alert) in alert_pairs(&Sentry::standard_for_zone(zone)) {
                assert!(
                    alert.noise < quietest_boss,
                    "zone {zone}: {} ({}) is as loud as {}'s quietest action ({})",
                    alert.name,
                    alert.noise,
                    boss.name(),
                    quietest_boss
                );
            }
        }
    }

    /// The two alert actions with abilities hit harder than their standard
    /// ones, and Firewall Surge stays short of BLACK MONOLITH's firewall.
    #[test]
    fn alert_abilities_hit_harder_than_standard_ones() {
        let pairs = alert_pairs(&Sentry::warden_7());
        let (firewall_up, surge) = &pairs[1];
        let (inject, flood) = &pairs[3];

        assert_eq!(surge.name, "Firewall Surge");
        assert!(surge.ability.firewall_raised() > firewall_up.ability.firewall_raised());
        assert!(surge.ability.firewall_raised() < FIREWALL_BLOCK);
        assert_eq!(flood.name, "Malware Flood");
        assert_eq!(flood.ability.malware_planted(), 2);
        assert_eq!(inject.ability.malware_planted(), 1);
    }

    /// An elite on alert still Reinforces, louder, for the same integrity.
    #[test]
    fn an_elite_on_alert_still_reinforces() {
        let elite = Sentry::elite_for_zone(0);
        let (reinforce, emergency) = alert_pairs(&elite).pop().unwrap();

        assert_eq!(reinforce.name, "Reinforce");
        assert_eq!(emergency.name, "Emergency Reinforce");
        assert_eq!(
            emergency.ability.integrity_restored(),
            reinforce.ability.integrity_restored()
        );
    }

    /// Bosses do not react to noise: their script is the same at any level,
    /// and THE ARCHITECT's second phase is its only change of action set.
    #[test]
    fn a_boss_does_not_react_to_noise() {
        for zone in 0..4 {
            let boss = Sentry::boss_for_zone(zone);
            assert!(!boss.is_on_alert(100), "{} goes on alert", boss.name());
            assert_eq!(boss.queued_action(100), boss.queued_action(0));
        }
    }
}

use godot::prelude::*;

/// Noise plus the shield currently protecting it. Shield absorbs positive
/// noise before any of it reaches the meter — 1 shield blocks 1 noise,
/// regardless of whether that noise came from the player's own card or from
/// the sentry's turn, since both route through the same `add`. Shield never
/// blocks a reduction (negative `amount`, e.g. a recovery card): those apply
/// to noise directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoiseLevel {
    noise: i32,
    max_noise: i32,
    /// How hard the construct currently being faced pushes back on the player
    /// lowering detection, as a percentage. 0 leaves recovery untouched, which
    /// is what every encounter did before bosses existed.
    resistance_percent: i32,
    shield: i32,
}

impl NoiseLevel {
    pub fn new(max_noise: i32) -> Self {
        Self {
            noise: 0,
            max_noise: max_noise.max(0),
            resistance_percent: 0,
            shield: 0,
        }
    }

    pub fn noise(&self) -> i32 {
        self.noise
    }

    pub fn max_noise(&self) -> i32 {
        self.max_noise
    }

    pub fn resistance_percent(&self) -> i32 {
        self.resistance_percent
    }

    /// Sets how hard recovery is resisted, clamped to 0..=100. Set per
    /// encounter by whichever construct is guarding it.
    pub fn set_resistance_percent(&mut self, percent: i32) {
        self.resistance_percent = percent.clamp(0, 100);
    }

    /// Detection resistance works against the player pulling noise back down,
    /// never against the security system putting it up: a hardened construct
    /// is hard to hide from, not louder by itself.
    ///
    /// Computed in i64 and rounded towards zero, so a reduction can be blunted
    /// to nothing but can never flip sign or overflow on its way to the clamp.
    fn resisted(&self, amount: i32) -> i32 {
        if amount >= 0 || self.resistance_percent <= 0 {
            return amount;
        }
        let kept = 100 - i64::from(self.resistance_percent);
        (i64::from(amount) * kept / 100) as i32
    }

    pub fn shield(&self) -> i32 {
        self.shield
    }

    /// Applies `amount` to noise, first spending shield to absorb as much of
    /// a positive `amount` as it can cover. Returns the actual signed change
    /// to `noise` itself (not counting what shield absorbed), same clamped-
    /// delta contract as before — so a fully-shielded hit reports `0` here
    /// even though shield went down.
    pub fn add(&mut self, amount: i32) -> i32 {
        let before = self.noise;
        if amount > 0 {
            let blocked = amount.min(self.shield);
            self.shield -= blocked;
            let remaining = amount - blocked;
            self.noise = self
                .noise
                .saturating_add(remaining)
                .clamp(0, self.max_noise);
        } else {
            self.noise = self
                .noise
                .saturating_add(self.resisted(amount))
                .clamp(0, self.max_noise);
        }
        self.noise - before
    }

    /// Adds shield, e.g. from a played Block card. Shield has no upper cap of
    /// its own; only ever clamped so it can't go negative. Returns the actual
    /// amount added (mirrors `add`'s clamped-delta contract).
    pub fn add_shield(&mut self, amount: i32) -> i32 {
        let before = self.shield;
        self.shield = self.shield.saturating_add(amount).max(0);
        self.shield - before
    }

    /// Wipes any shield left at end of turn, so it doesn't carry over.
    /// Returns how much was cleared, for display/logging.
    pub fn clear_shield(&mut self) -> i32 {
        let cleared = self.shield;
        self.shield = 0;
        cleared
    }

    pub fn is_at_cap(&self) -> bool {
        self.noise >= self.max_noise
    }
}

#[derive(GodotClass)]
#[class(base=Node)]
pub struct NoiseMeter {
    base: Base<Node>,
    level: NoiseLevel,

    // Computed, read-only properties: Inspector/script writes cannot create a
    // second value that disagrees with card transactions or cap detection.
    #[var(get = get_noise, no_set)]
    noise: PhantomVar<i32>,
    #[var(get = get_max_noise, no_set)]
    max_noise: PhantomVar<i32>,
    #[var(get = get_shield, no_set)]
    shield: PhantomVar<i32>,
}

#[godot_api]
impl INode for NoiseMeter {
    fn init(base: Base<Node>) -> Self {
        let level = NoiseLevel::new(100);
        Self {
            noise: PhantomVar::default(),
            max_noise: PhantomVar::default(),
            shield: PhantomVar::default(),
            level,
            base,
        }
    }
}

#[godot_api]
impl NoiseMeter {
    #[func]
    pub fn get_noise(&self) -> i32 {
        self.level.noise()
    }

    #[func]
    pub fn get_max_noise(&self) -> i32 {
        self.level.max_noise()
    }

    #[func]
    pub fn get_shield(&self) -> i32 {
        self.level.shield()
    }

    #[func]
    pub fn add_noise(&mut self, amount: i32) -> i32 {
        self.level.add(amount)
    }

    #[func]
    pub fn add_shield(&mut self, amount: i32) -> i32 {
        self.level.add_shield(amount)
    }

    #[func]
    pub fn clear_shield(&mut self) -> i32 {
        self.level.clear_shield()
    }

    #[func]
    pub fn is_at_cap(&self) -> bool {
        self.level.is_at_cap()
    }

    /// How hard the construct guarding this encounter resists recovery.
    #[func]
    pub fn get_resistance_percent(&self) -> i32 {
        self.level.resistance_percent()
    }

    /// Set when an encounter begins, by the construct guarding it. Cleared
    /// back to zero by the coordinator before a screen with no sentry on it,
    /// so a boss cannot go on resisting after the player has walked away.
    #[func]
    pub fn set_resistance_percent(&mut self, percent: i32) {
        self.level.set_resistance_percent(percent);
    }
}

impl NoiseMeter {
    /// The transaction borrows the authoritative value, never a copied mirror.
    pub(crate) fn level_mut(&mut self) -> &mut NoiseLevel {
        &mut self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_level_starts_silent() {
        let level = NoiseLevel::new(100);
        assert_eq!(level.noise(), 0);
        assert_eq!(level.shield(), 0);
        assert!(!level.is_at_cap());
    }

    #[test]
    fn adding_noise_moves_the_meter_and_reports_the_change() {
        let mut level = NoiseLevel::new(100);
        assert_eq!(level.add(15), 15);
        assert_eq!(level.noise(), 15);
    }

    #[test]
    fn noise_never_drops_below_zero() {
        let mut level = NoiseLevel::new(100);
        level.add(5);
        assert_eq!(level.add(-20), -5);
        assert_eq!(level.noise(), 0);
    }

    #[test]
    fn noise_never_exceeds_the_cap() {
        let mut level = NoiseLevel::new(10);
        assert_eq!(level.add(999), 10);
        assert!(level.is_at_cap());
    }

    #[test]
    fn an_enormous_amount_fills_the_meter_rather_than_overflowing_it() {
        let mut level = NoiseLevel::new(10);
        level.add(1);
        assert_eq!(level.add(i32::MAX), 9);
        assert_eq!(level.noise(), 10);
    }

    #[test]
    fn an_enormous_quietening_empties_the_meter_rather_than_overflowing_it() {
        let mut level = NoiseLevel::new(10);
        level.add(4);
        assert_eq!(level.add(i32::MIN), -4);
        assert_eq!(level.noise(), 0);
    }
    #[test]
    fn a_fresh_level_resists_nothing() {
        let mut level = NoiseLevel::new(100);
        assert_eq!(level.resistance_percent(), 0);

        level.add(50);
        assert_eq!(level.add(-10), -10, "recovery lands in full by default");
        assert_eq!(level.noise(), 40);
    }

    /// Resistance is the boss making the player's recovery worth less. It is
    /// not the boss being louder, so noise going on is untouched.
    #[test]
    fn resistance_blunts_recovery_but_never_the_noise_going_on() {
        let mut level = NoiseLevel::new(100);
        level.set_resistance_percent(40);

        assert_eq!(level.add(50), 50, "the system's own noise is unaffected");
        assert_eq!(level.add(-10), -6, "a 10 point recovery lands as 6");
        assert_eq!(level.noise(), 44);
    }

    #[test]
    fn shield_and_boss_resistance_apply_to_opposite_noise_directions() {
        let mut level = NoiseLevel::new(100);
        level.add(50);
        level.set_resistance_percent(30);
        level.add_shield(6);

        assert_eq!(level.add(-10), -7, "resistance blunts recovery");
        assert_eq!(level.shield(), 6, "recovery does not spend shield");
        assert_eq!(level.add(7), 1, "shield absorbs positive boss noise");
        assert_eq!(level.shield(), 0);
        assert_eq!(level.noise(), 44);
    }

    #[test]
    fn total_resistance_cancels_recovery_without_reversing_it() {
        let mut level = NoiseLevel::new(100);
        level.add(30);
        level.set_resistance_percent(100);

        assert_eq!(level.add(-25), 0, "nothing comes off");
        assert_eq!(level.noise(), 30, "and nothing goes on either");
    }

    #[test]
    fn a_resisted_recovery_rounds_towards_zero_rather_than_paying_out() {
        let mut level = NoiseLevel::new(100);
        level.add(50);
        level.set_resistance_percent(99);

        // 1% of 5 is a fraction of a point, which is worth nothing rather
        // than being rounded up into a free point of recovery.
        assert_eq!(level.add(-5), 0);
        assert_eq!(level.noise(), 50);
    }

    #[test]
    fn resistance_outside_its_range_is_clamped_rather_than_believed() {
        let mut level = NoiseLevel::new(100);

        level.set_resistance_percent(-30);
        assert_eq!(level.resistance_percent(), 0);

        level.set_resistance_percent(250);
        assert_eq!(level.resistance_percent(), 100);
    }

    /// The saturating add is what stops a loud action overflowing the meter,
    /// and resistance must not open that back up.
    #[test]
    fn an_enormous_quietening_is_still_safe_against_resistance() {
        for percent in [0, 1, 50, 99, 100] {
            let mut level = NoiseLevel::new(10);
            level.set_resistance_percent(percent);
            level.add(4);

            let change = level.add(i32::MIN);
            assert!(
                (0..=10).contains(&level.noise()),
                "resistance {percent} left the meter at {}",
                level.noise()
            );
            assert!(change <= 0, "a quietening cannot raise the meter");
        }
    }

    #[test]
    fn shield_fully_absorbs_noise_up_to_its_amount() {
        let mut level = NoiseLevel::new(100);
        level.add_shield(5);
        assert_eq!(level.add(3), 0, "3 noise fully absorbed by 5 shield");
        assert_eq!(level.noise(), 0);
        assert_eq!(level.shield(), 2, "2 shield left after blocking 3");
    }

    #[test]
    fn shield_partially_absorbs_noise_larger_than_itself() {
        let mut level = NoiseLevel::new(100);
        level.add_shield(4);
        assert_eq!(level.add(10), 6, "4 blocked, 6 gets through");
        assert_eq!(level.noise(), 6);
        assert_eq!(level.shield(), 0);
    }

    #[test]
    fn shield_does_not_block_noise_reduction() {
        let mut level = NoiseLevel::new(100);
        level.add(20);
        level.add_shield(5);
        assert_eq!(level.add(-8), -8, "recovery is unaffected by shield");
        assert_eq!(level.noise(), 12);
        assert_eq!(level.shield(), 5, "shield untouched by a reduction");
    }

    #[test]
    fn shield_blocks_noise_regardless_of_source() {
        // The meter can't distinguish a player's own card noise from the
        // sentry's noise — both go through `add`, so shield blocks either.
        let mut level = NoiseLevel::new(100);
        level.add_shield(10);
        assert_eq!(level.add(4), 0, "blocks the player's own card noise");
        assert_eq!(level.shield(), 6);
        assert_eq!(level.add(6), 0, "blocks noise from another source too");
        assert_eq!(level.shield(), 0);
    }

    #[test]
    fn add_shield_reports_the_actual_amount_added() {
        let mut level = NoiseLevel::new(100);
        assert_eq!(level.add_shield(5), 5);
        assert_eq!(level.add_shield(3), 3);
        assert_eq!(level.shield(), 8);
    }

    #[test]
    fn add_shield_cannot_go_negative() {
        let mut level = NoiseLevel::new(100);
        assert_eq!(level.add_shield(-5), 0, "clamped, nothing to remove yet");
        assert_eq!(level.shield(), 0);
    }

    #[test]
    fn clear_shield_zeroes_it_and_reports_what_was_cleared() {
        let mut level = NoiseLevel::new(100);
        level.add_shield(7);
        assert_eq!(level.clear_shield(), 7);
        assert_eq!(level.shield(), 0);
        assert_eq!(level.clear_shield(), 0, "nothing left to clear");
    }

    #[test]
    fn an_enormous_shield_and_amount_do_not_overflow() {
        let mut level = NoiseLevel::new(10);
        level.add_shield(i32::MAX);
        assert_eq!(level.add(i32::MAX), 0, "even i32::MAX noise is absorbed");
        assert_eq!(level.noise(), 0);
    }
}

#[cfg(test)]
mod stress {
    use super::*;

    const EDGE_AMOUNTS: [i32; 12] = [
        i32::MIN,
        i32::MIN + 1,
        i32::MIN / 2,
        -1_000_000,
        -11,
        -1,
        0,
        1,
        11,
        1_000_000,
        i32::MAX - 1,
        i32::MAX,
    ];

    struct Lcg(u32);
    impl Lcg {
        fn next_amount(&mut self) -> i32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.0 as i32
        }
    }

    fn assert_invariants(level: &NoiseLevel, delta: i32, before: i32) {
        assert!(level.noise() >= 0 && level.noise() <= level.max_noise());
        assert!(level.shield() >= 0);
        assert_eq!(level.noise() - before, delta);
        assert_eq!(level.is_at_cap(), level.noise() >= level.max_noise());
    }

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn every_pair_of_edge_amounts_keeps_the_meter_in_range() {
        for cap in [0, 1, 10, 1_000, i32::MAX] {
            for first in EDGE_AMOUNTS {
                for second in EDGE_AMOUNTS {
                    let mut level = NoiseLevel::new(cap);
                    let before = level.noise();
                    let delta = level.add(first);
                    assert_invariants(&level, delta, before);
                    let before = level.noise();
                    let delta = level.add(second);
                    assert_invariants(&level, delta, before);
                }
            }
        }
    }

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn a_long_run_of_random_amounts_keeps_the_meter_in_range() {
        let mut rng = Lcg(0x5EED_1234);
        for cap in [0, 1, 10, 97, i32::MAX] {
            let mut level = NoiseLevel::new(cap);
            for _ in 0..200_000 {
                let before = level.noise();
                let delta = level.add(rng.next_amount());
                assert_invariants(&level, delta, before);
            }
        }
    }

    #[test]
    #[ignore = "stress: run with --ignored"]
    fn shield_and_noise_interleaved_randomly_keep_both_in_range() {
        let mut rng = Lcg(0xC0FF_EE99);
        for cap in [0, 1, 10, 97, i32::MAX] {
            let mut level = NoiseLevel::new(cap);
            for i in 0..200_000 {
                if i % 3 == 0 {
                    let amount = rng.next_amount();
                    let before = level.shield();
                    let delta = level.add_shield(amount);
                    assert!(level.shield() >= 0);
                    assert_eq!(level.shield() - before, delta);
                } else {
                    let before = level.noise();
                    let delta = level.add(rng.next_amount());
                    assert_invariants(&level, delta, before);
                }
            }
        }
    }
}

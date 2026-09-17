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
    shield: i32,
}

impl NoiseLevel {
    pub fn new(max_noise: i32) -> Self {
        Self {
            noise: 0,
            max_noise: max_noise.max(0),
            shield: 0,
        }
    }

    pub fn noise(&self) -> i32 {
        self.noise
    }

    pub fn max_noise(&self) -> i32 {
        self.max_noise
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
            self.noise = self.noise.saturating_add(remaining).clamp(0, self.max_noise);
        } else {
            self.noise = self.noise.saturating_add(amount).clamp(0, self.max_noise);
        }
        self.noise - before
    }

    /// Adds shield, e.g. from a played Block card. Shield has no upper cap of
    /// its own; only ever clamped so it can't go negative. Returns the actual
    /// amount added (mirrors `add`'s clamped-delta contract).
    pub fn add_shield(&mut self, amount: i32) -> i32 {
        let before = self.shield;
        self.shield = (self.shield + amount).max(0);
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
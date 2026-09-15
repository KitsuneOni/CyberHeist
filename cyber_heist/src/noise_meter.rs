use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoiseLevel {
    noise: i32,
    max_noise: i32,
}

impl NoiseLevel {
    pub fn new(max_noise: i32) -> Self {
        Self {
            noise: 0,
            max_noise: max_noise.max(0),
        }
    }

    pub fn noise(&self) -> i32 {
        self.noise
    }

    pub fn max_noise(&self) -> i32 {
        self.max_noise
    }

    pub fn add(&mut self, amount: i32) -> i32 {
        let before = self.noise;
        self.noise = self.noise.saturating_add(amount).clamp(0, self.max_noise);
        self.noise - before
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

    #[export]
    pub noise: i32,
    #[export]
    pub max_noise: i32,
}

#[godot_api]
impl INode for NoiseMeter {
    fn init(base: Base<Node>) -> Self {
        let level = NoiseLevel::new(100);
        Self {
            noise: level.noise(),
            max_noise: level.max_noise(),
            level,
            base,
        }
    }
}

#[godot_api]
impl NoiseMeter {
    #[func]
    pub fn add_noise(&mut self, amount: i32) -> i32 {
        let delta = self.level.add(amount);
        self.noise = self.level.noise();
        godot_print!("Noise meter: {}/{}", self.noise, self.max_noise);
        delta
    }

    #[func]
    pub fn is_at_cap(&self) -> bool {
        self.level.is_at_cap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_level_starts_silent() {
        let level = NoiseLevel::new(100);
        assert_eq!(level.noise(), 0);
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
}

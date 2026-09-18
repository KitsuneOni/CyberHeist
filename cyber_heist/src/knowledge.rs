//! Run-persistent knowledge: how much the player has learned about the
//! target, independent of any single encounter. Unlike noise or shield, this
//! is not reset when a combat screen loads — an event outside combat can
//! raise or lower it, and a later encounter should see the same value.
//!
//! Starts at 1, bounded to 0..=3. What knowledge unlocks (visibility into the
//! enemy) is separate, later work; this just owns the counter.

use godot::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Knowledge {
    level: i32,
    max_level: i32,
}

impl Knowledge {
    /// Starts at 1 (clamped into range, in case `max_level` is ever
    /// authored below that), capped at `max_level`.
    pub fn new(max_level: i32) -> Self {
        let max_level = max_level.max(0);
        Self {
            level: 1.clamp(0, max_level),
            max_level,
        }
    }

    pub fn level(&self) -> i32 {
        self.level
    }

    pub fn max_level(&self) -> i32 {
        self.max_level
    }

    /// Applies a signed change, clamped to `0..=max_level`. Returns the
    /// actual change, same clamped-delta contract as `NoiseLevel::add`.
    pub fn add(&mut self, amount: i32) -> i32 {
        let before = self.level;
        self.level = self.level.saturating_add(amount).clamp(0, self.max_level);
        self.level - before
    }
}

/// Godot-facing autoload wrapping [`Knowledge`].
///
/// The real data lives in `state`, not in a field called `knowledge` —
/// gdext's `#[var]` macro exposes a property under the exact name of the
/// field it's attached to, regardless of the `get = ...` function name. So
/// the phantom fields below are named `knowledge`/`max_knowledge` (giving
/// GDScript `KnowledgeMeterGlobal.knowledge` / `.max_knowledge`), and the
/// actual data field is named `state` to avoid colliding with them.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct KnowledgeMeter {
    base: Base<Node>,
    state: Knowledge,

    #[var(get = get_knowledge, no_set)]
    knowledge: PhantomVar<i32>,
    #[var(get = get_max_knowledge, no_set)]
    max_knowledge: PhantomVar<i32>,
}

#[godot_api]
impl INode for KnowledgeMeter {
    fn init(base: Base<Node>) -> Self {
        Self {
            state: Knowledge::new(3),
            knowledge: PhantomVar::default(),
            max_knowledge: PhantomVar::default(),
            base,
        }
    }
}

#[godot_api]
impl KnowledgeMeter {
    #[func]
    pub fn get_knowledge(&self) -> i32 {
        self.state.level()
    }

    #[func]
    pub fn get_max_knowledge(&self) -> i32 {
        self.state.max_level()
    }

    #[func]
    pub fn add_knowledge(&mut self, amount: i32) -> i32 {
        self.state.add(amount)
    }
}

impl KnowledgeMeter {
    /// The transaction borrows the authoritative value, never a copied mirror.
    pub(crate) fn level_mut(&mut self) -> &mut Knowledge {
        &mut self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_one() {
        let knowledge = Knowledge::new(3);
        assert_eq!(knowledge.level(), 1);
        assert_eq!(knowledge.max_level(), 3);
    }

    #[test]
    fn adding_moves_the_level_and_reports_the_change() {
        let mut knowledge = Knowledge::new(3);
        assert_eq!(knowledge.add(1), 1);
        assert_eq!(knowledge.level(), 2);
    }

    #[test]
    fn clamps_at_the_cap() {
        let mut knowledge = Knowledge::new(3);
        assert_eq!(knowledge.add(10), 2, "1 -> 3 is a delta of 2, not 10");
        assert_eq!(knowledge.level(), 3);
    }

    #[test]
    fn clamps_at_zero() {
        let mut knowledge = Knowledge::new(3);
        assert_eq!(knowledge.add(-10), -1, "1 -> 0 is a delta of -1, not -10");
        assert_eq!(knowledge.level(), 0);
    }

    #[test]
    fn a_zero_max_level_still_starts_at_zero_not_one() {
        let knowledge = Knowledge::new(0);
        assert_eq!(knowledge.level(), 0);
        assert_eq!(knowledge.max_level(), 0);
    }

    #[test]
    fn an_enormous_amount_does_not_overflow() {
        let mut knowledge = Knowledge::new(3);
        assert_eq!(knowledge.add(i32::MAX), 2);
        assert_eq!(knowledge.level(), 3);
        assert_eq!(knowledge.add(i32::MIN), -3);
        assert_eq!(knowledge.level(), 0);
    }
}

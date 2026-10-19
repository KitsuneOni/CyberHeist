#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerEffects {
    might: i32,
    corrupting_discount: i32,
}

impl PlayerEffects {
    /// Stacks of Might: each adds 1 to every Damage and Penetrating hit.
    pub fn might(&self) -> i32 {
        self.might
    }

    /// Adds Might, floored at 0. Returns the actual change.
    pub fn add_might(&mut self, amount: i32) -> i32 {
        let before = self.might;
        self.might = self.might.saturating_add(amount).max(0);
        self.might - before
    }
    /// Energy off every card that applies Corrupting.
    pub fn corrupting_discount(&self) -> i32 {
        self.corrupting_discount
    }

    /// Adds to the Corrupting discount, floored at 0. Returns the actual
    /// change.
    pub fn add_corrupting_discount(&mut self, amount: i32) -> i32 {
        let before = self.corrupting_discount;
        self.corrupting_discount = self.corrupting_discount.saturating_add(amount).max(0);
        self.corrupting_discount - before
    }
}

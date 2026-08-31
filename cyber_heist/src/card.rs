//! Card data. Matches the `Card`, `Rarity`, `Target_Type` and `Card_Type`
//! shapes from the class diagram on the Trello Documentation tab. Effects and
//! playing cards are a separate story - this only needs enough of a `Card` to
//! be drawn into a hand.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardType {
    Attack,
    Skill,
    Power,
    Curse,
    Status,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetType {
    None,
    SelfTarget,
    Enemy,
    AllEnemies,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub cost: i32,
    pub rarity: Rarity,
    pub target_type: TargetType,
    pub card_type: CardType,
    pub description: String,
    pub upgraded: bool,
}

impl Card {
    pub fn new(
        id: &str,
        name: &str,
        cost: i32,
        rarity: Rarity,
        target_type: TargetType,
        card_type: CardType,
        description: &str,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            cost,
            rarity,
            target_type,
            card_type,
            description: description.into(),
            upgraded: false,
        }
    }
}

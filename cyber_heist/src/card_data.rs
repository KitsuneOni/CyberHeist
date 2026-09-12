use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CardData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost: u8,
    pub noise_generated: i8,
    pub card_type: CardType,
    pub rarity: Rarity,
    pub keywords: Vec<Keyword>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum CardType{
    Attack,
    Defence,
    Skill,
    Recon,
    SocialEngineering
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum Rarity{
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum Keyword{
    Damage(u32),
    Block(u32),
    Penetrating(u32),
    Corrupting(u32),
    Intangible(u32),
    Knowledge(u32),
    Draw(u32),
    Exhaust,
    Flip,
}
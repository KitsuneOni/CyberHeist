use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct CardData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost: u8,
    pub noise_generated: i8,
    pub card_type: CardType,
    pub rarity: Rarity,
    pub keywords: Vec<Keyword>,

    #[serde(default)]
    pub weak_side: Option<WeakSide>,
    #[serde(default)]
    pub upgrade: Option<CardUpgrade>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum CardType {
    Attack,
    Defence,
    Skill,
    Recon,
    SocialEngineering,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WeakSide {
    pub noise_threshold: i32,
    #[serde(default)]
    pub name: Option<String>,
    pub description: String,
    pub keywords: Vec<Keyword>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CardUpgrade {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cost: Option<u8>,
    #[serde(default)]
    pub noise_generated: Option<i8>,
    #[serde(default)]
    pub keywords: Option<Vec<Keyword>>,
    #[serde(default)]
    pub weak_side: Option<WeakSide>,
}

/// Ordered from most to least common, so a reward can ask for a rarity "or
/// better" with a plain comparison.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum Keyword {
    Damage(i32),
    Block(i32),
    Penetrating(i32),
    Corrupting(i32),
    Intangible(i32),
    Knowledge(i32),
    Draw(i32),
    MaxEnergyBoost(u32),
    CorruptingBoost(u32),
    ResolveAtEndOfTurn,
    EnergyTax(u32),
    Exhaust,
    Flip,
}

#[allow(dead_code)]
impl CardData {
    pub fn is_flipped(&self, current_noise: i32) -> bool {
        self.weak_side
            .as_ref()
            .is_some_and(|w| current_noise >= w.noise_threshold)
    }

    pub fn active_keywords(&self, current_noise: i32) -> Vec<Keyword> {
        match &self.weak_side {
            Some(weak) if current_noise >= weak.noise_threshold => weak.keywords.clone(),
            _ => self.keywords.clone(),
        }
    }

    pub fn display_name(&self, current_noise: i32) -> String {
        match &self.weak_side {
            Some(weak) if current_noise >= weak.noise_threshold => weak
                .name
                .clone()
                .unwrap_or_else(|| format!("{} (Weak)", self.name)),
            _ => self.name.clone(),
        }
    }

    pub fn active_description(&self, current_noise: i32) -> &str {
        match &self.weak_side {
            Some(weak) if current_noise >= weak.noise_threshold => &weak.description,
            _ => &self.description,
        }
    }
    pub fn upgraded(&self) -> Option<CardData> {
        let upgrade = self.upgrade.as_ref()?;
        let mut card = self.clone();
        card.id = format!("{}+", self.id);
        card.name = upgrade
            .name
            .clone()
            .unwrap_or_else(|| format!("{}+", self.name));
        if let Some(description) = &upgrade.description {
            card.description = description.clone();
        }
        if let Some(cost) = upgrade.cost {
            card.cost = cost;
        }
        if let Some(noise) = upgrade.noise_generated {
            card.noise_generated = noise;
        }
        if let Some(keywords) = &upgrade.keywords {
            card.keywords = keywords.clone();
        }
        if let Some(weak_side) = &upgrade.weak_side {
            card.weak_side = Some(weak_side.clone());
        }
        card.upgrade = None;
        Some(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pretext_card(threshold: i32) -> CardData {
        CardData {
            id: "pretext".into(),
            name: "Confident Pretext".into(),
            description: "Convince the guard you belong here.".into(),
            cost: 1,
            noise_generated: 0,
            card_type: CardType::SocialEngineering,
            rarity: Rarity::Common,
            keywords: vec![Keyword::Knowledge(2)],
            upgrade: None,
            weak_side: Some(WeakSide {
                noise_threshold: threshold,
                name: Some("Nervous Excuse".into()),
                description: "You stammer through an excuse.".into(),
                keywords: vec![Keyword::Knowledge(1)],
            }),
        }
    }

    #[test]
    fn uses_strong_side_below_threshold() {
        let card = pretext_card(50);
        assert!(!card.is_flipped(10));
        assert_eq!(card.active_keywords(10), vec![Keyword::Knowledge(2)]);
        assert_eq!(card.display_name(10), "Confident Pretext");
    }

    #[test]
    fn flips_to_weak_side_at_threshold() {
        let card = pretext_card(50);
        assert!(card.is_flipped(50));
        assert_eq!(card.active_keywords(50), vec![Keyword::Knowledge(1)]);
        assert_eq!(card.display_name(50), "Nervous Excuse");
    }

    #[test]
    fn non_flipping_card_ignores_noise() {
        let mut card = pretext_card(50);
        card.weak_side = None;
        assert!(!card.is_flipped(999));
        assert_eq!(card.active_keywords(999), vec![Keyword::Knowledge(2)]);
    }
}

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

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum Keyword {
    Damage(u32),
    Block(u32),
    Penetrating(u32),
    Corrupting(i32),
    Intangible(u32),
    Knowledge(i32),
    Draw(u32),
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

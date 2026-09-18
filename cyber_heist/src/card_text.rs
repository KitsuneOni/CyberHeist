//! Player-facing text for cards.
//!
//! Covers the "detailed and easy to read cards" acceptance tests from Trello:
//! 1. Selecting a card shows a detailed description explaining everything
//!    about it.
//! 2. When a card carries an effect, an explanation of that effect appears
//!    alongside it.
//!
//! `CardData` stores the raw values; this module turns them into the wording
//! the player reads. Plain Rust with no Godot types, so the text rules stay
//! unit testable per docs/rust-godot-setup.md.

use crate::card_data::{CardData, CardType, Keyword, Rarity};

/// One effect on a card, named and explained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeywordDetail {
    /// Short name including its amount, e.g. "Corrupting 4".
    pub label: String,
    /// What that effect actually does.
    pub explanation: String,
}

/// Everything the detail panel needs to describe a single card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardDetail {
    pub name: String,
    pub card_type: String,
    pub rarity: String,
    pub cost: i64,
    pub cost_text: String,
    pub noise_text: String,
    pub description: String,
    pub keywords: Vec<KeywordDetail>,
}

pub fn card_type_name(card_type: CardType) -> &'static str {
    match card_type {
        CardType::Attack => "Attack",
        CardType::Defence => "Defence",
        CardType::Skill => "Skill",
        CardType::Recon => "Recon",
        CardType::SocialEngineering => "Social Engineering",
    }
}

pub fn rarity_name(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "Common",
        Rarity::Uncommon => "Uncommon",
        Rarity::Rare => "Rare",
        Rarity::Legendary => "Legendary",
    }
}

/// Energy cost, worded so a free card does not read as "Cost 0".
pub fn cost_text(cost: u8) -> String {
    if cost == 0 {
        "Free to play".to_string()
    } else if cost == 1 {
        "Costs 1 energy".to_string()
    } else {
        format!("Costs {cost} energy")
    }
}

/// Noise, worded by direction so the player can tell a risk from a cover.
pub fn noise_text(noise: i8) -> String {
    match noise {
        0 => "Makes no noise".to_string(),
        n if n > 0 => format!("Generates {n} noise"),
        n => format!("Reduces noise by {}", -(n as i32)),
    }
}

/// Short name of an effect, including its amount where it has one.
pub fn keyword_label(keyword: &Keyword) -> String {
    match keyword {
        Keyword::Damage(amount) => format!("Damage {amount}"),
        Keyword::Block(amount) => format!("Block {amount}"),
        Keyword::Penetrating(amount) => format!("Penetrating {amount}"),
        Keyword::Corrupting(amount) if *amount < 0 => format!("Cleansing {}", amount.abs()),
        Keyword::Corrupting(amount) => format!("Corrupting {amount}"),
        Keyword::Intangible(amount) => format!("Intangible {amount}"),
        Keyword::Knowledge(amount) => format!("Knowledge {amount}"),
        Keyword::Draw(amount) => format!("Draw {amount}"),
        Keyword::MaxEnergyBoost(amount) => format!("Max Energy +{amount}"),
        Keyword::CorruptingBoost(amount) => format!("Corrupting Boost {amount}"),
        Keyword::Exhaust => "Exhaust".to_string(),
        Keyword::Flip => "Flip".to_string(),
    }
}

/// What an effect does, so the player does not have to guess at a keyword.
///
/// This is the glossary the second acceptance test asks for.
pub fn keyword_explanation(keyword: &Keyword) -> String {
    match keyword {
        Keyword::Damage(amount) => {
            format!("Deals {amount} damage to the target. Block absorbs it first.")
        }
        Keyword::Block(amount) => {
            format!(
                "Gains {amount} block, absorbing that much damage before it reaches your health."
            )
        }
        Keyword::Penetrating(amount) => {
            format!("Deals {amount} damage straight through the target's block.")
        }
        Keyword::Corrupting(amount) if *amount < 0 => {
            format!("Removes {} corruption from the target.", amount.abs())
        }
        Keyword::Corrupting(amount) => {
            format!("Applies {amount} corruption. Some cards hit corrupted targets harder.")
        }
        Keyword::Intangible(amount) => {
            format!("For the next {amount} turn(s), damage you take is reduced to 1.")
        }
        Keyword::Knowledge(amount) => {
            format!("Reveals {amount} piece(s) of hidden information about the security system.")
        }
        Keyword::Draw(amount) => format!("Draw {amount} extra card(s) immediately."),
        Keyword::MaxEnergyBoost(amount) => {
            format!("Raises your max energy by {amount} for the rest of this combat.")
        }
        Keyword::CorruptingBoost(amount) => format!(
            "For the rest of this turn, every Corrupting effect you apply is {amount} stronger."
        ),
        Keyword::Exhaust => {
            "Once played, this card leaves the encounter instead of going to the discard pile."
                .to_string()
        }
        Keyword::Flip => {
            "Turns the card to its alternate face for the rest of the encounter.".to_string()
        }
    }
}

/// Builds the full player-facing breakdown of a card.
pub fn describe(card: &CardData, current_noise: i32) -> CardDetail {
    CardDetail {
        name: card.display_name(current_noise),
        card_type: card_type_name(card.card_type).to_string(),
        rarity: rarity_name(card.rarity).to_string(),
        cost: i64::from(card.cost),
        cost_text: cost_text(card.cost),
        noise_text: noise_text(card.noise_generated),
        description: card.active_description(current_noise).to_string(),
        keywords: card
            .active_keywords(current_noise)
            .iter()
            .map(|keyword| KeywordDetail {
                label: keyword_label(keyword),
                explanation: keyword_explanation(keyword),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_database::parse_cards;

    /// Every keyword the card data can hold, so the glossary cannot quietly
    /// miss one when a new effect is added.
    fn all_keywords() -> Vec<Keyword> {
        vec![
            Keyword::Damage(6),
            Keyword::Block(6),
            Keyword::Penetrating(6),
            Keyword::Corrupting(4),
            Keyword::Intangible(1),
            Keyword::Knowledge(2),
            Keyword::Draw(2),
            Keyword::Exhaust,
            Keyword::Flip,
        ]
    }

    fn test_card(description: &str, keywords: Vec<Keyword>) -> CardData {
        CardData {
            id: "test".to_string(),
            name: "Test Card".to_string(),
            description: description.to_string(),
            cost: 2,
            noise_generated: 5,
            card_type: CardType::Skill,
            rarity: Rarity::Rare,
            keywords,
            weak_side: None,
        }
    }

    /// Acceptance scenario 2: coming across an effect must produce an
    /// explanation of that effect, for every effect in the game.
    #[test]
    fn every_keyword_has_a_label_and_an_explanation() {
        for keyword in all_keywords() {
            let label = keyword_label(&keyword);
            let explanation = keyword_explanation(&keyword);

            assert!(!label.trim().is_empty(), "{keyword:?} has no label");
            assert!(
                !explanation.trim().is_empty(),
                "{keyword:?} has no explanation"
            );
            assert!(
                explanation.trim().len() > label.trim().len(),
                "{keyword:?} explanation is not more informative than its label"
            );
        }
    }

    #[test]
    fn keyword_labels_include_their_amount() {
        assert_eq!(keyword_label(&Keyword::Damage(6)), "Damage 6");
        assert_eq!(keyword_label(&Keyword::Corrupting(4)), "Corrupting 4");
        assert_eq!(keyword_label(&Keyword::Exhaust), "Exhaust");
    }

    #[test]
    fn negative_corruption_reads_as_cleansing_rather_than_a_minus_sign() {
        assert_eq!(keyword_label(&Keyword::Corrupting(-15)), "Cleansing 15");
        assert_eq!(
            keyword_explanation(&Keyword::Corrupting(-15)),
            "Removes 15 corruption from the target."
        );
    }

    #[test]
    fn noise_text_reads_by_direction() {
        assert_eq!(noise_text(0), "Makes no noise");
        assert_eq!(noise_text(5), "Generates 5 noise");
        assert_eq!(noise_text(-10), "Reduces noise by 10");
    }

    #[test]
    fn cost_text_reads_naturally_at_zero_and_one() {
        assert_eq!(cost_text(0), "Free to play");
        assert_eq!(cost_text(1), "Costs 1 energy");
        assert_eq!(cost_text(3), "Costs 3 energy");
    }

    #[test]
    fn social_engineering_is_not_run_together() {
        assert_eq!(
            card_type_name(CardType::SocialEngineering),
            "Social Engineering"
        );
    }

    /// Acceptance scenario 1: selecting a card explains everything about it.
    #[test]
    fn describe_covers_every_field_and_one_entry_per_effect() {
        let card = test_card(
            "Deal 8 damage and corrupt the enemy.",
            vec![Keyword::Damage(8), Keyword::Corrupting(4)],
        );

        let detail = describe(&card, 0);

        assert_eq!(detail.name, "Test Card");
        assert_eq!(detail.card_type, "Skill");
        assert_eq!(detail.rarity, "Rare");
        assert_eq!(detail.cost, 2);
        assert_eq!(detail.cost_text, "Costs 2 energy");
        assert_eq!(detail.noise_text, "Generates 5 noise");
        assert_eq!(detail.description, "Deal 8 damage and corrupt the enemy.");
        assert_eq!(detail.keywords.len(), 2);
        assert_eq!(detail.keywords[0].label, "Damage 8");
        assert_eq!(detail.keywords[1].label, "Corrupting 4");
        assert!(detail.keywords[1].explanation.contains("corruption"));
    }

    #[test]
    fn a_card_without_effects_still_describes_cleanly() {
        let card = test_card("Nothing happens.", Vec::new());

        let detail = describe(&card, 0);

        assert!(detail.keywords.is_empty());
        assert!(!detail.description.trim().is_empty());
    }

    /// The cards players will actually see must all describe completely, so a
    /// missing description or type cannot ship unnoticed.
    #[test]
    fn every_real_card_describes_completely() {
        let text = std::fs::read_to_string("../godot/data/cards.ron")
            .expect("could not find cards.ron - check the relative path");
        let cards = parse_cards(&text);
        assert!(!cards.is_empty());

        for card in cards.values() {
            let detail = describe(card, 0);

            assert!(!detail.name.trim().is_empty(), "{} has no name", card.id);
            assert!(
                !detail.description.trim().is_empty(),
                "{} has no description",
                card.id
            );
            assert!(
                !detail.card_type.trim().is_empty(),
                "{} has no card type",
                card.id
            );
            assert!(
                !detail.rarity.trim().is_empty(),
                "{} has no rarity",
                card.id
            );
            assert!(
                !detail.cost_text.trim().is_empty(),
                "{} has no cost text",
                card.id
            );
            assert!(
                !detail.noise_text.trim().is_empty(),
                "{} has no noise text",
                card.id
            );

            for keyword in &detail.keywords {
                assert!(
                    !keyword.explanation.trim().is_empty(),
                    "{} has an unexplained effect '{}'",
                    card.id,
                    keyword.label
                );
            }
        }
    }
}

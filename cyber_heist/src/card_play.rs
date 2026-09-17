use crate::card_data::{CardData, Keyword};
use crate::deck::Deck;
use crate::noise_meter::NoiseLevel;
use crate::sentry::Sentry;

#[derive(Debug, PartialEq, Eq)]
pub enum PlayError {
    Detected,
    InvalidIndex,
    NotEnoughEnergy,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PlayedCard {
    pub name: String,
    pub noise_change: i32,
    pub damage_dealt: i32,
    pub shield_added: i32,
}

fn total_damage(keywords: &[Keyword]) -> i32 {
    keywords
        .iter()
        .map(|keyword| match keyword {
            Keyword::Damage(amount) | Keyword::Penetrating(amount) => *amount,
            _ => 0,
        })
        .sum()
}

fn total_shield(keywords: &[Keyword]) -> i32 {
    keywords
        .iter()
        .map(|keyword| match keyword {
            Keyword::Block(amount) => *amount,
            _ => 0,
        })
        .sum()
}

pub fn play_card(
    hand: &mut Vec<CardData>,
    deck: &mut Deck,
    energy: &mut i32,
    noise: &mut NoiseLevel,
    sentry: &mut Sentry,
    index: i32,
) -> Result<PlayedCard, PlayError> {
    if noise.is_at_cap() {
        return Err(PlayError::Detected);
    }
    let index = usize::try_from(index).map_err(|_| PlayError::InvalidIndex)?;
    let card = hand.get(index).ok_or(PlayError::InvalidIndex)?;
    let cost = i32::from(card.cost);
    if cost > *energy {
        return Err(PlayError::NotEnoughEnergy);
    }

    let card = hand.remove(index);
    *energy -= cost;

    let pre_play_noise = noise.noise();
    let active_keywords = card.active_keywords(pre_play_noise);

    let shield_added = noise.add_shield(total_shield(&active_keywords));
    let damage_dealt = sentry.take_damage(total_damage(&active_keywords));

    let played = PlayedCard {
        name: card.display_name(pre_play_noise),
        noise_change: noise.add(i32::from(card.noise_generated)),
        damage_dealt,
        shield_added,
    };
    deck.discard(vec![card]);
    Ok(played)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_database::parse_cards;
    use rand::{SeedableRng, rngs::StdRng};

    struct Combat {
        hand: Vec<CardData>,
        deck: Deck,
        energy: i32,
        sentry: Sentry,
        noise: NoiseLevel,
    }

    impl Combat {
        fn with_card(id: &str, noise: i32) -> Self {
            let mut cards = parse_cards(include_str!("../../godot/data/cards.ron"));
            let mut level = NoiseLevel::new(100);
            level.add(noise);
            Self {
                hand: vec![cards.remove(id).expect("authored card exists")],
                deck: Deck::new(Vec::new(), &mut StdRng::seed_from_u64(4)),
                energy: 3,
                sentry: Sentry::warden_7(),
                noise: level,
            }
        }

        fn play(&mut self, index: i32) -> Result<PlayedCard, PlayError> {
            play_card(
                &mut self.hand,
                &mut self.deck,
                &mut self.energy,
                &mut self.noise,
                &mut self.sentry,
                index,
            )
        }
    }

    #[test]
    fn vpn_recovers_before_detection_without_advancing_the_intent() {
        let mut combat = Combat::with_card("vpn", 99);
        let intent = combat.sentry.queued_action().cloned();
        let played = combat.play(0).unwrap();

        assert_eq!(played.name, "VPN");
        assert_eq!(played.noise_change, -10);
        assert_eq!(played.damage_dealt, 0);
        assert_eq!(played.shield_added, 0);
        assert_eq!(combat.noise.noise(), 89);
        assert_eq!(combat.sentry.queued_action(), intent.as_ref());
        assert_eq!(combat.energy, 1);
        assert!(combat.hand.is_empty());
        assert_eq!(combat.deck.discard_pile_len(), 1);
        assert!(!combat.noise.is_at_cap());

        let action = combat.sentry.perform_queued_action().unwrap();
        assert_eq!(action.name, "Packet Sniff");
        assert_eq!(combat.noise.add(action.noise), 1);
        assert_eq!(combat.noise.noise(), 90);
        assert!(!combat.noise.is_at_cap());
    }

    #[test]
    fn recovery_clamps_at_zero_but_still_costs_energy_and_the_card() {
        for noise in [0, 1, 5] {
            let mut combat = Combat::with_card("vpn", noise);
            assert_eq!(combat.play(0).unwrap().noise_change, -noise);
            assert_eq!(combat.noise.noise(), 0);
            assert_eq!(combat.energy, 1);
            assert!(combat.hand.is_empty());
            assert_eq!(combat.deck.discard_pile_len(), 1);
            assert_eq!(combat.play(0), Err(PlayError::InvalidIndex));
            assert_eq!(combat.energy, 1);
            assert_eq!(combat.deck.discard_pile_len(), 1);
        }
    }

    #[test]
    fn rejected_plays_leave_all_state_untouched() {
        for (index, energy, noise, error) in [
            (-1, 3, 9, PlayError::InvalidIndex),
            (1, 3, 9, PlayError::InvalidIndex),
            (i32::MAX, 3, 9, PlayError::InvalidIndex),
            (0, 1, 9, PlayError::NotEnoughEnergy),
            (0, 3, 100, PlayError::Detected),
        ] {
            let mut combat = Combat::with_card("vpn", noise);
            combat.energy = energy;
            let sentry_before = combat.sentry.clone();
            let noise_before = combat.noise;

            assert_eq!(combat.play(index), Err(error));
            assert_eq!(combat.hand.len(), 1);
            assert_eq!(combat.hand[0].id, "vpn");
            assert_eq!(combat.energy, energy);
            assert_eq!(combat.deck.discard_pile_len(), 0);
            assert_eq!(combat.deck.draw_pile_len(), 0);
            assert_eq!(combat.sentry, sentry_before);
            assert_eq!(combat.noise, noise_before);
        }
    }

    #[test]
    fn signed_noise_is_applied_once_and_positive_noise_stops_at_the_cap() {
        for (id, before, after, delta) in [
            ("trojan", 1, 6, 5),
            ("trojan", 99, 100, 1),
            ("ransomware", 0, 15, 15),
            ("strike", 4, 4, 0),
        ] {
            let mut combat = Combat::with_card(id, before);
            let cost = i32::from(combat.hand[0].cost);
            assert_eq!(combat.play(0).unwrap().noise_change, delta);
            assert_eq!(combat.noise.noise(), after);
            assert_eq!(combat.noise.is_at_cap(), after == 100);
            assert_eq!(combat.energy, 3 - cost);
            assert_eq!(combat.deck.discard_pile_len(), 1);
        }
    }

    #[test]
    fn unaffordable_loud_card_does_not_trigger_detection() {
        let mut combat = Combat::with_card("ransomware", 99);
        combat.energy = 2;
        assert_eq!(combat.play(0), Err(PlayError::NotEnoughEnergy));
        assert_eq!(combat.noise.noise(), 99);
        assert!(!combat.noise.is_at_cap());
        assert_eq!(combat.energy, 2);
        assert_eq!(combat.hand[0].id, "ransomware");
        assert_eq!(combat.deck.discard_pile_len(), 0);
    }

    #[test]
    fn exact_energy_can_pay_for_recovery_and_the_card_remains_in_the_deck() {
        let mut combat = Combat::with_card("vpn", 99);
        combat.energy = 2;
        combat.play(0).unwrap();
        assert_eq!(combat.energy, 0);
        let redrawn = combat.deck.draw_hand(1, &mut StdRng::seed_from_u64(4));
        assert_eq!(redrawn[0].id, "vpn");
        assert_eq!(combat.deck.discard_pile_len(), 0);
    }

    #[test]
    fn social_play_reports_the_side_selected_before_its_own_noise_lands() {
        for (before, name) in [(49, "Nigerian King"), (50, "Nigerian Prince")] {
            let mut combat = Combat::with_card("nigerian_king", before);
            let played = combat.play(0).unwrap();
            assert_eq!(played.name, name);
            assert_eq!(played.noise_change, 15);
            assert_eq!(combat.noise.noise(), before + 15);
            assert_eq!(combat.energy, 2);
            assert_eq!(combat.deck.discard_pile_len(), 1);
        }
    }

    #[test]
    fn every_signed_card_noise_value_keeps_its_sign_and_clamps() {
        for noise in i8::MIN..=i8::MAX {
            let mut combat = Combat::with_card("vpn", 5);
            combat.hand[0].noise_generated = noise;
            let expected = (5 + i32::from(noise)).clamp(0, 100);
            assert_eq!(combat.play(0).unwrap().noise_change, expected - 5);
            assert_eq!(combat.noise.noise(), expected);
        }
    }

    #[test]
    fn strike_deals_its_authored_damage_and_it_lands_on_the_sentry() {
        let mut combat = Combat::with_card("strike", 0);
        let health_before = combat.sentry.health();
        let played = combat.play(0).unwrap();
        assert_eq!(played.damage_dealt, 6);
        assert_eq!(
            combat.sentry.health(),
            health_before - 6,
            "damage must actually be applied to the sentry, not just reported"
        );
    }

    #[test]
    fn non_damage_cards_report_zero_damage_and_leave_the_sentry_untouched() {
        for id in ["vpn", "shield", "background_check", "social_engineering"] {
            let mut combat = Combat::with_card(id, 0);
            let health_before = combat.sentry.health();
            assert_eq!(
                combat.play(0).unwrap().damage_dealt,
                0,
                "{id} should not deal damage"
            );
            assert_eq!(combat.sentry.health(), health_before);
        }
    }

    #[test]
    fn penetrating_damage_is_counted_the_same_as_plain_damage() {
        let mut combat = Combat::with_card("trojan", 0); // Penetrating(6)
        assert_eq!(combat.play(0).unwrap().damage_dealt, 6);
        assert_eq!(combat.sentry.health(), combat.sentry.max_health() - 6);
    }

    #[test]
    fn a_flipped_card_deals_its_weak_side_damage() {
        let mut combat = Combat::with_card("nigerian_king", 49);
        assert_eq!(combat.play(0).unwrap().damage_dealt, 18);

        let mut combat = Combat::with_card("nigerian_king", 50);
        assert_eq!(combat.play(0).unwrap().damage_dealt, 8);
    }

    #[test]
    fn multi_keyword_cards_sum_every_damage_keyword() {
        let mut combat = Combat::with_card("ransomware", 0);
        assert_eq!(combat.play(0).unwrap().damage_dealt, 10);
    }

    #[test]
    fn damage_clamps_to_the_sentrys_remaining_health_and_reports_the_actual_amount() {
        let mut combat = Combat::with_card("wannacry", 0); // Penetrating(30)
        combat.sentry = Sentry::warden_7().with_health(10);
        let played = combat.play(0).unwrap();
        assert_eq!(played.damage_dealt, 10, "clamped, not the authored 30");
        assert_eq!(combat.sentry.health(), 0);
        assert!(combat.sentry.is_defeated());
    }

    #[test]
    fn damage_against_an_already_defeated_sentry_reports_zero() {
        let mut combat = Combat::with_card("strike", 0);
        combat.sentry = Sentry::warden_7().with_health(0);
        assert!(combat.sentry.is_defeated());
        let played = combat.play(0).unwrap();
        assert_eq!(played.damage_dealt, 0, "nothing left to lose");
        assert_eq!(combat.sentry.health(), 0);
    }

    #[test]
    fn shield_deals_its_authored_amount_and_blocks_noise() {
        let mut combat = Combat::with_card("shield", 0); // Block(6), noise_generated: 0
        let played = combat.play(0).unwrap();
        assert_eq!(played.shield_added, 6);
        assert_eq!(combat.noise.shield(), 6);
    }

    #[test]
    fn shield_added_by_a_card_blocks_that_same_cards_own_noise() {
        let mut combat = Combat::with_card("burner_phone", 20);
        let played = combat.play(0).unwrap();
        assert_eq!(played.shield_added, 8);
        assert_eq!(played.noise_change, -10);
        assert_eq!(combat.noise.noise(), 10);
        assert_eq!(
            combat.noise.add(5),
            0,
            "the 8 shield from this play blocks a later 5 noise hit"
        );
    }

    #[test]
    fn non_shield_cards_report_zero_shield_added() {
        for id in ["strike", "vpn", "background_check"] {
            let mut combat = Combat::with_card(id, 0);
            assert_eq!(
                combat.play(0).unwrap().shield_added,
                0,
                "{id} should not grant shield"
            );
        }
    }

    #[test]
    fn a_play_that_both_shields_and_damages_applies_both_in_the_same_transaction() {
        let mut cards = parse_cards(include_str!("../../godot/data/cards.ron"));
        let mut hybrid = cards.remove("strike").expect("strike exists");
        hybrid.keywords = vec![Keyword::Damage(6), Keyword::Block(4)];

        let mut combat = Combat::with_card("strike", 0);
        combat.hand[0] = hybrid;
        let health_before = combat.sentry.health();

        let played = combat.play(0).unwrap();
        assert_eq!(played.damage_dealt, 6);
        assert_eq!(played.shield_added, 4);
        assert_eq!(combat.sentry.health(), health_before - 6);
        assert_eq!(combat.noise.shield(), 4);
    }
}

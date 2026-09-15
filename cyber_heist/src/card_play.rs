//! One paid card play, kept independent of Godot so rejection is testable.
//! Only the card's signed noise is resolved here; keywords are separate work.

use crate::card_data::CardData;
use crate::deck::Deck;
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
    /// Actual signed change after clamping, not the authored amount.
    pub noise_change: i32,
}

/// Reject before mutating anything. A full meter is already a failed encounter,
/// not an opportunity to play a recovery card.
pub fn play_card(
    hand: &mut Vec<CardData>,
    deck: &mut Deck,
    energy: &mut i32,
    sentry: &mut Sentry,
    index: i32,
) -> Result<PlayedCard, PlayError> {
    if sentry.is_detected() {
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
    let played = PlayedCard {
        name: card.name.clone(),
        noise_change: sentry.add_noise(i32::from(card.noise_generated)),
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
    }

    impl Combat {
        fn with_card(id: &str, noise: i32) -> Self {
            let mut cards = parse_cards(include_str!("../../godot/data/cards.ron"));
            let mut sentry = Sentry::warden_7();
            sentry.add_noise(noise);
            Self {
                hand: vec![cards.remove(id).expect("authored card exists")],
                deck: Deck::new(Vec::new(), &mut StdRng::seed_from_u64(4)),
                energy: 3,
                sentry,
            }
        }

        fn play(&mut self, index: i32) -> Result<PlayedCard, PlayError> {
            play_card(
                &mut self.hand,
                &mut self.deck,
                &mut self.energy,
                &mut self.sentry,
                index,
            )
        }
    }

    #[test]
    fn vpn_recovers_before_detection_without_advancing_the_intent() {
        let mut combat = Combat::with_card("vpn", 9);
        let intent = combat.sentry.queued_action().cloned();
        let played = combat.play(0).unwrap();

        assert_eq!(played.name, "VPN");
        assert_eq!(played.noise_change, -9);
        assert_eq!(combat.sentry.noise(), 0);
        assert_eq!(combat.sentry.queued_action(), intent.as_ref());
        assert_eq!(combat.energy, 1);
        assert!(combat.hand.is_empty());
        assert_eq!(combat.deck.discard_pile_len(), 1);
        assert!(!combat.sentry.is_detected());

        let turn = combat.sentry.take_turn().unwrap();
        assert_eq!(turn.action_name, "Packet Sniff");
        assert_eq!(turn.noise, 1);
        assert!(!turn.run_failed);
    }

    #[test]
    fn recovery_clamps_at_zero_but_still_costs_energy_and_the_card() {
        for noise in [0, 1, 5] {
            let mut combat = Combat::with_card("vpn", noise);
            assert_eq!(combat.play(0).unwrap().noise_change, -noise);
            assert_eq!(combat.sentry.noise(), 0);
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
            (0, 3, 10, PlayError::Detected),
        ] {
            let mut combat = Combat::with_card("vpn", noise);
            combat.energy = energy;
            let sentry_before = combat.sentry.clone();

            assert_eq!(combat.play(index), Err(error));
            assert_eq!(combat.hand.len(), 1);
            assert_eq!(combat.hand[0].id, "vpn");
            assert_eq!(combat.energy, energy);
            assert_eq!(combat.deck.discard_pile_len(), 0);
            assert_eq!(combat.deck.draw_pile_len(), 0);
            assert_eq!(combat.sentry, sentry_before);
        }
    }

    #[test]
    fn signed_noise_is_applied_once_and_positive_noise_stops_at_the_cap() {
        for (id, before, after, delta) in [
            ("trojan", 1, 6, 5),
            ("trojan", 9, 10, 1),
            ("ransomware", 0, 10, 10),
            ("strike", 4, 4, 0),
        ] {
            let mut combat = Combat::with_card(id, before);
            let cost = i32::from(combat.hand[0].cost);
            assert_eq!(combat.play(0).unwrap().noise_change, delta);
            assert_eq!(combat.sentry.noise(), after);
            assert_eq!(combat.sentry.is_detected(), after == 10);
            assert_eq!(combat.energy, 3 - cost);
            assert_eq!(combat.deck.discard_pile_len(), 1);
        }
    }

    #[test]
    fn unaffordable_loud_card_does_not_trigger_detection() {
        let mut combat = Combat::with_card("ransomware", 9);
        combat.energy = 2;
        assert_eq!(combat.play(0), Err(PlayError::NotEnoughEnergy));
        assert_eq!(combat.sentry.noise(), 9);
        assert!(!combat.sentry.is_detected());
        assert_eq!(combat.energy, 2);
        assert_eq!(combat.hand[0].id, "ransomware");
        assert_eq!(combat.deck.discard_pile_len(), 0);
    }

    #[test]
    fn exact_energy_can_pay_for_recovery_and_the_card_remains_in_the_deck() {
        let mut combat = Combat::with_card("vpn", 9);
        combat.energy = 2;
        combat.play(0).unwrap();
        assert_eq!(combat.energy, 0);
        let redrawn = combat.deck.draw_hand(1, &mut StdRng::seed_from_u64(4));
        assert_eq!(redrawn[0].id, "vpn");
        assert_eq!(combat.deck.discard_pile_len(), 0);
    }

    #[test]
    fn every_signed_card_noise_value_keeps_its_sign_and_clamps() {
        for noise in i8::MIN..=i8::MAX {
            let mut combat = Combat::with_card("vpn", 5);
            combat.hand[0].noise_generated = noise;
            let expected = (5 + i32::from(noise)).clamp(0, 10);
            assert_eq!(combat.play(0).unwrap().noise_change, expected - 5);
            assert_eq!(combat.sentry.noise(), expected);
        }
    }
}

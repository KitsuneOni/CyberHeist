//! Random event encounters offered by `EncounterType::Event` nodes.
//!
//! Covers the "random event nodes on the contract map" acceptance tests from
//! Trello:
//! 1. An event presents at least two choices, each stating its potential
//!    outcome or risk before the player commits.
//! 2. Confirming a choice resolves it into an effect the caller applies to
//!    the run state.
//!
//! Plain Rust with no Godot types, so the rules stay unit testable per
//! docs/rust-godot-setup.md. `events_node.rs` adapts this for scenes.

use rand::Rng;

/// What resolving a choice does to the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEffect {
    /// Credits gained (positive) or lost (negative).
    pub credits: i64,
    /// Upgrade earned for this contract, if any.
    pub upgrade: Option<String>,
    /// Player-facing description of what actually happened.
    pub summary: String,
}

impl EventEffect {
    pub fn new(credits: i64, upgrade: Option<&str>, summary: &str) -> Self {
        Self {
            credits,
            upgrade: upgrade.map(str::to_string),
            summary: summary.to_string(),
        }
    }
}

/// How a choice resolves once confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventOutcome {
    /// Always resolves the same way.
    Certain(EventEffect),
    /// A gamble. `success_chance` is a percentage in 1..=99, so both results
    /// stay genuinely reachable and the choice carries real risk.
    Gamble {
        success_chance: u8,
        success: EventEffect,
        failure: EventEffect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventChoice {
    /// Button text.
    pub label: String,
    /// Shown before committing so the player can weigh risk against reward.
    pub preview: String,
    pub outcome: EventOutcome,
}

impl EventChoice {
    pub fn certain(label: &str, preview: &str, effect: EventEffect) -> Self {
        Self {
            label: label.to_string(),
            preview: preview.to_string(),
            outcome: EventOutcome::Certain(effect),
        }
    }

    pub fn gamble(
        label: &str,
        preview: &str,
        success_chance: u8,
        success: EventEffect,
        failure: EventEffect,
    ) -> Self {
        Self {
            label: label.to_string(),
            preview: preview.to_string(),
            outcome: EventOutcome::Gamble {
                success_chance,
                success,
                failure,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDefinition {
    pub id: String,
    pub title: String,
    pub description: String,
    pub choices: Vec<EventChoice>,
}

impl EventDefinition {
    /// Resolves the choice at `index`, rolling for it if it is a gamble.
    ///
    /// Returns `None` for an out-of-range index, so a bad call from the UI
    /// cannot silently apply somebody else's outcome.
    pub fn resolve(&self, index: usize, rng: &mut impl Rng) -> Option<EventEffect> {
        let choice = self.choices.get(index)?;

        let effect = match &choice.outcome {
            EventOutcome::Certain(effect) => effect.clone(),
            EventOutcome::Gamble {
                success_chance,
                success,
                failure,
            } => {
                let roll = rng.random_range(1..=100u8);
                if roll <= *success_chance {
                    success.clone()
                } else {
                    failure.clone()
                }
            }
        };

        Some(effect)
    }
}

/// Every event that can appear on a contract.
pub fn event_catalogue() -> Vec<EventDefinition> {
    vec![
        EventDefinition {
            id: "unsecured_terminal".to_string(),
            title: "Unsecured Terminal".to_string(),
            description: "A maintenance terminal three floors down is still logged in. \
                 Someone will notice eventually."
                .to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Siphon the credit cache",
                    "60% chance: gain 120 credits. Otherwise the transfer is flagged and costs you 80.",
                    60,
                    EventEffect::new(
                        120,
                        None,
                        "The transfer clears before anyone looks twice. +120 credits.",
                    ),
                    EventEffect::new(
                        -80,
                        None,
                        "The transfer trips an audit flag and the clawback stings. -80 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Wipe your traces and move on",
                    "Safe. Nothing gained, nothing lost.",
                    EventEffect::new(0, None, "You scrub the logs and slip away clean."),
                ),
            ],
        },
        EventDefinition {
            id: "black_market_broker".to_string(),
            title: "Black Market Broker".to_string(),
            description:
                "A broker on a rooftop offers you an exploit kit. She does not do refunds."
                    .to_string(),
            choices: vec![
                EventChoice::certain(
                    "Pay her asking price",
                    "Costs 100 credits. You gain the Exploit Kit upgrade.",
                    EventEffect::new(
                        -100,
                        Some("Exploit Kit"),
                        "She hands over the kit without a word. -100 credits, Exploit Kit acquired.",
                    ),
                ),
                EventChoice::gamble(
                    "Haggle",
                    "50% chance: the kit for only 40 credits. Otherwise she walks and you get nothing.",
                    50,
                    EventEffect::new(
                        -40,
                        Some("Exploit Kit"),
                        "She laughs, then takes the lowball. -40 credits, Exploit Kit acquired.",
                    ),
                    EventEffect::new(
                        0,
                        None,
                        "She pockets the kit and is gone down the fire escape.",
                    ),
                ),
                EventChoice::certain(
                    "Walk away",
                    "Safe. Nothing gained, nothing lost.",
                    EventEffect::new(0, None, "You decide the heat is not worth it."),
                ),
            ],
        },
        EventDefinition {
            id: "abandoned_data_cache".to_string(),
            title: "Abandoned Data Cache".to_string(),
            description: "An encrypted cache nobody has claimed, humming quietly in a dead node."
                .to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Crack the encryption",
                    "75% chance: gain 90 credits. Otherwise it self-wipes and the trace costs you 30.",
                    75,
                    EventEffect::new(90, None, "The cache cracks open. +90 credits."),
                    EventEffect::new(
                        -30,
                        None,
                        "The cache self-wipes and fires a trace back at you. -30 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Sell the location",
                    "Guaranteed 45 credits.",
                    EventEffect::new(
                        45,
                        None,
                        "You sell the coordinates to a fixer. +45 credits.",
                    ),
                ),
            ],
        },
    ]
}

/// Picks one event at random for an event node.
pub fn random_event(rng: &mut impl Rng) -> EventDefinition {
    let mut catalogue = event_catalogue();
    let index = rng.random_range(0..catalogue.len());
    catalogue.swap_remove(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Acceptance scenario 1: every event must offer a real choice, and every
    /// option must say what it might do before the player commits.
    #[test]
    fn every_event_offers_at_least_two_previewed_choices() {
        for event in event_catalogue() {
            assert!(
                event.choices.len() >= 2,
                "event '{}' only offers {} choice(s)",
                event.id,
                event.choices.len()
            );

            for choice in &event.choices {
                assert!(
                    !choice.label.trim().is_empty(),
                    "a choice in event '{}' has no label",
                    event.id
                );
                assert!(
                    !choice.preview.trim().is_empty(),
                    "choice '{}' in event '{}' does not tell the player its outcome or risk",
                    choice.label,
                    event.id
                );
            }
        }
    }

    /// A "risk" that can only land one way is not a risk, so both sides of a
    /// gamble have to stay reachable.
    #[test]
    fn gamble_chances_leave_both_outcomes_reachable() {
        for event in event_catalogue() {
            for choice in &event.choices {
                if let EventOutcome::Gamble { success_chance, .. } = &choice.outcome {
                    assert!(
                        (1..=99).contains(success_chance),
                        "choice '{}' in event '{}' has a {}% chance, which is not a gamble",
                        choice.label,
                        event.id,
                        success_chance
                    );
                }
            }
        }
    }

    #[test]
    fn event_ids_are_unique() {
        let catalogue = event_catalogue();
        let mut ids: Vec<&str> = catalogue.iter().map(|e| e.id.as_str()).collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        assert_eq!(unique, ids.len(), "duplicate event id in the catalogue");
    }

    fn gamble_event(success_chance: u8) -> EventDefinition {
        EventDefinition {
            id: "test_event".to_string(),
            title: "Test Event".to_string(),
            description: "A test".to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Take the risk",
                    "Might pay off",
                    success_chance,
                    EventEffect::new(50, Some("Lucky Charm"), "It worked"),
                    EventEffect::new(-20, None, "It did not work"),
                ),
                EventChoice::certain(
                    "Play it safe",
                    "Nothing happens",
                    EventEffect::new(0, None, "You walk away"),
                ),
            ],
        }
    }

    #[test]
    fn a_certain_choice_always_resolves_to_its_effect() {
        let event = gamble_event(50);
        let mut rng = StdRng::seed_from_u64(11);

        for _ in 0..20 {
            let effect = event.resolve(1, &mut rng).expect("choice 1 exists");
            assert_eq!(effect, EventEffect::new(0, None, "You walk away"));
        }
    }

    #[test]
    fn a_gamble_can_land_on_either_side() {
        let always_wins = gamble_event(99);
        let always_loses = gamble_event(1);
        let mut rng = StdRng::seed_from_u64(7);

        let mut wins = 0;
        let mut losses = 0;
        for _ in 0..200 {
            if always_wins.resolve(0, &mut rng).unwrap().credits == 50 {
                wins += 1;
            }
            if always_loses.resolve(0, &mut rng).unwrap().credits == -20 {
                losses += 1;
            }
        }

        assert!(
            wins > 150,
            "a 99% chance should mostly succeed, got {wins}/200"
        );
        assert!(
            losses > 150,
            "a 1% chance should mostly fail, got {losses}/200"
        );
    }

    #[test]
    fn a_winning_gamble_carries_its_upgrade_and_credits() {
        let event = gamble_event(99);
        let mut rng = StdRng::seed_from_u64(3);

        let effect = event.resolve(0, &mut rng).expect("choice 0 exists");

        assert_eq!(effect.credits, 50);
        assert_eq!(effect.upgrade.as_deref(), Some("Lucky Charm"));
        assert_eq!(effect.summary, "It worked");
    }

    #[test]
    fn an_out_of_range_choice_resolves_to_nothing() {
        let event = gamble_event(50);
        let mut rng = StdRng::seed_from_u64(1);

        assert_eq!(event.resolve(2, &mut rng), None);
        assert_eq!(event.resolve(99, &mut rng), None);
    }

    #[test]
    fn random_event_returns_a_catalogue_entry() {
        let catalogue_ids: Vec<String> = event_catalogue().into_iter().map(|e| e.id).collect();
        let mut rng = StdRng::seed_from_u64(21);

        for _ in 0..30 {
            let event = random_event(&mut rng);
            assert!(
                catalogue_ids.contains(&event.id),
                "random_event returned unknown id '{}'",
                event.id
            );
        }
    }
}

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
        EventDefinition {
            id: "honeypot_server".to_string(),
            title: "Honeypot Server".to_string(),
            description: "A payroll server sits wide open with no firewall in front of it. \
                 It looks almost too easy."
                .to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Raid the payroll data",
                    "40% chance: gain 150 credits. Otherwise it is a honeypot and the trace costs you 60.",
                    40,
                    EventEffect::new(
                        150,
                        None,
                        "The server is exactly what it looks like. +150 credits.",
                    ),
                    EventEffect::new(
                        -60,
                        None,
                        "The honeypot snaps shut and you pay to burn the trace. -60 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Tip off a fixer",
                    "Guaranteed 30 credits for the lead.",
                    EventEffect::new(
                        30,
                        None,
                        "Let someone else take the risk. The fixer pays for the lead. +30 credits.",
                    ),
                ),
            ],
        },
        EventDefinition {
            id: "stranded_courier".to_string(),
            title: "Stranded Courier".to_string(),
            description: "A data courier's drop fell through. She wants the drive off her \
                 hands before her client comes looking."
                .to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Buy the drive for 50",
                    "Costs 50 credits. 55% chance the data resells for 150, for a net gain of 100.",
                    55,
                    EventEffect::new(
                        100,
                        None,
                        "The drive is full of client records that resell fast. +100 credits after her cut.",
                    ),
                    EventEffect::new(
                        -50,
                        None,
                        "The drive was wiped before she got it. -50 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Point her to a safe drop",
                    "Safe. Nothing gained, nothing lost.",
                    EventEffect::new(0, None, "She nods and vanishes into the crowd."),
                ),
            ],
        },
        EventDefinition {
            id: "leaked_wallet_keys".to_string(),
            title: "Leaked Wallet Keys".to_string(),
            description: "A paste site is leaking crypto wallet keys faster than \
                 anyone can sweep them."
                .to_string(),
            choices: vec![
                EventChoice::gamble(
                    "Sweep the big wallets",
                    "30% chance: gain 200 credits. Otherwise someone beats you to them and the fees cost you 40.",
                    30,
                    EventEffect::new(
                        200,
                        None,
                        "You empty the fat wallets before anyone else wakes up. +200 credits.",
                    ),
                    EventEffect::new(
                        -40,
                        None,
                        "Every wallet is already empty, and the transaction fees still cost you. -40 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Skim the small change",
                    "Guaranteed 25 credits.",
                    EventEffect::new(
                        25,
                        None,
                        "Nobody misses a few coins from the small wallets. +25 credits.",
                    ),
                ),
            ],
        },
        EventDefinition {
            id: "rival_crew".to_string(),
            title: "Rival Crew".to_string(),
            description: "Another crew is working the same network. Their lead offers to \
                 split what you both find."
                .to_string(),
            choices: vec![
                EventChoice::certain(
                    "Split the take",
                    "Guaranteed 60 credits.",
                    EventEffect::new(
                        60,
                        None,
                        "They keep their word, which is rare in this business. +60 credits.",
                    ),
                ),
                EventChoice::gamble(
                    "Double-cross them",
                    "45% chance: gain 140 credits. Otherwise they see it coming and it costs you 90.",
                    45,
                    EventEffect::new(
                        140,
                        None,
                        "You lock them out and take the whole haul. +140 credits.",
                    ),
                    EventEffect::new(
                        -90,
                        None,
                        "They were ready for you and drain your account on the way out. -90 credits.",
                    ),
                ),
                EventChoice::certain(
                    "Decline and keep moving",
                    "Safe. Nothing gained, nothing lost.",
                    EventEffect::new(0, None, "You leave them to it."),
                ),
            ],
        },
        EventDefinition {
            id: "bug_bounty".to_string(),
            title: "Bug Bounty".to_string(),
            description: "You stumble on a zero-day in the target's login portal. \
                 Somebody will pay for it."
                .to_string(),
            choices: vec![
                EventChoice::certain(
                    "Claim the corporate bounty",
                    "Guaranteed 50 credits.",
                    EventEffect::new(
                        50,
                        None,
                        "The bounty clears through a shell account. +50 credits.",
                    ),
                ),
                EventChoice::gamble(
                    "Sell it on the dark web",
                    "55% chance: gain 130 credits. Otherwise the buyer is a sting and you pay 50 to get clear.",
                    55,
                    EventEffect::new(
                        130,
                        None,
                        "A broker pays top price for the zero-day. +130 credits.",
                    ),
                    EventEffect::new(
                        -50,
                        None,
                        "The buyer was a sting, and you pay 50 credits to get clear.",
                    ),
                ),
            ],
        },
    ]
}

/// Picks one event at random for an event node, skipping any whose id is in
/// `seen` so that a run does not show the same event twice.
///
/// Once a run has seen every event, it picks from the whole catalogue again,
/// so an event node always gets an event.
pub fn random_event(seen: &[String], rng: &mut impl Rng) -> EventDefinition {
    let mut pool: Vec<EventDefinition> = event_catalogue()
        .into_iter()
        .filter(|event| !seen.contains(&event.id))
        .collect();
    if pool.is_empty() {
        pool = event_catalogue();
    }
    let index = rng.random_range(0..pool.len());
    pool.swap_remove(index)
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
            let event = random_event(&[], &mut rng);
            assert!(
                catalogue_ids.contains(&event.id),
                "random_event returned unknown id '{}'",
                event.id
            );
        }
    }

    /// Story #116, scenario 1: the event pool is big enough to stay
    /// surprising across several runs.
    #[test]
    fn the_catalogue_has_at_least_eight_events() {
        let count = event_catalogue().len();
        assert!(count >= 8, "only {count} events in the catalogue");
    }

    /// Story #116, scenario 2: an event already seen this run is not rolled
    /// again while there are others left.
    #[test]
    fn a_seen_event_is_not_rolled_again() {
        let mut seen: Vec<String> = Vec::new();
        let mut rng = StdRng::seed_from_u64(5);

        for _ in 0..event_catalogue().len() {
            let event = random_event(&seen, &mut rng);
            assert!(
                !seen.contains(&event.id),
                "'{}' was rolled again after being seen",
                event.id
            );
            seen.push(event.id);
        }
    }

    /// With every event seen, the roll falls back to the whole catalogue, so
    /// an event node is never left empty.
    #[test]
    fn a_run_that_has_seen_everything_still_gets_an_event() {
        let seen: Vec<String> = event_catalogue().into_iter().map(|e| e.id).collect();
        let mut rng = StdRng::seed_from_u64(9);

        let event = random_event(&seen, &mut rng);

        assert!(seen.contains(&event.id));
    }

    /// The previews quote the numbers, so they have to agree with what the
    /// choice actually does.
    #[test]
    fn previews_quote_the_credits_and_chances_they_resolve_to() {
        for event in event_catalogue() {
            for choice in &event.choices {
                let mut quoted: Vec<i64> = Vec::new();
                match &choice.outcome {
                    EventOutcome::Certain(effect) => quoted.push(effect.credits.abs()),
                    EventOutcome::Gamble {
                        success_chance,
                        success,
                        failure,
                    } => {
                        quoted.push(i64::from(*success_chance));
                        quoted.push(success.credits.abs());
                        quoted.push(failure.credits.abs());
                    }
                }
                for amount in quoted.into_iter().filter(|amount| *amount != 0) {
                    assert!(
                        choice.preview.contains(&amount.to_string()),
                        "choice '{}' in event '{}' resolves to {} but its preview does not say so",
                        choice.label,
                        event.id,
                        amount
                    );
                }
            }
        }
    }
}

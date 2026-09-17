//! Player-facing text for map nodes.
//!
//! Covers the "types of encounters clearly defined" acceptance tests from
//! Trello:
//! 1. Nodes on the map are marked so the player can tell what kind of
//!    encounter each one is.
//! 2. A key is available explaining what each marker means.
//!
//! The markers here are short text tags standing in for real icon art, which
//! does not exist yet (`godot/assets/` is empty). Swapping them for sprites
//! later only touches the map screen, not this table.
//!
//! Plain Rust with no Godot types so the wording stays unit testable, in the
//! same spirit as `card_text.rs`.

use crate::contract_map::{EncounterType, NodeStatus};

/// How one encounter type is presented on the map and in the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncounterTypeText {
    /// Short marker shown on the node itself.
    pub marker: &'static str,
    /// Name of the encounter type.
    pub name: &'static str,
    /// What the player should expect from it.
    pub description: &'static str,
}

pub fn encounter_type_text(encounter_type: EncounterType) -> EncounterTypeText {
    match encounter_type {
        EncounterType::Entry => EncounterTypeText {
            // Not [>]: that belongs to the "you can go here next" status, and
            // one glyph cannot mean two things in the same key.
            marker: "[^]",
            name: "Entry",
            description: "Where the contract begins. Nothing to fight here.",
        },
        EncounterType::Combat => EncounterTypeText {
            marker: "[C]",
            name: "Combat",
            description: "A security system to break through using your cards.",
        },
        EncounterType::Event => EncounterTypeText {
            marker: "[E]",
            name: "Event",
            description: "A choice with its own risk and reward. No fighting.",
        },
        EncounterType::Shop => EncounterTypeText {
            marker: "[$]",
            name: "Shop",
            description: "Spend credits on new hardware between jobs.",
        },
        EncounterType::Elite => EncounterTypeText {
            marker: "[!]",
            name: "Elite",
            description: "A far tougher security system, with a better payout.",
        },
        EncounterType::Boss => EncounterTypeText {
            marker: "[#]",
            name: "Boss",
            description: "The system guarding this zone. Beat it to open the next one.",
        },
    }
}

/// Every encounter type, for building the map key.
pub fn all_encounter_types() -> [EncounterType; 6] {
    [
        EncounterType::Entry,
        EncounterType::Combat,
        EncounterType::Event,
        EncounterType::Shop,
        EncounterType::Elite,
        EncounterType::Boss,
    ]
}

/// How one node status is presented on the map and in the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeStatusText {
    /// Short marker shown beside the node.
    pub marker: &'static str,
    /// What that marker means.
    pub description: &'static str,
}

pub fn node_status_text(status: NodeStatus) -> NodeStatusText {
    match status {
        NodeStatus::Completed => NodeStatusText {
            marker: "[x]",
            description: "Completed.",
        },
        NodeStatus::Current => NodeStatusText {
            marker: "[@]",
            description: "Where you are now.",
        },
        NodeStatus::Available => NodeStatusText {
            marker: "[>]",
            description: "You can go here next.",
        },
        NodeStatus::Locked => NodeStatusText {
            marker: "[-]",
            description: "Locked out by an earlier choice.",
        },
        NodeStatus::Sealed => NodeStatusText {
            marker: "[=]",
            description: "Sealed until this zone's boss is beaten.",
        },
        NodeStatus::Upcoming => NodeStatusText {
            marker: "[ ]",
            description: "Further ahead on the contract.",
        },
    }
}

/// Every node status, for building the map key.
pub fn all_node_statuses() -> [NodeStatus; 6] {
    [
        NodeStatus::Completed,
        NodeStatus::Current,
        NodeStatus::Available,
        NodeStatus::Locked,
        NodeStatus::Sealed,
        NodeStatus::Upcoming,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Acceptance scenario 2: the key must be able to explain every marker the
    /// player can see, so none of them can be left undefined.
    #[test]
    fn every_encounter_type_has_a_marker_name_and_description() {
        for encounter_type in all_encounter_types() {
            let text = encounter_type_text(encounter_type);

            assert!(
                !text.marker.trim().is_empty(),
                "{encounter_type:?} has no marker"
            );
            assert!(
                !text.name.trim().is_empty(),
                "{encounter_type:?} has no name"
            );
            assert!(
                text.description.trim().len() > text.name.trim().len(),
                "{encounter_type:?} description is not more informative than its name"
            );
        }
    }

    /// Acceptance scenario 1: two encounter types must not look the same, or
    /// the player cannot tell what is ahead.
    #[test]
    fn encounter_type_markers_are_distinct() {
        let markers: Vec<&str> = all_encounter_types()
            .into_iter()
            .map(|t| encounter_type_text(t).marker)
            .collect();

        let mut unique = markers.clone();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(
            unique.len(),
            markers.len(),
            "two encounter types share a marker: {markers:?}"
        );
    }

    #[test]
    fn every_node_status_has_a_marker_and_description() {
        for status in all_node_statuses() {
            let text = node_status_text(status);

            assert!(!text.marker.trim().is_empty(), "{status:?} has no marker");
            assert!(
                !text.description.trim().is_empty(),
                "{status:?} has no description"
            );
        }
    }

    #[test]
    fn node_status_markers_are_distinct() {
        let markers: Vec<&str> = all_node_statuses()
            .into_iter()
            .map(|s| node_status_text(s).marker)
            .collect();

        let mut unique = markers.clone();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(
            unique.len(),
            markers.len(),
            "two node statuses share a marker: {markers:?}"
        );
    }

    /// The key lists encounter markers and status markers together, so a
    /// glyph shared across the two groups defines itself twice. The per-group
    /// tests above cannot catch that on their own.
    #[test]
    fn no_marker_means_two_different_things() {
        let mut markers: Vec<(&str, String)> = Vec::new();

        for encounter_type in all_encounter_types() {
            let text = encounter_type_text(encounter_type);
            markers.push((text.marker, format!("encounter type {}", text.name)));
        }
        for status in all_node_statuses() {
            let text = node_status_text(status);
            markers.push((text.marker, format!("status {status:?}")));
        }

        for (index, (marker, owner)) in markers.iter().enumerate() {
            for (other_marker, other_owner) in markers.iter().skip(index + 1) {
                assert_ne!(
                    marker, other_marker,
                    "'{marker}' is used for both {owner} and {other_owner}"
                );
            }
        }
    }

    #[test]
    fn all_encounter_types_covers_every_variant() {
        // If a variant is added to EncounterType, this match stops compiling,
        // which is the reminder to add it to the key as well.
        for encounter_type in all_encounter_types() {
            match encounter_type {
                EncounterType::Entry
                | EncounterType::Combat
                | EncounterType::Event
                | EncounterType::Shop
                | EncounterType::Elite
                | EncounterType::Boss => {}
            }
        }
        assert_eq!(all_encounter_types().len(), 6);
    }
}

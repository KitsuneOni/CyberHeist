//! The rules behind the game's audio (story #111): which sound effect a played
//! card makes, and how a volume setting turns into a level on an audio bus.
//!
//! Plain Rust so both can be tested without Godot. The `AudioDirector`
//! autoload in GDScript owns the actual players, streams and buses, and asks
//! this module for the decisions:
//!
//! - every card type has its own [`SoundCue`], and `DrawPhase.play_card`
//!   reports the cue of the card that was played, and
//! - a volume is a whole percentage, 0 to 100, where 0 is complete silence
//!   rather than merely very quiet.

use godot::prelude::*;

use crate::card_data::CardType;

/// The bus level used for a silent volume. Godot's sliders and meters bottom
/// out at -80 dB, so anything below this is treated as the same silence.
pub const SILENT_DB: f32 = -80.0;

/// The sound effect played for a card, one per card type, so the player can
/// tell what kind of card went down by ear alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCue {
    Attack,
    Defence,
    Skill,
    Recon,
    SocialEngineering,
}

impl SoundCue {
    /// Every cue, in card type order. The audio director loads one stream for
    /// each of these.
    ///
    /// Kept by hand, since Rust cannot list an enum's variants without a
    /// derive crate. A card type left out would compile but never be loaded,
    /// so the tests check this against every card type the card data files
    /// actually use, and that each one has its sound file.
    pub const ALL: [SoundCue; 5] = [
        SoundCue::Attack,
        SoundCue::Defence,
        SoundCue::Skill,
        SoundCue::Recon,
        SoundCue::SocialEngineering,
    ];

    pub fn for_card_type(card_type: CardType) -> Self {
        match card_type {
            CardType::Attack => SoundCue::Attack,
            CardType::Defence => SoundCue::Defence,
            CardType::Skill => SoundCue::Skill,
            CardType::Recon => SoundCue::Recon,
            CardType::SocialEngineering => SoundCue::SocialEngineering,
        }
    }

    /// The name GDScript knows the cue by. It is also the file name of the
    /// cue's sound under `godot/assets/audio/sfx/`.
    pub fn id(self) -> &'static str {
        match self {
            SoundCue::Attack => "attack",
            SoundCue::Defence => "defence",
            SoundCue::Skill => "skill",
            SoundCue::Recon => "recon",
            SoundCue::SocialEngineering => "social_engineering",
        }
    }
}

/// A volume setting as the player sees it: a whole percentage from 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume {
    percent: u8,
}

impl Volume {
    pub const MAX_PERCENT: u8 = 100;

    /// Clamps into 0..=100, so a corrupt or hand-edited settings file can
    /// never push a bus louder than full volume or below silence.
    pub fn from_percent(percent: i64) -> Self {
        let clamped = percent.clamp(0, i64::from(Self::MAX_PERCENT));
        Self {
            percent: u8::try_from(clamped).expect("clamped into 0..=100"),
        }
    }

    pub fn percent(self) -> u8 {
        self.percent
    }

    /// Zero means silence. The bus is muted outright rather than left at a
    /// very low level, so nothing on it can be heard at all.
    pub fn is_silent(self) -> bool {
        self.percent == 0
    }

    /// The bus level for this volume, in decibels.
    ///
    /// Loudness is perceived roughly logarithmically, so a straight linear
    /// gain makes the top half of a slider barely change anything. The gain
    /// here follows the square of the slider position instead: 100% is
    /// 0 dB, 50% is about -12 dB and 10% is -40 dB. Silence, and anything
    /// that would fall below it, is [`SILENT_DB`].
    pub fn volume_db(self) -> f32 {
        if self.is_silent() {
            return SILENT_DB;
        }
        let fraction = f32::from(self.percent) / f32::from(Self::MAX_PERCENT);
        let gain = fraction * fraction;
        (20.0 * gain.log10()).max(SILENT_DB)
    }
}

/// GDScript's view of the rules above. It holds no state, so it is never
/// instantiated: everything is a static call such as
/// `AudioRules.volume_db(50)`.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct AudioRules;

#[godot_api]
impl AudioRules {
    /// The percentage actually stored and applied for a requested volume.
    #[func]
    fn clamp_volume(percent: i64) -> i64 {
        i64::from(Volume::from_percent(percent).percent())
    }

    #[func]
    fn volume_db(percent: i64) -> f32 {
        Volume::from_percent(percent).volume_db()
    }

    #[func]
    fn is_silent(percent: i64) -> bool {
        Volume::from_percent(percent).is_silent()
    }

    /// Every cue id a played card can report, one per card type.
    #[func]
    fn sound_cues() -> PackedStringArray {
        SoundCue::ALL
            .iter()
            .map(|cue| GString::from(cue.id()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_database::parse_cards;
    use std::collections::HashSet;

    const ALL_CARD_TYPES: [CardType; 5] = [
        CardType::Attack,
        CardType::Defence,
        CardType::Skill,
        CardType::Recon,
        CardType::SocialEngineering,
    ];

    #[test]
    fn every_card_type_has_its_own_sound() {
        let cues: HashSet<&str> = ALL_CARD_TYPES
            .iter()
            .map(|card_type| SoundCue::for_card_type(*card_type).id())
            .collect();

        assert_eq!(cues.len(), ALL_CARD_TYPES.len());
    }

    #[test]
    fn every_cue_a_card_can_make_is_listed_for_loading() {
        for card_type in ALL_CARD_TYPES {
            let cue = SoundCue::for_card_type(card_type);
            assert!(SoundCue::ALL.contains(&cue), "{cue:?} is not in ALL");
        }
    }

    /// The real card data, not a hand-kept list of types: a new card type
    /// used by any card must have its cue in `SoundCue::ALL` (or it is never
    /// loaded) and its sound file on disk.
    #[test]
    fn every_card_type_in_the_card_data_has_a_loaded_cue_and_a_sound_file() {
        let mut card_types_used = Vec::new();
        for file in ["cards.ron", "cursed_cards.ron"] {
            let text = std::fs::read_to_string(format!("../godot/data/{file}"))
                .unwrap_or_else(|error| panic!("could not read {file}: {error}"));
            card_types_used.extend(parse_cards(&text).values().map(|card| card.card_type));
        }
        assert!(!card_types_used.is_empty(), "the card data names no cards");

        for card_type in card_types_used {
            let cue = SoundCue::for_card_type(card_type);
            assert!(
                SoundCue::ALL.contains(&cue),
                "{card_type:?} plays {cue:?}, which is not in SoundCue::ALL"
            );
            let sound = format!("../godot/assets/audio/sfx/{}.wav", cue.id());
            assert!(
                std::path::Path::new(&sound).is_file(),
                "{cue:?} has no sound at {sound}"
            );
        }
    }

    #[test]
    fn cue_ids_are_usable_as_file_names() {
        for cue in SoundCue::ALL {
            let id = cue.id();
            assert!(!id.is_empty());
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{id} should be lowercase snake_case"
            );
        }
    }

    #[test]
    fn volume_is_clamped_to_a_percentage() {
        assert_eq!(Volume::from_percent(-20).percent(), 0);
        assert_eq!(Volume::from_percent(55).percent(), 55);
        assert_eq!(Volume::from_percent(250).percent(), 100);
        assert_eq!(Volume::from_percent(i64::MAX).percent(), 100);
    }

    #[test]
    fn zero_volume_is_silent_and_nothing_else_is() {
        assert!(Volume::from_percent(0).is_silent());
        assert!(Volume::from_percent(-5).is_silent());
        assert!(!Volume::from_percent(1).is_silent());
        assert!(!Volume::from_percent(100).is_silent());
    }

    #[test]
    fn silence_sits_at_the_bottom_of_the_range() {
        assert_eq!(Volume::from_percent(0).volume_db(), SILENT_DB);
    }

    #[test]
    fn full_volume_leaves_the_bus_unchanged() {
        assert_eq!(Volume::from_percent(100).volume_db(), 0.0);
    }

    #[test]
    fn the_curve_follows_the_square_of_the_slider() {
        let half = Volume::from_percent(50).volume_db();
        let tenth = Volume::from_percent(10).volume_db();

        assert!((half - -12.04).abs() < 0.01, "50% was {half} dB");
        assert!((tenth - -40.0).abs() < 0.01, "10% was {tenth} dB");
    }

    #[test]
    fn louder_settings_are_never_quieter() {
        let mut previous = Volume::from_percent(0).volume_db();
        for percent in 1..=100 {
            let level = Volume::from_percent(percent).volume_db();
            assert!(level >= previous, "{percent}% dropped to {level} dB");
            assert!(level >= SILENT_DB);
            previous = level;
        }
    }
}

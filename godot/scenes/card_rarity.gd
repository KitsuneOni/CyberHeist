# The colour each card rarity is shown in (story #109): Common white,
# Uncommon green, Rare blue, Legendary gold. Every screen that shows a card
# takes its colour from here, so a rarity reads the same wherever it is seen.
#
# Rarities arrive as their display names ("Common", ...) in the card detail
# dictionaries Rust hands over.
extends RefCounted

const COMMON := Color(0.88, 0.93, 0.9, 1)
# The terminal green the rest of the interface uses.
const UNCOMMON := Color(0.2, 1, 0.4, 1)
const RARE := Color(0.25, 0.6, 1, 1)
const LEGENDARY := Color(1, 0.7, 0.15, 1)
# For a rarity this build does not know, so it stands out as wrong rather than
# passing for one of the real four.
const UNKNOWN := Color(0.55, 0.55, 0.55, 1)

const COLOURS := {
	"Common": COMMON,
	"Uncommon": UNCOMMON,
	"Rare": RARE,
	"Legendary": LEGENDARY,
}

const STRIPE_WIDTH := 4
const STRIPE_GAP := 12.0


static func colour_for(rarity: String) -> Color:
	return COLOURS.get(rarity, UNKNOWN)


# A box with a bar in the rarity's colour down its left edge, for a list row's
# text, so each row is marked without changing what the row is made of.
static func stripe_style(rarity: String) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0, 0, 0, 0)
	style.border_width_left = STRIPE_WIDTH
	style.border_color = colour_for(rarity)
	style.content_margin_left = STRIPE_WIDTH + STRIPE_GAP
	return style

# Each zone's name and look (Trello card 115), shared by the map and combat
# screens so a zone always looks the same wherever the player meets it.
#
# Zones count from 0, as FlowCoordinator.zone_progress() does. A contract is
# at most three zones deep (BOSS_NAMES in sentry.rs), and a zone past the last
# one here reuses the deepest look rather than having none.
#
# The art is credited in assets/CREDITS.md.
extends RefCounted

const ZONES := [
	{
		"name": "Corporate Network",
		"background": "res://assets/backgrounds/zone_1_corporate.jpg",
		"accent": Color(0.35, 1, 0.82, 1),
	},
	{
		"name": "Bank",
		"background": "res://assets/backgrounds/zone_2_bank.jpg",
		"accent": Color(1, 0.8, 0.4, 1),
	},
	{
		"name": "Data Vault",
		"background": "res://assets/backgrounds/zone_3_vault.jpg",
		"accent": Color(0.45, 1, 0.55, 1),
	},
]

# How much of the screen's own background colour is laid over the art, so the
# cards, map and text on top stay readable.
const DIM := 0.55


# The look for zone `zone`: {name, background, accent}.
static func for_zone(zone: int) -> Dictionary:
	return ZONES[clampi(zone, 0, ZONES.size() - 1)]


# The look for the zone the player is in now. Takes the FlowCoordinator rather
# than naming the autoload, so test scripts can load this file too.
static func current(flow: Node) -> Dictionary:
	return for_zone(int(flow.zone_progress().get("current_zone", 0)))


# Lays the zone's art over `background` (the screen's full-screen ColorRect),
# under everything else, with a wash of the background colour on top to dim
# it. Returns the art, named ZoneBackdrop, so the screen can hide it.
static func add_backdrop(background: ColorRect, zone: Dictionary) -> TextureRect:
	var screen := background.get_parent()

	var art := TextureRect.new()
	art.name = "ZoneBackdrop"
	art.texture = load(zone["background"])
	art.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	art.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	art.mouse_filter = Control.MOUSE_FILTER_IGNORE
	art.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var dim := ColorRect.new()
	dim.name = "ZoneDim"
	dim.color = Color(background.color, DIM)
	dim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	art.add_child(dim)

	screen.add_child(art)
	screen.move_child(art, background.get_index() + 1)
	return art

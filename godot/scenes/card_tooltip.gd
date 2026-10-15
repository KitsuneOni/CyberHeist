# A card's full description, shown next to a card while the mouse is over it
# so the player never has to select a card just to find out what it does.
#
# Built from the same detail dictionary the combat detail panel reads
# (draw_phase.card_detail), and the formatting helpers below are shared with
# that panel so the two can never disagree about what a card says.
#
# It sits above the screen's layout (top_level) so containers never move it,
# and ignores the mouse so it can't steal the hover from the card under it.
extends PanelContainer

# Gap between the hovered card and the tooltip, and the tooltip and the screen
# edge it is kept inside.
const CARD_GAP := 8.0
const SCREEN_MARGIN := 8.0
const TEXT_WIDTH := 420.0

const PANEL_COLOUR := Color(0.0392157, 0.0588235, 0.0509804, 0.96)
const BORDER_COLOUR := Color(0.2, 1, 0.4, 0.8)
# The same colours and sizes as the combat detail panel.
const TITLE_COLOUR := Color(0.85, 0.9, 0.95, 1)
const STATS_COLOUR := Color(0.65, 0.85, 0.72, 1)
const DESCRIPTION_COLOUR := Color(0.82, 0.86, 0.9, 1)
const KEYWORDS_COLOUR := Color(0.65, 0.72, 0.78, 1)

var title_label := Label.new()
var stats_label := Label.new()
var description_label := Label.new()
var keywords_label := Label.new()

# The control the tooltip is currently describing, or null when hidden.
var _anchor: Control = null


static func title_for(detail: Dictionary) -> String:
	return (
		"%s — %s · %s"
		% [detail.get("name", "?"), detail.get("type", "?"), detail.get("rarity", "?")]
	)


static func stats_for(detail: Dictionary) -> String:
	return "%s · %s" % [detail.get("cost_text", ""), detail.get("noise_text", "")]


# One line per keyword, each with what it actually does.
static func keywords_for(detail: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in detail.get("keywords", []):
		lines.append("%s — %s" % [keyword.get("label", "?"), keyword.get("explanation", "")])
	return "\n".join(lines)


func _init() -> void:
	top_level = true
	visible = false
	z_index = 100
	mouse_filter = Control.MOUSE_FILTER_IGNORE

	var style := StyleBoxFlat.new()
	style.bg_color = PANEL_COLOUR
	style.border_color = BORDER_COLOUR
	style.set_border_width_all(1)
	style.set_content_margin_all(12)
	add_theme_stylebox_override("panel", style)

	var content := VBoxContainer.new()
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_theme_constant_override("separation", 6)
	add_child(content)

	_add_line(content, title_label, TITLE_COLOUR, 18)
	_add_line(content, stats_label, STATS_COLOUR, 14)
	_add_line(content, description_label, DESCRIPTION_COLOUR, 15)
	_add_line(content, keywords_label, KEYWORDS_COLOUR, 13)


# Fills the tooltip from a card_detail dictionary and places it beside
# `anchor`. A failed lookup hides it instead of showing a half-empty box.
func show_for(anchor: Control, detail: Dictionary) -> void:
	if anchor == null or not detail.get("ok", false):
		hide_tooltip()
		return

	title_label.text = title_for(detail)
	stats_label.text = stats_for(detail)
	description_label.text = str(detail.get("description", ""))
	keywords_label.text = keywords_for(detail)
	keywords_label.visible = not keywords_label.text.is_empty()

	_anchor = anchor
	visible = true
	# Let the labels settle their wrapped height before measuring.
	reset_size()
	_place_beside(anchor)


# Hides the tooltip, but only if it is still describing `anchor`, so a late
# exit from one card can't hide the tooltip that the next card just opened.
func hide_for(anchor: Control) -> void:
	if anchor == _anchor:
		hide_tooltip()


func hide_tooltip() -> void:
	_anchor = null
	visible = false


func describing() -> Control:
	return _anchor


# Above the card by default, since the hand sits low on the screen; below it if
# there isn't room. Always kept inside the visible screen.
func _place_beside(anchor: Control) -> void:
	var card := anchor.get_global_rect()
	var screen := get_viewport_rect().grow(-SCREEN_MARGIN)
	var box := get_combined_minimum_size()
	size = box

	var spot := Vector2(
		card.position.x + (card.size.x - box.x) * 0.5, card.position.y - CARD_GAP - box.y
	)
	if spot.y < screen.position.y:
		spot.y = card.end.y + CARD_GAP

	spot.x = clampf(spot.x, screen.position.x, maxf(screen.position.x, screen.end.x - box.x))
	spot.y = clampf(spot.y, screen.position.y, maxf(screen.position.y, screen.end.y - box.y))
	global_position = spot


func _add_line(parent: Control, label: Label, colour: Color, font_size: int) -> void:
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.custom_minimum_size.x = TEXT_WIDTH
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", font_size)
	parent.add_child(label)

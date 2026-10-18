# The deck viewer (Trello #55): every card the run owns, with how many copies
# and what each one does, so the player can plan around what they have. It
# opens over the map or a combat and closes with its Close button or Escape.
#
# Built in code rather than as a scene so the map and combat screens can each
# add one without either shared .tscn having to change. It only reads the run
# deck through FlowCoordinator and never changes it.
extends Control

# Signalled when the viewer closes, so the screen that opened it can put focus
# back where it was.
signal closed

const CardRarity := preload("res://scenes/card_rarity.gd")

const DIM_COLOUR := Color(0, 0, 0, 0.7)
const PANEL_COLOUR := Color(0.0392157, 0.0588235, 0.0509804, 0.97)
const BORDER_COLOUR := Color(0.2, 1, 0.4, 0.8)
const TITLE_COLOUR := Color(0.2, 1, 0.4, 1)
const NAME_COLOUR := Color(0.85, 0.9, 0.95, 1)
const STATS_COLOUR := Color(0.65, 0.85, 0.72, 1)
const DESCRIPTION_COLOUR := Color(0.82, 0.86, 0.9, 1)
const KEYWORDS_COLOUR := Color(0.65, 0.72, 0.78, 1)
const DIVIDER_COLOUR := Color(0.2, 1, 0.4, 0.15)

# Distance kept between the panel and the screen edge.
const PAGE_MARGIN := 48
const ROW_SEPARATION := 10

var title_label := Label.new()
var list := VBoxContainer.new()
var close_button := Button.new()

# What is on screen, as FlowCoordinator.run_deck_listing() returned it, sorted
# for display.
var shown_cards: Array = []


# Groups cards by type, then alphabetically by name within a type, so like
# cards sit together and the order never shifts between openings.
static func sorted_listing(listing: Array) -> Array:
	var sorted := listing.duplicate()
	sorted.sort_custom(
		func(a: Dictionary, b: Dictionary) -> bool:
			var a_type := str(a.get("type", ""))
			var b_type := str(b.get("type", ""))
			if a_type != b_type:
				return a_type < b_type
			return str(a.get("name", "")) < str(b.get("name", ""))
	)
	return sorted


static func total_cards(listing: Array) -> int:
	var total := 0
	for card: Dictionary in listing:
		total += int(card.get("count", 0))
	return total


static func heading_for(card: Dictionary) -> String:
	return (
		"%d×  %s — %s · %s"
		% [
			int(card.get("count", 0)),
			card.get("name", "?"),
			card.get("type", "?"),
			card.get("rarity", "?"),
		]
	)


static func keywords_for(card: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		lines.append("%s — %s" % [keyword.get("label", "?"), keyword.get("explanation", "")])
	return "\n".join(lines)


func _init() -> void:
	visible = false
	set_anchors_preset(Control.PRESET_FULL_RECT)
	# Swallow clicks so nothing underneath can be pressed while it is open.
	mouse_filter = Control.MOUSE_FILTER_STOP
	z_index = 50

	var dim := ColorRect.new()
	dim.color = DIM_COLOUR
	dim.set_anchors_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(dim)

	var page := MarginContainer.new()
	page.set_anchors_preset(Control.PRESET_FULL_RECT)
	page.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side in ["left", "top", "right", "bottom"]:
		page.add_theme_constant_override("margin_" + side, PAGE_MARGIN)
	add_child(page)

	var panel := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = PANEL_COLOUR
	style.border_color = BORDER_COLOUR
	style.set_border_width_all(2)
	style.set_corner_radius_all(12)
	style.set_content_margin_all(24)
	panel.add_theme_stylebox_override("panel", style)
	page.add_child(panel)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 14)
	panel.add_child(column)

	title_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title_label.add_theme_color_override("font_color", TITLE_COLOUR)
	title_label.add_theme_font_size_override("font_size", 26)
	column.add_child(title_label)

	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	column.add_child(scroll)

	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list.add_theme_constant_override("separation", ROW_SEPARATION)
	scroll.add_child(list)

	close_button.text = "Close"
	close_button.custom_minimum_size = Vector2(200, 40)
	close_button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	close_button.pressed.connect(close)
	column.add_child(close_button)


# Reads the run deck fresh each time, so a card picked up since the last look
# is always there. Looked up at runtime rather than named, so tests can load
# this script before the autoloads exist.
func open() -> void:
	shown_cards = sorted_listing(get_node("/root/FlowCoordinator").run_deck_listing())
	title_label.text = (
		"YOUR DECK  ·  %d cards, %d different" % [total_cards(shown_cards), shown_cards.size()]
	)
	_rebuild_rows()
	visible = true
	close_button.grab_focus()


func close() -> void:
	if not visible:
		return
	visible = false
	closed.emit()


func is_open() -> bool:
	return visible


func _unhandled_input(event: InputEvent) -> void:
	if visible and event.is_action_pressed("ui_cancel"):
		close()
		get_viewport().set_input_as_handled()


func _rebuild_rows() -> void:
	for child in list.get_children():
		list.remove_child(child)
		child.queue_free()

	if shown_cards.is_empty():
		list.add_child(_line("Your deck is empty.", DESCRIPTION_COLOUR, 16))
		return

	for index in shown_cards.size():
		if index > 0:
			var divider := ColorRect.new()
			divider.color = DIVIDER_COLOUR
			divider.custom_minimum_size = Vector2(0, 1)
			list.add_child(divider)
		list.add_child(_row(shown_cards[index]))


func _row(card: Dictionary) -> VBoxContainer:
	var row := VBoxContainer.new()
	row.name = "Card_%s" % str(card.get("id", "unknown"))
	row.add_theme_constant_override("separation", 4)
	# The heading takes the card's rarity colour (#109).
	row.add_child(_line(heading_for(card), CardRarity.colour_for(str(card.get("rarity", ""))), 18))
	row.add_child(
		_line("%s · %s" % [card.get("cost_text", ""), card.get("noise_text", "")], STATS_COLOUR, 14)
	)
	row.add_child(_line(str(card.get("description", "")), DESCRIPTION_COLOUR, 15))
	var keywords := keywords_for(card)
	if not keywords.is_empty():
		row.add_child(_line(keywords, KEYWORDS_COLOUR, 13))
	return row


func _line(text: String, colour: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", font_size)
	return label

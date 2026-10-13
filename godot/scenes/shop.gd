# The dark web shop a shop node opens (Trello #38, with S1 #104, S2 #105,
# S3 #106 and S4 #107).
#
# Five cards are for sale, each with its full details and a price set by its
# rarity. The stock is rolled the first time this screen opens during a shop
# encounter and kept on the run, so it stays the same for the visit even if the
# screen is rebuilt; a bought card's slot shows as sold. Beside Leave are two
# services: Scrub Logs, which lowers noise once per visit, and a reroll, which
# replaces every unsold card and costs more each time. Anything the player
# cannot buy right now has its button disabled and its price in red. Leave
# completes the shop node and returns to the contract map.
#
# The rules live in ../../cyber_heist/src/shop.rs, and ShopNode (shop_node.rs)
# is the bridge. Every paid action is one Rust call that checks the price
# against the player's credits and takes it together with its effect, or
# changes nothing.
#
# The S1 to S4 cards give this screen's visual design to Neel, so the layout
# here is deliberately plain and functional, in the main menu's terminal style.
# It reads everything from ShopNode, so a redesign only has to replace this
# file and the scene.
extends Control

const PRICE_COLOUR := Color(0.2, 1, 0.4, 1)
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)
# Sold slots stay in place, dimmed, so the rest of the stock does not move.
const SOLD_ROW_ALPHA := 0.45

@onready var shop_node: Node = $ShopNode
@onready var credits_label: Label = %CreditsLabel
@onready var stock_list: VBoxContainer = %StockList
@onready var status_label: Label = %StatusLabel
@onready var leave_button: Button = %LeaveButton
@onready var scrub_logs_details: Label = %ScrubLogsDetails
@onready var scrub_logs_price_label: Label = %ScrubLogsPriceLabel
@onready var scrub_logs_button: Button = %ScrubLogsButton
@onready var reroll_details: Label = %RerollDetails
@onready var reroll_price_label: Label = %RerollPriceLabel
@onready var reroll_button: Button = %RerollButton

# Each stock slot's controls and current card name, keyed by the slot number
# ShopNode reports (and buy() takes), never by position in the stock list, so a
# row and the card it sells cannot drift apart.
var _rows: Dictionary[int, Control] = {}
var _detail_labels: Dictionary[int, Label] = {}
var _price_labels: Dictionary[int, Label] = {}
var _buy_buttons: Dictionary[int, Button] = {}
var _card_names: Dictionary[int, String] = {}
# Guards against a double click leaving twice.
var _leaving := false


func _ready() -> void:
	leave_button.pressed.connect(_on_leave_pressed)
	scrub_logs_button.pressed.connect(_on_scrub_logs_pressed)
	reroll_button.pressed.connect(_on_reroll_pressed)
	_build_stock_rows()
	_refresh()
	_focus_first_control()


# Builds one row per card for sale. The number of slots is fixed for the
# visit, so this only runs once; _refresh updates the rows after that,
# including the cards a reroll puts in them.
func _build_stock_rows() -> void:
	var stock: Array = shop_node.stock()
	if stock.is_empty():
		status_label.text = "The shop has nothing for sale."
		return

	for card: Dictionary in stock:
		_add_stock_row(card)


func _add_stock_row(card: Dictionary) -> void:
	var slot := int(card.get("slot", -1))
	if slot < 0:
		push_error("ShopNode listed a card without a slot: %s" % card)
		return

	var row := HBoxContainer.new()
	row.name = "Slot%d" % slot
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.mouse_filter = Control.MOUSE_FILTER_PASS
	row.add_child(details)

	var purchase := VBoxContainer.new()
	purchase.custom_minimum_size = Vector2(160, 0)
	purchase.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_child(purchase)

	var price_label := Label.new()
	price_label.name = "PriceLabel"
	price_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	purchase.add_child(price_label)

	var buy_button := Button.new()
	buy_button.name = "BuyButton"
	buy_button.theme_type_variation = &"TerminalMenuButton"
	buy_button.add_theme_font_size_override("font_size", 22)
	buy_button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	buy_button.pressed.connect(_on_buy_pressed.bind(slot))
	purchase.add_child(buy_button)

	stock_list.add_child(row)
	_rows[slot] = row
	_detail_labels[slot] = details
	_price_labels[slot] = price_label
	_buy_buttons[slot] = buy_button


# Three lines, so all five cards fit on screen at once: name, type and rarity;
# what it costs to play, the noise it makes and the effects it carries; then
# what it does.
func _card_details_text(card: Dictionary) -> String:
	var heading := "%s  —  %s · %s" % [
		String(card.get("name", "?")).to_upper(),
		card.get("type", "?"),
		card.get("rarity", "?"),
	]

	var facts: Array[String] = [
		String(card.get("cost_text", "")),
		String(card.get("noise_text", "")),
	]
	var labels: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		labels.append(String(keyword.get("label", "")))
	if not labels.is_empty():
		facts.append("Effects: %s" % ", ".join(labels))

	return "\n".join([heading, " · ".join(facts), String(card.get("description", ""))])


# Hovering a card explains each of its effects, as the combat card detail does.
func _keyword_explanations(card: Dictionary) -> String:
	var lines: Array[String] = []
	for keyword: Dictionary in card.get("keywords", []):
		lines.append("%s: %s" % [keyword.get("label", ""), keyword.get("explanation", "")])
	return "\n".join(lines)


# Re-reads the stock, the services and the balance, so every slot shows its
# current card, whether it is sold and whether it is still affordable after
# the last purchase or reroll.
func _refresh() -> void:
	credits_label.text = "$ %d  credits on hand" % PlayerStateGlobal.money()

	for card: Dictionary in shop_node.stock():
		var slot := int(card.get("slot", -1))
		if not _rows.has(slot):
			continue
		var price := int(card.get("price", 0))
		var price_label := _price_labels[slot]
		var buy_button := _buy_buttons[slot]
		# A reroll changes the card in a slot, so the text and name are
		# refreshed with everything else.
		_detail_labels[slot].text = _card_details_text(card)
		_detail_labels[slot].tooltip_text = _keyword_explanations(card)
		_card_names[slot] = String(card.get("name", "the card"))

		if card.get("sold", false):
			_rows[slot].modulate.a = SOLD_ROW_ALPHA
			price_label.text = "SOLD"
			price_label.remove_theme_color_override("font_color")
			buy_button.text = "sold"
			buy_button.disabled = true
		else:
			var affordable: bool = card.get("affordable", false)
			_rows[slot].modulate.a = 1.0
			_show_price(price_label, price, affordable)
			buy_button.text = "[ buy ]"
			buy_button.disabled = not affordable

		_update_focus_mode(buy_button)

	_refresh_scrub_logs()
	_refresh_reroll()


# The services sit in the footer beside Leave, so the five cards keep the
# rest of the screen. Each shows two short lines; hovering explains it fully.
func _refresh_scrub_logs() -> void:
	var offer: Dictionary = shop_node.scrub_logs_offer()
	var reduction := int(offer.get("reduction", 0))
	var noise := int(offer.get("noise", 0))
	scrub_logs_details.text = "SCRUB LOGS\n−%d noise · now %d/%d" % [
		reduction,
		noise,
		int(offer.get("max_noise", 0)),
	]
	var explanation := "Wipe the access logs to lower noise by %d. Once per visit." % reduction
	scrub_logs_details.tooltip_text = explanation
	scrub_logs_button.tooltip_text = explanation

	if offer.get("sold", false):
		scrub_logs_price_label.text = "SOLD OUT"
		scrub_logs_price_label.remove_theme_color_override("font_color")
		scrub_logs_button.text = "sold out"
		scrub_logs_button.disabled = true
	else:
		var affordable: bool = offer.get("affordable", false)
		_show_price(scrub_logs_price_label, int(offer.get("price", 0)), affordable)
		scrub_logs_button.text = "[ scrub ]"
		# ShopNode reports it unaffordable when there is no noise it would
		# remove, so the button and the red price agree in that case too.
		scrub_logs_button.disabled = not affordable

	_update_focus_mode(scrub_logs_button)


func _refresh_reroll() -> void:
	var offer: Dictionary = shop_node.reroll_offer()
	reroll_details.text = "REROLL STOCK\nunsold cards only"
	var explanation := (
		"Replace every card still for sale with a new one. Sold cards stay sold."
		+ " Each reroll in this shop costs more than the last."
	)
	reroll_details.tooltip_text = explanation
	reroll_button.tooltip_text = explanation

	if not offer.get("available", false):
		reroll_price_label.text = "SOLD OUT"
		reroll_price_label.remove_theme_color_override("font_color")
		reroll_button.text = "nothing left"
		reroll_button.disabled = true
	else:
		var affordable: bool = offer.get("affordable", false)
		_show_price(reroll_price_label, int(offer.get("price", 0)), affordable)
		reroll_button.text = "[ reroll ]"
		reroll_button.disabled = not affordable

	_update_focus_mode(reroll_button)


func _show_price(price_label: Label, price: int, affordable: bool) -> void:
	price_label.text = "%d cr" % price
	price_label.add_theme_color_override(
		"font_color", PRICE_COLOUR if affordable else UNAFFORDABLE_COLOUR
	)


# A disabled button is skipped by keyboard navigation, as the main menu does
# for Continue run.
func _update_focus_mode(button: Button) -> void:
	button.focus_mode = Control.FOCUS_NONE if button.disabled else Control.FOCUS_ALL


func _focus_first_control() -> void:
	var candidates: Array[Button] = []
	candidates.append_array(_buy_buttons.values())
	candidates.append(scrub_logs_button)
	candidates.append(reroll_button)
	for button in candidates:
		if not button.disabled:
			button.grab_focus()
			return
	leave_button.grab_focus()


# Keeps keyboard focus on a usable control after an action that may have
# disabled the button the player just pressed.
func _refocus_if_disabled(pressed_button: Button) -> void:
	if pressed_button.disabled:
		_focus_first_control()


func _on_buy_pressed(slot: int) -> void:
	var result: Dictionary = shop_node.buy(slot)
	if result.get("ok", false):
		status_label.text = "Bought %s for %d credits. It is in your deck." % [
			_card_names.get(slot, "the card"),
			int(result.get("price", 0)),
		]
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), "that card")

	_refresh()
	if _buy_buttons.has(slot):
		_refocus_if_disabled(_buy_buttons[slot])
	else:
		_focus_first_control()


func _on_scrub_logs_pressed() -> void:
	var result: Dictionary = shop_node.scrub_logs()
	if result.get("ok", false):
		status_label.text = "Logs scrubbed for %d credits. Noise down %d to %d." % [
			int(result.get("price", 0)),
			int(result.get("noise_removed", 0)),
			int(result.get("noise", 0)),
		]
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), "Scrub Logs")

	_refresh()
	_refocus_if_disabled(scrub_logs_button)


func _on_reroll_pressed() -> void:
	var result: Dictionary = shop_node.reroll()
	if result.get("ok", false):
		status_label.text = "Stock rerolled for %d credits. The next reroll costs %d." % [
			int(result.get("price", 0)),
			int(result.get("next_price", 0)),
		]
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), "a reroll")

	_refresh()
	_refocus_if_disabled(reroll_button)


# `item` names what was refused, such as "that card" or "Scrub Logs".
func _refusal_text(error: String, item: String) -> String:
	match error:
		"not_enough_credits":
			return "Not enough credits for %s." % item
		"already_sold":
			# Upper-cases only the first letter, so "that card" reads as a
			# sentence and "Scrub Logs" keeps its own capitals.
			var sentence_start := item.left(1).to_upper() + item.substr(1)
			return "%s has already been sold this visit." % sentence_start
		"nothing_to_scrub":
			return "Noise is already at zero. There are no logs to scrub."
		"nothing_to_reroll":
			return "There are no other cards left to reroll into."
		_:
			return "Could not buy %s: %s" % [item, error]


func _on_leave_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them in the shop.
		_leaving = false
		status_label.text = "Could not leave the shop: %s" % result.get("error", "unknown error")

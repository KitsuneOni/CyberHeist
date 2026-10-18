extends Control

@onready var credits_label: Label = %CreditsLabel
@onready var upgrade_list: VBoxContainer = %UpgradeList
@onready var status_label: Label = %StatusLabel
@onready var back_button: Button = %BackButton
@onready var enter_shop_button: Button = %EnterShopButton

const PRICE_COLOUR := Color(0.2, 1, 0.4, 1)
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)
# An owned relic stays in place, dimmed, so the list does not shift.
const OWNED_ROW_ALPHA := 0.45

var _buy_buttons: Array[Button] = []

# Guards against a double click leaving twice.
var _leaving := false

# Entering spends every point and cannot be undone, so it takes two presses.
var _enter_armed := false


func _ready() -> void:
	FlowCoordinator._progression.award_run_completion(1500)
	back_button.pressed.connect(_on_back_pressed)
	status_label.text = "Finish a contract to earn 1 point, plus 1 for every 500 credits you hold."
	enter_shop_button.tooltip_text = "Spend all your points to browse the normal shop once. Leaving takes you back to the contracts."
	enter_shop_button.pressed.connect(_on_enter_shop_pressed)
	_refresh()
	_focus_first_control()


# Re-reads the balance and rebuilds the relic list.
func _refresh() -> void:
	var points := FlowCoordinator.progression_points()
	credits_label.text = "$ %d  credits on hand    %d %s" % [
		PlayerStateGlobal.money(),
		points,
		"point" if points == 1 else "points",
	]

	for child in upgrade_list.get_children():
		upgrade_list.remove_child(child)
		child.queue_free()
	_buy_buttons.clear()

	for relic: Dictionary in FlowCoordinator.relic_listing():
		_add_relic_row(relic)
		
	_enter_armed = false
	_refresh_enter_shop_button(points)

func _refresh_enter_shop_button(points: int) -> void:
	enter_shop_button.disabled = points < 1
	if points < 1:
		enter_shop_button.text = "> Enter shop (need 1 point)"
	else:
		enter_shop_button.text = "> Enter shop (spends %d %s)" % [
			points,
			"point" if points == 1 else "points",
		]
	enter_shop_button.focus_mode = (
		Control.FOCUS_NONE if enter_shop_button.disabled else Control.FOCUS_ALL
	)


func _on_enter_shop_pressed() -> void:
	if _leaving:
		return

	# The first press arms it, the second enters.
	if not _enter_armed:
		_enter_armed = true
		enter_shop_button.text = "> Confirm: spend all %d?" % FlowCoordinator.progression_points()
		status_label.text = "Press again to spend all your points and enter the shop. You can enter once, and leaving returns you to the contracts."
		return

	_leaving = true
	var result: Dictionary = FlowCoordinator.enter_shop_from_dark_web()
	if not result.get("ok", false):
		_leaving = false
		status_label.text = "Could not enter the shop: %s" % result.get("error", "unknown error")
		_refresh()

func _on_back_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.close_dark_web_shop()
	if not result.get("ok", false):
		_leaving = false
		status_label.text = "Could not leave the dark web shop: %s" % result.get("error", "unknown error")

func _add_relic_row(relic: Dictionary) -> void:
	var relic_id := String(relic.get("id", ""))
	var relic_name := String(relic.get("name", "the relic"))
	var owned: bool = relic.get("owned", false)
	var available: bool = relic.get("available", false)
	var affordable: bool = relic.get("affordable", false)

	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 24)

	var details := Label.new()
	details.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 16)
	details.text = "%s\n%s" % [relic_name.to_upper(), relic.get("description", "")]
	row.add_child(details)

	var purchase := VBoxContainer.new()
	purchase.custom_minimum_size = Vector2(160, 0)
	purchase.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_child(purchase)

	var price_label := Label.new()
	price_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	purchase.add_child(price_label)

	var button := Button.new()
	button.theme_type_variation = &"TerminalMenuButton"
	button.add_theme_font_size_override("font_size", 22)
	button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	purchase.add_child(button)

	if owned:
		row.modulate.a = OWNED_ROW_ALPHA
		price_label.text = "OWNED"
		button.text = "owned"
		button.disabled = true
	elif not available:
		price_label.text = "SOON"
		button.text = "coming soon"
		button.disabled = true
	else:
		price_label.text = "%d pt" % int(relic.get("price", 1))
		price_label.add_theme_color_override(
			"font_color", PRICE_COLOUR if affordable else UNAFFORDABLE_COLOUR
		)
		button.text = "[ buy ]"
		button.disabled = not affordable
		button.pressed.connect(_on_buy_pressed.bind(relic_id, relic_name))

	button.focus_mode = Control.FOCUS_NONE if button.disabled else Control.FOCUS_ALL
	upgrade_list.add_child(row)
	_buy_buttons.append(button)


func _focus_first_control() -> void:
	for button in _buy_buttons:
		if not button.disabled:
			button.grab_focus()
			return
	back_button.grab_focus()


func _on_buy_pressed(relic_id: String, relic_name: String) -> void:
	var result: Dictionary = FlowCoordinator.buy_relic(relic_id)
	if result.get("ok", false):
		status_label.text = "Bought %s. It applies from now on." % relic_name
	else:
		status_label.text = _refusal_text(String(result.get("error", "")), relic_name)

	_refresh()
	_focus_first_control()


func _refusal_text(error: String, relic_name: String) -> String:
	match error:
		"not_enough_points":
			return "You need a point to buy %s." % relic_name
		"already_owned":
			return "You already own %s." % relic_name
		"not_available":
			return "%s is not available yet." % relic_name
		_:
			return "Could not buy %s: %s" % [relic_name, error]
# The computer desktop (Trello #25): the player's own machine, shown between
# contracts. A new run from the main menu and Take another contract on the
# victory screen both land here before any contract is accepted.
#
# Three apps: contracts.exe opens the offer picker, darkweb.onion opens the
# dark web shop for permanent upgrades, and toolkit/ opens the deck viewer
# over the desktop. A terminal window sums up where the player stands and a
# second window lists the upgrades they own.
#
# Navigation goes through FlowCoordinator like every other screen; this script
# never changes scene itself.
extends Control

const DeckViewer := preload("res://scenes/deck_viewer.gd")

# Prefixes keep every entry the same width in the monospace font, so the
# selection marker appearing never shifts the text sideways.
const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "

# How long the terminal takes to type its lines out.
const TERMINAL_TYPE_SECONDS := 0.8

@onready var contracts_app: Button = %ContractsApp
@onready var dark_web_app: Button = %DarkWebApp
@onready var toolkit_app: Button = %ToolkitApp
@onready var app_detail_label: Label = %AppDetailLabel
@onready var balance_label: Label = %BalanceLabel
@onready var clock_label: Label = %ClockLabel
@onready var terminal_log: Label = %TerminalLog
@onready var installed_label: Label = %InstalledLabel
@onready var status_label: Label = %StatusLabel

# Made in _ready rather than here, so a screen freed before it is ever shown
# leaves nothing orphaned behind.
var deck_viewer: Control

# Each app button's label and what it says about itself when selected.
var _labels := {}
var _details := {}

# Guards against a double click opening two screens.
var _leaving := false


func _ready() -> void:
	var offers: int = FlowCoordinator.contract_offers().size()
	var points: int = FlowCoordinator.progression_points()
	var deck_size: int = FlowCoordinator.run_deck_size()

	_labels = {
		contracts_app: "contracts.exe  [%d %s]" % [offers, _plural(offers, "offer", "offers")],
		dark_web_app: "darkweb.onion  [%d %s]" % [points, _plural(points, "point", "points")],
		toolkit_app: "toolkit/  [%d %s]" % [deck_size, _plural(deck_size, "card", "cards")],
	}
	_details = {
		contracts_app: "Browse the jobs on offer and accept one to start the run.",
		dark_web_app: "Spend points on permanent upgrades that carry into every future run.",
		toolkit_app: "Look through the cards you will take into the next contract.",
	}

	for button: Button in _labels:
		button.focus_entered.connect(_on_selection_changed.bind(button))
		button.focus_exited.connect(_render_label.bind(button))
		# Hover moves the selection, so the mouse and keyboard share one
		# highlight instead of two that can disagree.
		button.mouse_entered.connect(button.grab_focus)
		_render_label(button)

	contracts_app.pressed.connect(_on_contracts_pressed)
	dark_web_app.pressed.connect(_on_dark_web_pressed)
	toolkit_app.pressed.connect(_on_toolkit_pressed)

	deck_viewer = DeckViewer.new()
	deck_viewer.name = "DeckViewer"
	deck_viewer.closed.connect(toolkit_app.grab_focus)
	add_child(deck_viewer)

	balance_label.text = "$ %d credits  ·  %d %s" % [
		PlayerStateGlobal.money(),
		points,
		_plural(points, "point", "points"),
	]
	_update_clock()
	installed_label.text = installed_text(FlowCoordinator.relic_listing())
	terminal_log.text = "\n".join(terminal_lines(offers, points, deck_size))
	_type_out_terminal()

	contracts_app.grab_focus()


func _process(_delta: float) -> void:
	_update_clock()


# What the terminal window says when the desktop opens: how many jobs are
# waiting, what there is to spend, and how big the deck is.
static func terminal_lines(offers: int, points: int, deck_size: int) -> Array[String]:
	var lines: Array[String] = ["> uplink secured. no trace detected."]
	if offers > 0:
		lines.append("> %d %s waiting in contracts.exe." % [
			offers,
			_plural(offers, "contract", "contracts"),
		])
	else:
		lines.append("> no contracts on offer right now.")
	if points > 0:
		lines.append("> %d %s to spend on darkweb.onion." % [
			points,
			_plural(points, "point", "points"),
		])
	else:
		lines.append("> no points yet. finish a contract to earn one.")
	lines.append("> toolkit/ holds %d %s." % [deck_size, _plural(deck_size, "card", "cards")])
	lines.append("> awaiting input_")
	return lines


# The installed upgrades window: every relic owned, or a line saying there are
# none yet so the window is never just empty.
static func installed_text(relics: Array) -> String:
	var owned: Array[String] = []
	for relic: Dictionary in relics:
		if relic.get("owned", false):
			owned.append("%s  -  %s" % [
				String(relic.get("name", "")).to_upper(),
				String(relic.get("description", "")),
			])
	if owned.is_empty():
		return "nothing installed yet. upgrades bought on the dark web show up here."
	return "\n".join(owned)


static func _plural(count: int, singular: String, plural: String) -> String:
	return singular if count == 1 else plural


# Whether the toolkit's deck viewer is open over the desktop.
func is_toolkit_open() -> bool:
	return deck_viewer.is_open()


func _update_clock() -> void:
	var time := Time.get_time_dict_from_system()
	clock_label.text = "%02d:%02d" % [time.get("hour", 0), time.get("minute", 0)]


# Types the terminal's lines out rather than showing them all at once, so the
# desktop feels like a machine waking up. The text is all there from the
# start, which keeps it readable for tests and screen readers.
func _type_out_terminal() -> void:
	terminal_log.visible_ratio = 0.0
	var tween := create_tween()
	tween.tween_property(terminal_log, "visible_ratio", 1.0, TERMINAL_TYPE_SECONDS)


func _render_label(button: Button) -> void:
	var prefix := SELECTED_PREFIX if button.has_focus() else IDLE_PREFIX
	button.text = prefix + str(_labels[button])


func _on_selection_changed(button: Button) -> void:
	_render_label(button)
	app_detail_label.text = str(_details[button])


func _on_contracts_pressed() -> void:
	_open(FlowCoordinator.open_contracts, "Could not open contracts")


func _on_dark_web_pressed() -> void:
	_open(FlowCoordinator.open_dark_web_shop, "Could not open the dark web")


func _on_toolkit_pressed() -> void:
	deck_viewer.open()


func _open(opener: Callable, failure: String) -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = opener.call()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them on the desktop.
		_leaving = false
		status_label.text = "%s: %s" % [failure, result.get("error", "unknown error")]

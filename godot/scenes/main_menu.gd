# The first screen after launch: wordmark, a short menu column, and a looping
# packet capture scrolling behind it. Layout follows sheet 2 of the UI mockups
# (docs/ui-reference/cyberheist-ui-mockups.pdf).
#
# Navigation goes through FlowCoordinator like every other screen; this script
# never changes scene itself.
extends Control

# Prefixes keep every entry the same width in the monospace font, so the
# selection marker appearing never shifts the text sideways.
const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "

# Characters swapped in for the one-frame glitch when the selection moves.
const GLITCH_GLYPHS := "#%&@$/\\|<>=+*01"

# How many packet lines the background holds, and how fast it scrolls in
# pixels per second. One line is recycled off the top for every line height
# scrolled, so the feed loops for as long as the menu is open.
const FEED_LINES := 48
const FEED_SCROLL_SPEED := 22.0

@onready var new_contract_button: Button = %NewContractButton
@onready var continue_button: Button = %ContinueButton
@onready var quit_button: Button = %QuitButton
@onready var status_label: Label = %StatusLabel
@onready var build_label: Label = %BuildLabel
@onready var packet_feed: Label = %PacketFeed

var _labels := {}
var _feed: Array[String] = []
var _feed_offset := 0.0
var _rng := RandomNumberGenerator.new()


func _ready() -> void:
	_labels = {
		new_contract_button: "New contract",
		continue_button: "Continue run",
		quit_button: "Quit",
	}

	# Continue stays visible so the player knows it exists, but is greyed out
	# and taken out of keyboard navigation until there is a run to resume.
	var can_continue: bool = FlowCoordinator.has_saved_run()
	continue_button.disabled = not can_continue
	continue_button.focus_mode = Control.FOCUS_ALL if can_continue else Control.FOCUS_NONE
	if not can_continue:
		_labels[continue_button] = "Continue run   [no save]"

	for button: Button in _labels:
		button.focus_entered.connect(_on_selection_changed.bind(button))
		button.focus_exited.connect(_render_label.bind(button))
		# Hover moves the selection, so the mouse and keyboard share one
		# highlight instead of two that can disagree.
		button.mouse_entered.connect(_on_hovered.bind(button))
		_render_label(button)

	new_contract_button.pressed.connect(_on_new_contract_pressed)
	continue_button.pressed.connect(_on_continue_pressed)
	quit_button.pressed.connect(_on_quit_pressed)

	var version := str(ProjectSettings.get_setting("application/config/version", ""))
	build_label.text = "build %s" % (version if not version.is_empty() else "dev")

	_rng.randomize()
	for _line in FEED_LINES:
		_feed.append(_packet_line())
	packet_feed.text = "\n".join(_feed)

	new_contract_button.grab_focus()


func _process(delta: float) -> void:
	var line_height: float = packet_feed.get_line_height()
	if line_height <= 0.0:
		return

	_feed_offset += FEED_SCROLL_SPEED * delta
	if _feed_offset >= line_height:
		_feed_offset -= line_height
		_feed.pop_front()
		_feed.append(_packet_line())
		packet_feed.text = "\n".join(_feed)
	packet_feed.position.y = -_feed_offset


func _render_label(button: Button) -> void:
	var prefix := SELECTED_PREFIX if button.has_focus() else IDLE_PREFIX
	button.text = prefix + str(_labels[button])


func _on_hovered(button: Button) -> void:
	if button.focus_mode != Control.FOCUS_NONE:
		button.grab_focus()


# The mockup's "one-frame glitch on change": scramble a couple of characters
# for a single frame, then settle on the real label.
func _on_selection_changed(button: Button) -> void:
	var label: String = SELECTED_PREFIX + str(_labels[button])
	var glitched := label
	for _swap in 2:
		var index := _rng.randi_range(SELECTED_PREFIX.length(), label.length() - 1)
		glitched[index] = GLITCH_GLYPHS[_rng.randi_range(0, GLITCH_GLYPHS.length() - 1)]
	button.text = glitched

	await get_tree().process_frame
	if is_instance_valid(button):
		_render_label(button)


func _on_new_contract_pressed() -> void:
	var result: Dictionary = FlowCoordinator.start_new_contract()
	if not result.get("ok", false):
		status_label.text = "Could not start a contract: %s" % result.get("error", "unknown error")


func _on_continue_pressed() -> void:
	var result: Dictionary = FlowCoordinator.continue_run()
	if not result.get("ok", false):
		status_label.text = "Could not continue: %s" % result.get("error", "unknown error")


func _on_quit_pressed() -> void:
	get_tree().quit()


# One line of made-up traffic for the background, shaped like a packet
# capture: timestamp, source and destination, protocol, flags and length.
func _packet_line() -> String:
	var protocols := ["TCP", "TCP", "TCP", "UDP", "TLSv1.3", "DNS", "ARP", "ICMP"]
	var flags := ["[ACK]", "[SYN]", "[SYN, ACK]", "[PSH, ACK]", "[FIN, ACK]", "[RST]", ""]
	return "%02d:%02d:%02d.%03d  %s:%-5d -> %s:%-5d  %-7s %-11s len=%d" % [
		_rng.randi_range(0, 23),
		_rng.randi_range(0, 59),
		_rng.randi_range(0, 59),
		_rng.randi_range(0, 999),
		_address(),
		_rng.randi_range(1024, 65535),
		_address(),
		[22, 53, 80, 443, 445, 3389, 8080][_rng.randi_range(0, 6)],
		protocols[_rng.randi_range(0, protocols.size() - 1)],
		flags[_rng.randi_range(0, flags.size() - 1)],
		_rng.randi_range(40, 1500),
	]


func _address() -> String:
	return "%d.%d.%d.%d" % [
		[10, 172, 192][_rng.randi_range(0, 2)],
		_rng.randi_range(0, 255),
		_rng.randi_range(0, 255),
		_rng.randi_range(1, 254),
	]

extends Control

@onready var draw_phase: Node = $DrawPhase
@onready var sentry: Node = $Sentry
@onready var turn_label: Label = $VBoxContainer/TurnLabel
@onready var noise_label: Label = $VBoxContainer/NoiseLabel
@onready var intent_label: Label = $VBoxContainer/IntentLabel
@onready var energy_label: Label = $VBoxContainer/EnergyLabel
@onready var pile_label: Label = $VBoxContainer/PileLabel
@onready var play_count_label: Label = $VBoxContainer/PlayCountLabel
@onready var status_label: Label = $VBoxContainer/StatusLabel
@onready var card_container: HBoxContainer = $VBoxContainer/CardContainer
@onready var detail_title: Label = $VBoxContainer/DetailPanel/DetailMargin/DetailContent/DetailTitle
@onready var detail_stats: Label = $VBoxContainer/DetailPanel/DetailMargin/DetailContent/DetailStats
@onready var detail_description: Label = $VBoxContainer/DetailPanel/DetailMargin/DetailContent/DetailDescription
@onready var detail_keywords: Label = $VBoxContainer/DetailPanel/DetailMargin/DetailContent/DetailKeywords
@onready var noise_bar: ProgressBar = $NoiseBarContainer/NoiseBar
@onready var noise_label: Label = $NoiseBarContainer/NoiseLabel


const NO_SELECTION_HINT := "Select a card to see its details"

var selected_index := -1
var card_buttons: Array[Button] = []
# What the sentry did on its turn. Filled in while end_turn() is still
# running and read once it returns.
var sentry_turn: Dictionary = {}


func _ready() -> void:
	_draw_hand()
	_update_noise_label()


func _on_draw_button_pressed() -> void:
	_draw_hand()


func _on_end_turn_button_pressed() -> void:
	# Rust owns the whole turn boundary: discard what's left, hand the sentry
	# its phase, then deal the next turn's hand with refreshed energy. The
	# sentry reports back through _on_sentry_turn_resolved before this call
	# returns, so the labels below are already up to date.
	selected_index = -1
	status_label.text = ""
	var hand: PackedStringArray = draw_phase.end_turn()
	_rebuild_card_buttons(hand)
	_refresh_status_labels()

	var turn := sentry_turn
	sentry_turn = {}

	# A full noise meter means the player has been spotted, so the new hand
	# never gets played and the detection screen takes over instead.
	if turn.get("run_failed", false):
		_report_detected()
		return

	if turn.get("ok", false):
		status_label.text = "%s ran %s and added %d noise." % [
			turn.get("sentry", "?"),
			turn.get("action", ""),
			turn.get("noise_added", 0),
		]


# Connected in the scene to the sentry, which announces its turn from inside
# end_turn(). Nothing is drawn here because the hand for the next turn has not
# been dealt yet at this point.
func _on_sentry_turn_resolved(turn: Dictionary) -> void:
	sentry_turn = turn


func _report_detected() -> void:
	status_label.text = "%s has you. The noise meter is full and the run is over." % sentry.construct_name()
	var result: Dictionary = FlowCoordinator.report_caught()
	if not result.get("ok", false):
		push_error("Could not report detection: %s" % result.get("error", "unknown error"))


func _on_complete_encounter_pressed() -> void:
	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		push_error("Could not complete encounter: %s" % result.get("error", "unknown error"))


func _draw_hand() -> void:
	status_label.text = ""
	var hand: PackedStringArray = draw_phase.draw_hand()
	_rebuild_card_buttons(hand)
	_refresh_status_labels()


func _rebuild_card_buttons(names: PackedStringArray) -> void:
	# A rebuilt hand invalidates any previous selection.
	if selected_index >= names.size():
		selected_index = -1

	for child in card_container.get_children():
		child.queue_free()
	card_buttons.clear()

	var costs: PackedInt32Array = draw_phase.hand_costs()

	for i in names.size():
		var button := Button.new()
		button.text = "%s [%d]" % [names[i], costs[i]]
		button.toggle_mode = true
		button.disabled = costs[i] > draw_phase.energy
		button.pressed.connect(_on_card_clicked.bind(i))
		card_container.add_child(button)
		card_buttons.append(button)

	_update_selection_visuals()
	_update_card_detail()


func _on_card_clicked(index: int) -> void:
	selected_index = index
	_update_selection_visuals()
	_update_card_detail()


# Fills the detail panel from the selected card, including an explanation of
# every effect it carries so keywords do not have to be guessed at.
func _update_card_detail() -> void:
	if selected_index == -1:
		detail_title.text = NO_SELECTION_HINT
		detail_stats.text = ""
		detail_description.text = ""
		detail_keywords.text = ""
		return

	var detail: Dictionary = draw_phase.card_detail(selected_index)
	if not detail.get("ok", false):
		detail_title.text = NO_SELECTION_HINT
		detail_stats.text = ""
		detail_description.text = ""
		detail_keywords.text = ""
		return

	detail_title.text = "%s — %s · %s" % [
		detail.get("name", "?"),
		detail.get("type", "?"),
		detail.get("rarity", "?"),
	]
	detail_stats.text = "%s · %s" % [detail.get("cost_text", ""), detail.get("noise_text", "")]
	detail_description.text = str(detail.get("description", ""))

	var keyword_lines: Array[String] = []
	for keyword: Dictionary in detail.get("keywords", []):
		keyword_lines.append("%s — %s" % [keyword.get("label", "?"), keyword.get("explanation", "")])
	detail_keywords.text = "\n".join(keyword_lines)


func _update_selection_visuals() -> void:
	for i in card_buttons.size():
		card_buttons[i].button_pressed = (i == selected_index)


func _on_play_button_pressed() -> void:
	if selected_index == -1:
		return

	var played_name: String = draw_phase.play_card(selected_index)
	if played_name != "":
		play_count_label.text = "Cards played: %d" % draw_phase.play_count
		selected_index = -1
		status_label.text = ""
		_rebuild_card_buttons(draw_phase.hand_names())
	else:
		status_label.text = "Not enough energy to play that card."

	_refresh_status_labels()


func _refresh_status_labels() -> void:
	_update_turn_label()
	_update_noise_label()
	_update_intent_label()
	_update_pile_label()
	_update_energy_label()
	_update_noise_label()


func _update_turn_label() -> void:
	turn_label.text = "Turn %d" % draw_phase.turn_number


func _update_pile_label() -> void:
	pile_label.text = "Draw pile: %d | Discard pile: %d" % [
		draw_phase.draw_pile_count(),
		draw_phase.discard_pile_count(),
	]

func _update_noise_label() -> void:
	var current: int = NoiseMeterGlobal.noise
	var max_val: int = NoiseMeterGlobal.max_noise
	noise_bar.max_value = max_val
	noise_bar.value = current
	noise_label.text = "Noise: %d / %d" % [current, max_val]
	

func _update_energy_label() -> void:
	energy_label.text = "Energy: %d / %d" % [draw_phase.energy, draw_phase.max_energy]


func _update_noise_label() -> void:
	noise_label.text = "Noise: %d / %d" % [sentry.noise(), sentry.max_noise()]


# Shows what the sentry will do next, so ending the turn is a choice made with
# the threat in view rather than a coin flip.
func _update_intent_label() -> void:
	var action: String = sentry.queued_action_name()
	if action == "":
		intent_label.text = "%s: nothing queued" % sentry.construct_name()
	else:
		intent_label.text = "%s will: %s (+%d noise)" % [
			sentry.construct_name(),
			action,
			sentry.queued_action_noise(),
		]

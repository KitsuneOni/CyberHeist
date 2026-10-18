extends Control

@onready var draw_phase: Node = $DrawPhase
@onready var credits_label: Label = %CreditsLabel
@onready var sentry: Node = $Sentry
@onready var turn_label: Label = %TurnLabel
@onready var intent_label: Label = %IntentLabel
@onready var energy_label: Label = %EnergyLabel
@onready var energy_max_label: Label = %EnergyMaxLabel
@onready var pile_label: Label = %PileLabel
@onready var play_count_label: Label = %PlayCountLabel
@onready var status_label: Label = %StatusLabel
@onready var card_container: HBoxContainer = %CardContainer
@onready var detail_title: Label = %DetailTitle
@onready var detail_stats: Label = %DetailStats
@onready var detail_description: Label = %DetailDescription
@onready var detail_keywords: Label = %DetailKeywords
@onready var detection_meter: Control = %NoiseBarContainer
@onready var noise_bar: ProgressBar = %NoiseBar
@onready var noise_label: Label = %NoiseLabel
@onready var enemy_name: Label = %EnemyName
@onready var enemy_icon: TextureRect = %EnemyIcon
@onready var integrity_bar: ProgressBar = %IntegrityBar
@onready var health_label: Label = %HealthLabel
@onready var shield_label: Label = %ShieldLabel
@onready var knowledge_label: Label = %KnowledgeLabel
@onready var corruption_chip: Control = %CorruptionChip
@onready var corruption_label: Label = %CorruptionLabel
@onready var complete_encounter_button: Button = %CompleteEncounterButton

const CardTooltip := preload("res://scenes/card_tooltip.gd")

const NO_SELECTION_HINT := "Select a card to see its details"
const FINAL_BOSS_SHORTCUT_HINT := "The final boss has to be beaten to win the contract."
# Marks an action security takes from its alert set, on the intent and in the
# turn report, so the player can tell loud play is what changed it.
const ESCALATED_MARK := " [ESCALATED]"

# The terminal palette shared with the main menu and the shop.
const GREEN := Color(0.2, 1, 0.4, 1)
const RED := Color(1, 0.2, 0.333333, 1)
const CYAN := Color(0, 0.898, 1, 1)
const CARD_BG := Color(0.0509804, 0.0862745, 0.0705882, 1)

const CARD_SIZE := Vector2(124, 172)
# How far a card rises out of the hand when hovered or selected.
const CARD_LIFT := 14.0

# Art for each card type until real card art exists, tinted green like every
# other icon on the terminal screens.
const TYPE_ICONS := {
	"Attack": "res://assets/ui/icons/game-icons/virus.svg",
	"Defence": "res://assets/ui/icons/tabler/shield.svg",
	"Skill": "res://assets/ui/icons/game-icons/cpu.svg",
	"Recon": "res://assets/ui/icons/game-icons/radar-sweep.svg",
	"Social Engineering": "res://assets/ui/icons/game-icons/domino-mask.svg",
}
const FALLBACK_CARD_ICON := "res://assets/ui/icons/game-icons/circuitry.svg"

# Plate art by construct. Anything not listed is standard security.
const ENEMY_ICONS := {
	"BASTION": "res://assets/ui/icons/game-icons/robot-golem.svg",
	"HYDRA": "res://assets/ui/icons/game-icons/mesh-network.svg",
	"CERBERUS": "res://assets/ui/icons/game-icons/sentry-gun.svg",
	"ICEBREAKER": "res://assets/ui/icons/game-icons/security-gate.svg",
	"BLACK MONOLITH": "res://assets/ui/icons/game-icons/server-rack.svg",
	"THE ARCHITECT": "res://assets/ui/icons/game-icons/cyber-eye.svg",
}
const STANDARD_ENEMY_ICON := "res://assets/ui/icons/game-icons/cctv-camera.svg"

var selected_index := -1
var card_buttons: Array[Button] = []
# The name and cost shown on each card face, in hand order.
var card_name_labels: Array[Label] = []
var card_cost_labels: Array[Label] = []
# Shows a card's full description while the mouse is over it (story #14).
# Made in _ready rather than here, so a screen freed before it is ever shown
# leaves nothing orphaned behind.
var card_tooltip: PanelContainer
# What the sentry did on its turn. Filled in while end_turn() is still
# running and read once it returns.
var sentry_turn: Dictionary = {}
# The phase the player has been told about, so an escalation is announced once
# whichever way it happens: a card on their turn, or corruption on the boss's.
var shown_phase := 1
# The deck viewer and the button that opens it (story #55). Made in _ready
# rather than here, so a screen freed before it is ever shown leaves nothing
# orphaned behind.
var view_deck_button: Button
var deck_viewer: Control


func _ready() -> void:
	draw_phase.apply_run_bonuses()
	card_tooltip = CardTooltip.new()
	add_child(card_tooltip)
	shown_phase = sentry.phase()
	_disable_shortcut_on_final_boss()
	_draw_hand()
	_add_deck_viewer()


# A Deck button above Play Card, and the viewer it opens, for checking the
# whole run deck mid-fight (story #55). Styled like the other turn buttons.
func _add_deck_viewer() -> void:
	deck_viewer = preload("res://scenes/deck_viewer.gd").new()
	deck_viewer.name = "DeckViewer"
	add_child(deck_viewer)

	var end_turn: Button = %EndTurnButton
	view_deck_button = Button.new()
	view_deck_button.name = "ViewDeckButton"
	view_deck_button.text = "DECK (%d)" % FlowCoordinator.run_deck_size()
	view_deck_button.theme_type_variation = end_turn.theme_type_variation
	view_deck_button.add_theme_font_size_override("font_size", 16)
	for state in ["normal", "hover", "focus", "pressed", "hover_pressed", "disabled"]:
		view_deck_button.add_theme_stylebox_override(state, end_turn.get_theme_stylebox(state))
	view_deck_button.pressed.connect(deck_viewer.open)
	var turn_block := end_turn.get_parent()
	turn_block.add_child(view_deck_button)
	turn_block.move_child(view_deck_button, 0)
	deck_viewer.closed.connect(view_deck_button.grab_focus)


# Skipping the final boss would win the whole contract reward without a fight,
# so the testing shortcut is off for it. Every other encounter keeps it.
func _disable_shortcut_on_final_boss() -> void:
	if not FlowCoordinator.active_encounter_wins_contract():
		return
	complete_encounter_button.disabled = true
	complete_encounter_button.tooltip_text = FINAL_BOSS_SHORTCUT_HINT


func _on_end_turn_button_pressed() -> void:
	if not _can_act():
		return

	selected_index = -1
	status_label.text = ""
	var hand: PackedStringArray = draw_phase.end_turn()

	var turn := sentry_turn
	sentry_turn = {}

	# A boss lockdown takes energy off the turn end_turn() has just opened, not
	# the one that was spent getting here. Applied before the hand is drawn on
	# screen, so cards the player can no longer afford come up disabled rather
	# than failing when clicked.
	var drained := 0
	if turn.get("ok", false):
		drained = draw_phase.drain_energy(int(turn.get("energy_drain", 0)))

	_rebuild_card_buttons(hand)
	_refresh_status_labels()

	# A full noise meter means the player has been spotted, so the new hand
	# never gets played and the detection screen takes over instead.
	if turn.get("run_failed", false):
		_report_detected()
		return

	# Corruption can kill the sentry during its own phase, on a turn where
	# no card was played — this must be checked here too, not only after a
	# card play, or a corruption kill at end of turn goes unnoticed.
	if turn.get("defeated", false):
		_report_sentry_defeated()
		return

	if turn.get("ok", false):
		var action: String = turn.get("action", "")
		if turn.get("alert", false):
			action += ESCALATED_MARK
		var message: String = (
			"%s ran %s and added %d noise."
			% [
				turn.get("sentry", "?"),
				action,
				turn.get("noise_added", 0),
			]
		)
		if drained > 0:
			message += " Lockdown cost you %d energy this turn." % drained
		var restored: int = turn.get("integrity_restored", 0)
		if restored > 0:
			message += " Reinforce restored %d integrity." % restored
		var purged: int = turn.get("corruption_purged", 0)
		if purged > 0:
			message += " Purge cleared %d corruption." % purged
		var firewall: int = turn.get("firewall", 0)
		if firewall > 0:
			message += " Firewall will block the next %d damage." % firewall
		var malware: int = turn.get("malware_planted", 0)
		if malware > 0:
			message += " %d Injected Malware added to your discard pile." % malware
		var corruption_damage: int = turn.get("corruption_damage", 0)
		if corruption_damage > 0:
			message += " Corruption dealt %d damage." % corruption_damage
		status_label.text = message + _escalation_message()
	else:
		var corruption_damage: int = turn.get("corruption_damage", 0)
		if corruption_damage > 0:
			status_label.text = "Corruption dealt %d damage." % corruption_damage
		status_label.text = (status_label.text + _escalation_message()).strip_edges()


# Connected in the scene to the sentry, which announces its turn from inside
# end_turn(). Nothing is drawn here because the hand for the next turn has not
# been dealt yet at this point.
func _on_sentry_turn_resolved(turn: Dictionary) -> void:
	sentry_turn = turn


# Says so the first time the sentry is seen in a new phase, and nothing after.
# The intent line then carries the phase for as long as it lasts.
func _escalation_message() -> String:
	var phase: int = sentry.phase()
	if phase == shown_phase:
		return ""
	shown_phase = phase
	return " %s escalates to phase %d: new action set." % [sentry.construct_name(), phase]


func _report_detected() -> void:
	status_label.text = (
		"%s has you. The noise meter is full and the run is over." % sentry.construct_name()
	)
	var result: Dictionary = FlowCoordinator.report_caught()
	if not result.get("ok", false):
		push_error("Could not report detection: %s" % result.get("error", "unknown error"))


# Testing aid, not a gameplay feature: the Complete Encounter button skips the
# fight without beating the construct, so a playtest can get past a node
# quickly. It goes through the same completion as a real win, which means it
# also collects that encounter's full payout, including an elite or boss's
# credits and guaranteed Rare. Remove the button, or stop it paying out,
# before this is treated as a real build.
#
# It never completes the final boss, which would win the contract and its
# reward. The button is disabled there, and this check also stops a press that
# arrives anyway, such as a test emitting the signal.
func _on_complete_encounter_pressed() -> void:
	if not _can_act():
		return
	if FlowCoordinator.active_encounter_wins_contract():
		status_label.text = FINAL_BOSS_SHORTCUT_HINT
		return

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

	# The card under the mouse may be about to go, and a freed button never
	# sends its mouse_exited.
	card_tooltip.hide_tooltip()
	for child in card_container.get_children():
		child.queue_free()
	card_buttons.clear()
	card_name_labels.clear()
	card_cost_labels.clear()

	var costs: PackedInt32Array = draw_phase.hand_costs()

	for i in names.size():
		var detail: Dictionary = draw_phase.card_detail(i)
		var button := _build_card(names[i], costs[i], detail)
		button.disabled = costs[i] > draw_phase.energy
		button.pressed.connect(_on_card_clicked.bind(i))
		# Unaffordable cards are disabled but still report hover, so the player
		# can read a card they can't play yet.
		button.mouse_entered.connect(_on_card_hovered.bind(i))
		button.mouse_exited.connect(_on_card_unhovered.bind(i))
		card_buttons.append(button)

	_update_selection_visuals()
	_update_card_detail()


# What the face of the card at `index` reads: its name and its cost.
func card_face_text(index: int) -> String:
	return "%s [%s]" % [card_name_labels[index].text, card_cost_labels[index].text]


# One card in the hand, laid out like the card anatomy sheet: cost pip and
# name on top, an art window, the type banner, and the noise it makes along the
# bottom in red. The button sits in a slot taller than itself so hovering or
# selecting can lift it out of the hand without shifting its neighbours.
func _build_card(card_name: String, cost: int, detail: Dictionary) -> Button:
	var slot := Control.new()
	slot.custom_minimum_size = Vector2(CARD_SIZE.x, CARD_SIZE.y + CARD_LIFT)
	slot.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card_container.add_child(slot)

	var button := Button.new()
	button.toggle_mode = true
	button.focus_mode = Control.FOCUS_NONE
	button.size = CARD_SIZE
	button.position = Vector2(0, CARD_LIFT)
	button.add_theme_stylebox_override("normal", _card_style(GREEN, 0.45, 1, 0.0))
	button.add_theme_stylebox_override("hover", _card_style(GREEN, 0.9, 2, 0.06))
	button.add_theme_stylebox_override("pressed", _card_style(GREEN, 1.0, 3, 0.14))
	button.add_theme_stylebox_override("hover_pressed", _card_style(GREEN, 1.0, 3, 0.14))
	button.add_theme_stylebox_override("disabled", _card_style(GREEN, 0.18, 1, 0.0))
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	slot.add_child(button)

	var face := VBoxContainer.new()
	face.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	face.offset_left = 8
	face.offset_top = 6
	face.offset_right = -8
	face.offset_bottom = -6
	face.add_theme_constant_override("separation", 4)
	face.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(face)

	var top := HBoxContainer.new()
	top.add_theme_constant_override("separation", 6)
	top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	face.add_child(top)

	var pip := PanelContainer.new()
	pip.custom_minimum_size = Vector2(26, 26)
	pip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var ring := StyleBoxFlat.new()
	ring.bg_color = CARD_BG
	ring.set_border_width_all(2)
	ring.border_color = GREEN
	ring.set_corner_radius_all(13)
	pip.add_theme_stylebox_override("panel", ring)
	top.add_child(pip)

	var cost_label := _card_label(str(cost), 16)
	cost_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	cost_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	pip.add_child(cost_label)
	card_cost_labels.append(cost_label)

	var name_label := _card_label(card_name, 12)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	name_label.uppercase = true
	top.add_child(name_label)
	card_name_labels.append(name_label)

	var art := PanelContainer.new()
	art.size_flags_vertical = Control.SIZE_EXPAND_FILL
	art.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var art_style := StyleBoxFlat.new()
	art_style.bg_color = Color(GREEN, 0.05)
	art_style.set_border_width_all(1)
	art_style.border_color = Color(GREEN, 0.3)
	art.add_theme_stylebox_override("panel", art_style)
	face.add_child(art)

	var card_type: String = str(detail.get("type", ""))
	var icon := TextureRect.new()
	icon.texture = load(TYPE_ICONS.get(card_type, FALLBACK_CARD_ICON))
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.custom_minimum_size = Vector2(0, 48)
	icon.modulate = Color(GREEN, 0.8)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	art.add_child(icon)

	var banner := _card_label(card_type, 11)
	banner.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	banner.uppercase = true
	banner.clip_text = true
	var banner_style := StyleBoxFlat.new()
	banner_style.bg_color = Color(GREEN, 0.14)
	banner.add_theme_stylebox_override("normal", banner_style)
	face.add_child(banner)

	var noise := _card_label(_noise_tag(str(detail.get("noise_text", ""))), 12)
	noise.add_theme_color_override("font_color", _noise_tag_colour(str(detail.get("noise_text", ""))))
	face.add_child(noise)

	return button


func _card_label(text: String, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return label


func _card_style(colour: Color, alpha: float, border: int, fill: float) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = CARD_BG.lerp(colour, fill)
	style.set_border_width_all(border)
	style.border_color = Color(colour, alpha)
	return style


# The short noise line along the bottom of a card, from the readout's wording:
# "Generates 5 noise" reads "noise +5", "Reduces noise by 10" reads "noise -10".
func _noise_tag(noise_text: String) -> String:
	var amount := noise_text.to_int()
	if noise_text.begins_with("Generates"):
		return "noise +%d" % amount
	if noise_text.begins_with("Reduces"):
		return "noise -%d" % amount
	return "silent"


func _noise_tag_colour(noise_text: String) -> Color:
	if noise_text.begins_with("Generates"):
		return RED
	if noise_text.begins_with("Reduces"):
		return CYAN
	return Color(GREEN, 0.55)


func _on_card_clicked(index: int) -> void:
	selected_index = index
	_update_selection_visuals()
	_update_card_detail()


func _on_card_hovered(index: int) -> void:
	if index >= card_buttons.size():
		return
	card_tooltip.show_for(card_buttons[index], draw_phase.card_detail(index))
	if not card_buttons[index].disabled:
		_lift_card(index, true)


func _on_card_unhovered(index: int) -> void:
	if index >= card_buttons.size():
		return
	card_tooltip.hide_for(card_buttons[index])
	_lift_card(index, index == selected_index)


func _lift_card(index: int, lifted: bool) -> void:
	var button := card_buttons[index]
	if not is_instance_valid(button):
		return
	button.position.y = 0.0 if lifted else CARD_LIFT


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

	detail_title.text = CardTooltip.title_for(detail)
	detail_stats.text = CardTooltip.stats_for(detail)
	detail_description.text = str(detail.get("description", ""))
	detail_keywords.text = CardTooltip.keywords_for(detail)


func _update_selection_visuals() -> void:
	for i in card_buttons.size():
		card_buttons[i].button_pressed = (i == selected_index)
		_lift_card(i, i == selected_index)


# Screen destruction is deferred. Ignore queued input on a combat screen whose
# encounter has already ended, including the frame in which a card fills noise.
func _can_act() -> bool:
	return (
		not is_queued_for_deletion()
		and FlowCoordinator.run_snapshot().get("phase", "") == "encounter_active"
	)


func _on_play_button_pressed() -> void:
	if not _can_act() or selected_index == -1:
		return

	var result: Dictionary = draw_phase.play_card(selected_index)
	if result.get("ok", false):
		# Every card type has its own sound, so the play is heard as well as
		# read. DrawPhase reports which one this card makes.
		AudioDirector.play_card_sound(str(result.get("sound_cue", "")))
		play_count_label.text = "cards played: %d" % draw_phase.play_count
		selected_index = -1
		var noise_change: int = result.get("noise_change", 0)
		var damage_dealt: int = result.get("damage_dealt", 0)
		var damage_blocked: int = result.get("damage_blocked", 0)
		var shield_added: int = result.get("shield_added", 0)
		var cards_drawn: int = result.get("cards_drawn", 0)
		var knowledge_change: int = result.get("knowledge_change", 0)
		var max_energy_gained: int = result.get("max_energy_gained", 0)
		var corruption_added: int = result.get("corruption_added", 0)
		var corruption_boost_added: int = result.get("corruption_boost_added", 0)
		var energy_tax_paid: int = result.get("energy_tax_paid", 0)

		var message: String = (
			"%s played. Noise change: %+d."
			% [
				result.get("name", "Card"),
				noise_change,
			]
		)
		if damage_dealt > 0:
			message += " Damage: %d." % damage_dealt
		if damage_blocked > 0:
			message += " Firewall blocked %d." % damage_blocked
		if shield_added > 0:
			message += " Shield: +%d." % shield_added
		if cards_drawn > 0:
			message += " Drew %d card(s)." % cards_drawn
		if knowledge_change != 0:
			message += " Knowledge: %+d." % knowledge_change
		if max_energy_gained > 0:
			message += " Max energy +%d!" % max_energy_gained
		if corruption_added > 0:
			message += " Corruption +%d." % corruption_added
		if energy_tax_paid > 0:
			message += " (+%d energy tax)" % energy_tax_paid
		if corruption_boost_added > 0:
			message += " Corruption Boost +%d (this turn)." % corruption_boost_added
		# Exhausted cards, such as security's Injected Malware, are gone for
		# the rest of the fight rather than waiting in the discard pile.
		if result.get("exhausted", false):
			message += " %s leaves the encounter." % result.get("name", "Card")
		status_label.text = message + _escalation_message()

		_rebuild_card_buttons(draw_phase.hand_names())
	else:
		match result.get("error", ""):
			"not_enough_energy":
				status_label.text = "Not enough energy. Choose a cheaper card or end your turn."
			"invalid_index":
				status_label.text = "That card is no longer in your hand. Select another card."

	_refresh_status_labels()

	if sentry.is_defeated():
		_report_sentry_defeated()
		return

	if NoiseMeterGlobal.is_at_cap():
		_report_detected()


func _report_sentry_defeated() -> void:
	status_label.text = "%s is down. Encounter cleared." % sentry.construct_name()
	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		push_error("Could not complete encounter: %s" % result.get("error", "unknown error"))


func _update_health_label() -> void:
	health_label.text = (
		"%s: %d / %d HP" % [sentry.construct_name(), sentry.health(), sentry.max_health()]
	)
	# Only shown while one is up, so the player can see how much plain damage
	# is still going to be soaked up before integrity.
	var firewall: int = sentry.firewall()
	if firewall > 0:
		health_label.text += "  [Firewall %d]" % firewall

	integrity_bar.max_value = maxi(sentry.max_health(), 1)
	integrity_bar.value = sentry.health()

	var who: String = sentry.construct_name()
	if sentry.phase() > 1:
		who += " // PHASE %d" % sentry.phase()
	enemy_name.text = who
	enemy_icon.texture = load(ENEMY_ICONS.get(sentry.construct_name(), STANDARD_ENEMY_ICON))


func _update_knowledge_label() -> void:
	knowledge_label.text = (
		"knowledge %d / %d"
		% [
			KnowledgeMeterGlobal.knowledge,
			KnowledgeMeterGlobal.max_knowledge,
		]
	)


func _refresh_status_labels() -> void:
	_update_credits_label()
	_update_turn_label()
	_update_intent_label()
	_update_pile_label()
	_update_energy_label()
	_update_noise_label()
	_update_health_label()
	_update_shield_label()
	_update_knowledge_label()
	_update_corruption_label()


# Refreshed alongside everything else, so spending or earning shows up the
# moment it happens rather than only back on the map.
func _update_credits_label() -> void:
	credits_label.text = "$ %d  credits on hand" % PlayerStateGlobal.money()


func _update_turn_label() -> void:
	turn_label.text = "TURN %d" % draw_phase.turn_number


func _update_pile_label() -> void:
	pile_label.text = (
		"draw %d · discard %d"
		% [
			draw_phase.draw_pile_count(),
			draw_phase.discard_pile_count(),
		]
	)


func _update_corruption_label() -> void:
	var corruption: int = sentry.corruption()
	corruption_label.text = "corruption %d" % corruption
	corruption_chip.visible = corruption > 0


func _update_shield_label() -> void:
	shield_label.text = "shield %d" % NoiseMeterGlobal.shield


func _update_noise_label() -> void:
	var current: int = NoiseMeterGlobal.noise
	var max_val: int = NoiseMeterGlobal.max_noise
	noise_bar.max_value = max_val
	noise_bar.value = current
	noise_label.text = "Noise: %d / %d" % [current, max_val]
	detection_meter.show_level(current, max_val)

	# Only worth saying when there is some: an unhardened construct reads as
	# the plain meter it has always been.
	var resistance: int = NoiseMeterGlobal.get_resistance_percent()
	if resistance > 0:
		noise_label.text += "  (recovery -%d%%)" % resistance


func _update_energy_label() -> void:
	energy_label.text = str(draw_phase.energy)
	energy_max_label.text = "energy %d / %d" % [draw_phase.energy, draw_phase.max_energy]


# Shows what the sentry will do next, so ending the turn is a choice made with
# the threat in view rather than a coin flip.
func _update_intent_label() -> void:
	var action: String = sentry.queued_action_name()
	if action == "":
		intent_label.text = "%s: nothing queued" % sentry.construct_name()
		return

	# A second phase is called out on the intent itself, so the player can see
	# the boss has changed what it does, not only that this action is new.
	var who: String = sentry.construct_name()
	if sentry.phase() > 1:
		who += " [PHASE %d]" % sentry.phase()

	# Loud play puts security on alert. The intent is read again after every
	# card, so it flips the moment noise crosses the line, either way.
	var escalated: bool = sentry.queued_action_escalated()
	if escalated:
		action += ESCALATED_MARK
	intent_label.add_theme_color_override("font_color", RED if escalated else CYAN)

	# An ability is announced with the intent rather than sprung afterwards, so
	# ending the turn into a lockdown is a decision and not a surprise.
	var ability: String = sentry.queued_action_ability()
	if ability == "":
		intent_label.text = (
			"%s will: %s (+%d noise)"
			% [
				who,
				action,
				sentry.queued_action_noise(),
			]
		)
	else:
		intent_label.text = (
			"%s will: %s (+%d noise, %s)"
			% [
				who,
				action,
				sentry.queued_action_noise(),
				ability,
			]
		)

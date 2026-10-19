extends Control

@onready var draw_phase: Node = $DrawPhase
@onready var credits_label: Label = %CreditsLabel
@onready var sentry: Node = $Sentry
@onready var turn_label: Label = %TurnLabel
@onready var intent_label: Label = %IntentLabel
@onready var intent_icon: TextureRect = %IntentIcon
@onready var intent_value: Label = %IntentValue
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
@onready var might_chip: Control = %MightChip
@onready var might_label: Label = %MightLabel

const CardRarity := preload("res://scenes/card_rarity.gd")
const CardTooltip := preload("res://scenes/card_tooltip.gd")
const CombatFx := preload("res://scenes/combat_fx.gd")

const NO_SELECTION_HINT := "Select a card to see its details"
const FINAL_BOSS_SHORTCUT_HINT := "The final boss has to be beaten to win the contract."
const ALLOW_FINAL_BOSS_SKIP := false
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
# How long a new hand waits for the old one to start flying off before it
# deals in.
const FLY_AND_DEAL_GAP := 0.15

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

# The intent icon for each kind of move security can queue (story #103), and
# how its number reads beside it. Purge has no number.
const INTENT_ICONS := {
	"noise": "res://assets/ui/icons/tabler/eye.svg",
	"drain_energy": "res://assets/ui/icons/tabler/bolt.svg",
	"restore_integrity": "res://assets/ui/icons/tabler/arrow-up-circle.svg",
	"purge": "res://assets/ui/icons/tabler/trash.svg",
	"firewall": "res://assets/ui/icons/game-icons/firewall.svg",
	"inject_malware": "res://assets/ui/icons/tabler/bug.svg",
}
# Widest the intent line gets before it wraps, so a boss's long ability text
# stays clear of the card readout beside it.
const INTENT_MAX_WIDTH := 520.0
const INTENT_VALUE_FORMATS := {
	"noise": "+%d",
	"drain_energy": "-%d",
	"restore_integrity": "+%d",
	"purge": "",
	"firewall": "%d",
	"inject_malware": "+%d",
}

var selected_index := -1
var card_buttons: Array[Button] = []
# The name and cost shown on each card face, in hand order.
var card_name_labels: Array[Label] = []
var card_cost_labels: Array[Label] = []
# Shows a card's full description while the mouse is over it (story #14).
# Made in _ready rather than here, so a screen freed before it is ever shown
# leaves nothing orphaned behind.
var card_tooltip: PanelContainer
# Card flights, the hand dealing in, hits and floating numbers (story #45).
# Made in _ready for the same reason as the tooltip.
var fx: Control
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
# Set once _ready has finished. is_node_ready() is already true inside _ready,
# so it cannot tell the first build apart from later updates.
var _built := false


func _ready() -> void:
	draw_phase.apply_run_bonuses()
	# Before the tooltip, so a card's tooltip draws over its flying ghost.
	fx = CombatFx.new()
	add_child(fx)
	card_tooltip = CardTooltip.new()
	add_child(card_tooltip)
	shown_phase = sentry.phase()
	_disable_shortcut_on_final_boss()
	_draw_hand()
	_add_deck_viewer()
	_built = true


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
	if ALLOW_FINAL_BOSS_SKIP or not FlowCoordinator.active_encounter_wins_contract():
		return
	complete_encounter_button.disabled = true
	complete_encounter_button.tooltip_text = FINAL_BOSS_SHORTCUT_HINT

func _update_might_label() -> void:
	var might: int = draw_phase.might()
	might_label.text = "might %d" % might
	might_chip.visible = might > 0
	
func _on_end_turn_button_pressed() -> void:
	if not _can_act():
		return

	selected_index = -1
	status_label.text = ""
	var energy_before: int = draw_phase.energy
	var noise_before: int = NoiseMeterGlobal.noise
	var health_before: int = sentry.health()
	_discard_hand_on_screen()
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
	_deal_hand_in(FLY_AND_DEAL_GAP)
	_refresh_status_labels()
	_show_sentry_turn(turn, noise_before, health_before, drained)
	if draw_phase.energy != energy_before:
		fx.punch(energy_label)

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
	if not ALLOW_FINAL_BOSS_SKIP and FlowCoordinator.active_encounter_wins_contract():
		status_label.text = FINAL_BOSS_SHORTCUT_HINT
		return

	var result: Dictionary = FlowCoordinator.complete_active_encounter()
	if not result.get("ok", false):
		push_error("Could not complete encounter: %s" % result.get("error", "unknown error"))


func _draw_hand() -> void:
	status_label.text = ""
	var hand: PackedStringArray = draw_phase.draw_hand()
	_rebuild_card_buttons(hand)
	_deal_hand_in(0.0)
	_refresh_status_labels()


# Every card in hand rises into place from below, left to right.
func _deal_hand_in(delay: float) -> void:
	for i in card_buttons.size():
		fx.deal_in(card_buttons[i], _rest_height(i), delay + i * CombatFx.DEAL_STAGGER)


# The hand about to be replaced flies off to the discard pile, so ending the
# turn is seen to throw it away. Ghosts, because the real cards are about to
# be freed.
func _discard_hand_on_screen() -> void:
	var pile := pile_label.get_global_rect().get_center()
	for i in card_buttons.size():
		fx.fly_card(card_buttons[i], pile, i * CombatFx.DEAL_STAGGER * 0.5, 0.4)


# Where each card in hand sits on screen, so a rebuilt hand can slide the
# cards that stayed from there to their new places.
func _card_screen_positions() -> Array[float]:
	var positions: Array[float] = []
	for button in card_buttons:
		positions.append(button.global_position.x if is_instance_valid(button) else 0.0)
	return positions


# After a card at `played` leaves the hand, the cards either side of it close
# the gap from where they were, and any card the play drew is dealt in. The
# new layout is only known once the hand has been sorted, so it waits for that.
func _close_hand_gap(old_positions: Array[float], played: int) -> void:
	# Two plays before the hand is next sorted: the later hand is the one on
	# screen, so only its slide is kept.
	if card_container.sort_children.is_connected(_slide_hand_into_place):
		card_container.sort_children.disconnect(_slide_hand_into_place)
	card_container.sort_children.connect(
		_slide_hand_into_place.bind(card_buttons.duplicate(), old_positions, played),
		CONNECT_ONE_SHOT
	)


func _slide_hand_into_place(buttons: Array, old_positions: Array[float], played: int) -> void:
	var dealt := 0
	for i in buttons.size():
		var button: Button = buttons[i]
		if not is_instance_valid(button):
			continue
		var was := i if i < played else i + 1
		if was < old_positions.size():
			fx.slide_from(button, old_positions[was])
		else:
			fx.deal_in(button, _rest_height(i), FLY_AND_DEAL_GAP + dealt * CombatFx.DEAL_STAGGER)
			dealt += 1


func _rebuild_card_buttons(names: PackedStringArray) -> void:
	# A rebuilt hand invalidates any previous selection.
	if selected_index >= names.size():
		selected_index = -1

	# The card under the mouse may be about to go, and a freed button never
	# sends its mouse_exited.
	card_tooltip.hide_tooltip()
	# Taken out now rather than left for queue_free, so the new hand is laid
	# out on its own and the gap-closing slide measures the right places.
	for child in card_container.get_children():
		card_container.remove_child(child)
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
	# The frame and cost ring are drawn in the card's rarity colour (#109).
	var frame := CardRarity.colour_for(str(detail.get("rarity", "")))
	var slot := Control.new()
	slot.custom_minimum_size = Vector2(CARD_SIZE.x, CARD_SIZE.y + CARD_LIFT)
	slot.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card_container.add_child(slot)

	var button := Button.new()
	button.toggle_mode = true
	button.focus_mode = Control.FOCUS_NONE
	button.size = CARD_SIZE
	button.position = Vector2(0, CARD_LIFT)
	button.add_theme_stylebox_override("normal", _card_style(frame, 0.85, 1, 0.0))
	button.add_theme_stylebox_override("hover", _card_style(frame, 0.95, 2, 0.06))
	button.add_theme_stylebox_override("pressed", _card_style(frame, 1.0, 3, 0.14))
	button.add_theme_stylebox_override("hover_pressed", _card_style(frame, 1.0, 3, 0.14))
	button.add_theme_stylebox_override("disabled", _card_style(frame, 0.22, 1, 0.0))
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
	ring.border_color = frame
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
	_update_selection_visuals(true)
	_update_card_detail()


func _on_card_hovered(index: int) -> void:
	if index >= card_buttons.size():
		return
	card_tooltip.show_for(card_buttons[index], draw_phase.card_detail(index))
	if not card_buttons[index].disabled:
		_lift_card(index, true, true)


func _on_card_unhovered(index: int) -> void:
	if index >= card_buttons.size():
		return
	card_tooltip.hide_for(card_buttons[index])
	_lift_card(index, index == selected_index, true)


# Raises or lowers a card. `animate` eases it there, as the mouse does; a
# rebuilt hand is placed at once and leaves the motion to the deal.
func _lift_card(index: int, lifted: bool, animate := false) -> void:
	var button := card_buttons[index]
	if not is_instance_valid(button):
		return
	var y := 0.0 if lifted else CARD_LIFT
	if animate:
		fx.lift_to(button, y)
	else:
		button.position.y = y


# The height a card rests at while not hovered: up if selected.
func _rest_height(index: int) -> float:
	return 0.0 if index == selected_index else CARD_LIFT


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
	detail_title.add_theme_color_override(
		"font_color", CardRarity.colour_for(str(detail.get("rarity", "")))
	)
	detail_stats.text = CardTooltip.stats_for(detail)
	detail_description.text = str(detail.get("description", ""))
	detail_keywords.text = CardTooltip.keywords_for(detail)


func _update_selection_visuals(animate := false) -> void:
	for i in card_buttons.size():
		card_buttons[i].button_pressed = (i == selected_index)
		_lift_card(i, i == selected_index, animate)


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

	var played := selected_index
	var played_button: Button = card_buttons[played] if played < card_buttons.size() else null
	var energy_before: int = draw_phase.energy
	var knowledge_before: int = KnowledgeMeterGlobal.knowledge
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
		var firewall_stripped: int = result.get("firewall_stripped", 0)
		var might_gained: int = result.get("might_gained", 0)
		var corrupting_discount_gained: int = result.get("corrupting_discount_gained", 0)
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
		if firewall_stripped > 0:
			message += " Firewall stripped: %d." % firewall_stripped
		if might_gained > 0:
			message += " Might +%d (this combat)." % might_gained
		if corrupting_discount_gained > 0:
			message += " Corrupting cards cost %d less (this combat)." % corrupting_discount_gained
		# Exhausted cards, such as security's Injected Malware, are gone for
		# the rest of the fight rather than waiting in the discard pile.
		if result.get("exhausted", false):
			message += " %s leaves the encounter." % result.get("name", "Card")
		status_label.text = message + _escalation_message()

		_show_card_played(result, played_button, energy_before, knowledge_before)
		var old_positions := _card_screen_positions()
		_rebuild_card_buttons(draw_phase.hand_names())
		_close_hand_gap(old_positions, played)
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


# What a successful play looks like: the card flies to what it hit, and every
# number it changed says so where that number is shown.
func _show_card_played(
	result: Dictionary, button: Button, energy_before: int, knowledge_before: int
) -> void:
	var damage_dealt: int = result.get("damage_dealt", 0)
	var damage_blocked: int = result.get("damage_blocked", 0)
	var firewall_stripped: int = result.get("firewall_stripped", 0)
	if firewall_stripped > 0:
		fx.flash(enemy_icon, CYAN)
		fx.float_text("-%d firewall" % firewall_stripped, enemy_icon, CYAN)
	var shield_added: int = result.get("shield_added", 0)
	var noise_change: int = result.get("noise_change", 0)
	var corruption_added: int = result.get("corruption_added", 0)

	if result.get("exhausted", false):
		fx.dissolve_card(button)
	elif damage_dealt > 0 or damage_blocked > 0 or corruption_added > 0:
		fx.fly_card(button, enemy_icon.get_global_rect().get_center())
	elif shield_added > 0:
		fx.fly_card(button, shield_label.get_global_rect().get_center())
	else:
		fx.fly_card(button, pile_label.get_global_rect().get_center(), 0.0, 0.4)

	if damage_dealt > 0:
		fx.hit(enemy_icon)
		fx.float_text("-%d" % damage_dealt, enemy_icon, RED)
	if damage_blocked > 0:
		fx.float_text("BLOCKED %d" % damage_blocked, enemy_icon, CYAN)
	if corruption_added > 0:
		fx.flash(enemy_icon, Color(GREEN, 1.0))
		fx.float_text("+%d corruption" % corruption_added, enemy_icon, GREEN)
	if shield_added > 0:
		fx.punch(shield_label)
		fx.float_text("+%d shield" % shield_added, shield_label, CYAN)
	if noise_change != 0:
		_show_noise_change(noise_change)
	var knowledge_change: int = KnowledgeMeterGlobal.knowledge - knowledge_before
	if knowledge_change != 0:
		fx.punch(knowledge_label)
		fx.float_text("%+d knowledge" % knowledge_change, knowledge_label, CYAN)
	if draw_phase.energy != energy_before or result.get("max_energy_gained", 0) > 0:
		fx.punch(energy_label)


# What security's turn did, shown where it landed.
func _show_sentry_turn(turn: Dictionary, noise_before: int, health_before: int, drained: int) -> void:
	var noise_change: int = NoiseMeterGlobal.noise - noise_before
	if noise_change != 0:
		_show_noise_change(noise_change)
	var health_change: int = sentry.health() - health_before
	if health_change < 0:
		fx.hit(enemy_icon)
		fx.float_text("%d" % health_change, enemy_icon, RED)
	elif health_change > 0:
		fx.float_text("+%d" % health_change, enemy_icon, GREEN)
	if int(turn.get("firewall", 0)) > 0:
		fx.flash(enemy_icon, CYAN)
	if drained > 0:
		fx.float_text("-%d energy" % drained, energy_label, RED)


# Noise going up is the danger, so it reads red; going down reads cyan.
func _show_noise_change(change: int) -> void:
	fx.punch(noise_label)
	fx.float_text("%+d noise" % change, noise_label, RED if change > 0 else CYAN)


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
	# Set outright while the screen is first being built, so it opens full
	# rather than filling up.
	if _built:
		fx.ease_bar(integrity_bar, sentry.health())
	else:
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
	_fit_intent_label()
	_update_pile_label()
	_update_energy_label()
	_update_noise_label()
	_update_health_label()
	_update_shield_label()
	_update_knowledge_label()
	_update_corruption_label()
	_update_might_label()


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
	_update_intent_icon()
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
	intent_icon.modulate = RED if escalated else CYAN
	intent_value.add_theme_color_override("font_color", RED if escalated else CYAN)

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


# An icon for the kind of move security has queued, with its number, so the
# threat reads at a glance before the line beside it is.
func _update_intent_icon() -> void:
	var intent: Dictionary = sentry.queued_intent()
	var kind: String = intent.get("kind", "")
	intent_icon.visible = INTENT_ICONS.has(kind)
	intent_value.visible = INTENT_ICONS.has(kind)
	if not INTENT_ICONS.has(kind):
		return
	intent_icon.texture = load(INTENT_ICONS[kind])
	var value_format: String = INTENT_VALUE_FORMATS[kind]
	intent_value.text = value_format % int(intent.get("value", 0)) if value_format != "" else ""
	intent_value.visible = not intent_value.text.is_empty()


# Wraps the intent line onto a second row once it is wider than
# INTENT_MAX_WIDTH, and lets it shrink back to one row when it is not.
func _fit_intent_label() -> void:
	intent_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	intent_label.custom_minimum_size.x = 0
	if intent_label.get_minimum_size().x > INTENT_MAX_WIDTH:
		intent_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		intent_label.custom_minimum_size.x = INTENT_MAX_WIDTH

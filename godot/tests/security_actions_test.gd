# Real scene/extension coverage for Trello #102: standard security does more
# than add noise. The Rust tests own the rules; this drives a real first-zone
# encounter against WARDEN-7 and checks what the player actually sees.
#
#   - E4a: Firewall Up is announced with the intent before the turn ends,
#     leaves a firewall of 8 standing once it runs, and that firewall is used
#     up by plain damage before any integrity is lost.
#
# Run: godot --headless --path godot -s res://tests/security_actions_test.gd
extends SceneTree

const FIREWALL_UP_BLOCK := 8
const STRIKE_DAMAGE := 6

var _flow: Node
var _combat: Node
var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, description: String) -> void:
	if condition:
		print("  ok    %s" % description)
	else:
		print("  FAIL  %s" % description)
		_failures.append(description)


func _screen() -> Node:
	var host := root.get_node_or_null("Main/ScreenHost")
	if host == null or host.get_child_count() == 0:
		return null
	return host.get_child(0)


func _finish() -> void:
	print("")
	if _failures.is_empty():
		print("security actions test: PASSED")
		quit(0)
	else:
		print("security actions test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _press_end_turn() -> void:
	_combat.get_node("VBoxContainer/Buttons/EndTurnButton").pressed.emit()


# Fixture only: reuse the real draw API rather than add a debug gameplay API.
func _draw_whole_deck() -> void:
	var draw: Node = _combat.draw_phase
	draw.hand_size = (draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count())
	_combat.selected_index = -1
	_combat._draw_hand()


func _play(card_name: String) -> bool:
	var index: int = _combat.draw_phase.hand_names().find(card_name)
	if index < 0:
		_check(false, "%s is in hand to play" % card_name)
		return false
	_combat.card_buttons[index].pressed.emit()
	_combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()
	return true


# Ends turns until WARDEN-7's next action is `action`, so a check can be made
# against the intent before the player commits. False if it never comes up.
func _end_turns_until_queued(action: String) -> bool:
	for turn in 6:
		if _combat.sentry.queued_action_name() == action:
			return true
		_press_end_turn()
	return _combat.sentry.queued_action_name() == action


# E4a scenario 1: Firewall Up is announced, then leaves 8 standing.
# E4a scenario 2: plain damage spends the firewall before integrity.
func _check_firewall_up() -> void:
	_check(_end_turns_until_queued("Firewall Up"), "WARDEN-7 queues Firewall Up")
	_check(
		_combat.intent_label.text
		== "WARDEN-7 will: Firewall Up (+1 noise, blocks %d damage)" % FIREWALL_UP_BLOCK,
		"the intent shows the firewall before the turn ends (got '%s')" % _combat.intent_label.text
	)
	_check(_combat.sentry.firewall() == 0, "no firewall is up before it runs")

	_press_end_turn()
	_check(
		_combat.sentry.firewall() == FIREWALL_UP_BLOCK,
		"Firewall Up leaves %d standing (got %d)" % [FIREWALL_UP_BLOCK, _combat.sentry.firewall()]
	)
	_check(
		_combat.health_label.text.contains("[Firewall %d]" % FIREWALL_UP_BLOCK),
		"the integrity line shows the firewall (got '%s')" % _combat.health_label.text
	)
	_check(
		_combat.status_label.text.contains("Firewall will block the next %d damage." % FIREWALL_UP_BLOCK),
		"the turn report says what the firewall will block (got '%s')" % _combat.status_label.text
	)

	var health_before: int = _combat.sentry.health()
	_draw_whole_deck()
	if not _play("Strike"):
		return
	_check(
		_combat.sentry.health() == health_before,
		"a Strike into the firewall loses no integrity"
	)
	_check(
		_combat.sentry.firewall() == FIREWALL_UP_BLOCK - STRIKE_DAMAGE,
		"the firewall wears down by the damage it absorbed"
	)
	_check(
		_combat.status_label.text.contains("Firewall blocked %d." % STRIKE_DAMAGE),
		"the play report says what the firewall blocked (got '%s')" % _combat.status_label.text
	)

	if not _play("Strike"):
		return
	var through := STRIKE_DAMAGE - (FIREWALL_UP_BLOCK - STRIKE_DAMAGE)
	_check(_combat.sentry.firewall() == 0, "the second Strike uses the firewall up")
	_check(
		_combat.sentry.health() == health_before - through,
		"only what the firewall could not absorb reaches integrity (%d)" % through
	)


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	_check(_flow.start_new_contract().get("ok", false), "New contract leaves the main menu")
	await process_frame
	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "a first-zone encounter can be entered")
	await process_frame
	await process_frame

	_combat = _screen()
	_check(
		_combat != null and _combat.name == "CombatScreen",
		"the combat screen is showing"
	)
	if _combat == null or _combat.name != "CombatScreen":
		_finish()
		return
	_check(
		_combat.sentry.construct_name() == "WARDEN-7",
		"the encounter is guarded by standard security (got %s)" % _combat.sentry.construct_name()
	)
	if _combat.sentry.construct_name() != "WARDEN-7":
		_finish()
		return

	_check_firewall_up()
	_finish()

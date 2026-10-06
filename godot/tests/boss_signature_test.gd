# Real scene/extension coverage for the deeper bosses on Trello #100 (E2b).
# The Rust tests own the rules; this walks a generated run to the zone-2 and
# zone-3 bosses and checks what the player actually sees: each boss is its own
# construct with more integrity than the last, BLACK MONOLITH's Firewall is
# announced, soaks up a plain Strike while Trojan goes straight through, and
# comes down when its next turn starts, and THE ARCHITECT's Grid Lockdown takes
# energy off the turn it opens. The zone-1 boss and its Purge are covered by
# boss_encounter_test.gd.
#
# Run: godot --headless --path godot -s res://tests/boss_signature_test.gd
extends SceneTree

const FIREWALL_BLOCK := 12
const STRIKE_DAMAGE := 6
const TROJAN_DAMAGE := 6

var _flow: Node
var _combat: Node
var _noise_meter: Node
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
		print("boss signature test: PASSED")
		quit(0)
	else:
		print("boss signature test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Walks the run into the boss closing `zone` (0-based), completing everything
# on the way without playing it: this test is about the bosses, not combat in
# general. Returns true once that boss's combat screen is up.
func _advance_to_boss(zone: int) -> bool:
	var bosses_passed := 0
	for step in 40:
		var options: Array = _flow.selectable_encounters()
		if options.is_empty():
			return false

		var chosen: Dictionary = options[0]
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "boss":
				chosen = candidate
				break

		var type := str(chosen.get("type", ""))
		var selected: Dictionary = _flow.select_encounter(chosen.id)
		if not selected.get("ok", false):
			push_error("could not enter %s: %s" % [chosen, selected])
			return false
		await process_frame
		await process_frame
		if type == "boss" and bosses_passed == zone:
			_combat = _screen()
			return _combat != null and _combat.has_node("Sentry")

		var completed: Dictionary = _flow.complete_active_encounter()
		if not completed.get("ok", false):
			push_error("could not complete %s: %s" % [chosen, completed])
			return false
		if type in ["combat", "elite", "boss"]:
			var skipped: Dictionary = _flow.finish_reward()
			if not skipped.get("ok", false):
				push_error("could not leave the card reward: %s" % skipped)
				return false
		if type == "boss":
			bosses_passed += 1
		await process_frame
		await process_frame
	return false


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
		return false
	_combat.card_buttons[index].pressed.emit()
	_combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()
	return true


# Ends turns until the boss's next action is `action`, so a check can be made
# against the intent before the player commits. False if it never comes up.
func _end_turns_until_queued(action: String) -> bool:
	for turn in 4:
		if _combat.sentry.queued_action_name() == action:
			return true
		_press_end_turn()
	return _combat.sentry.queued_action_name() == action


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	if _flow == null or _noise_meter == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	var zones: int = int(_flow.zone_progress().get("total_zones", 0))
	_check(zones == 3, "a generated contract has three zones (got %d)" % zones)
	if zones < 3:
		_finish()
		return

	# Fixture: Trojan is the penetrating card the Firewall cannot stop. It is
	# added up front, since a fight deals from the run deck it started with.
	_check(_flow.take_card_reward("trojan"), "the fixture deck includes Trojan")

	# --- zone 2: BLACK MONOLITH --------------------------------------------
	_check(await _advance_to_boss(1), "the run reaches the zone-2 boss")
	if _combat == null or not _combat.has_node("Sentry"):
		_finish()
		return

	_check(
		_combat.sentry.construct_name() == "BLACK MONOLITH",
		"the zone-2 boss is BLACK MONOLITH (got '%s')" % _combat.sentry.construct_name()
	)
	_check(
		_combat.sentry.max_health() == 100,
		"BLACK MONOLITH has 100 integrity (got %d)" % _combat.sentry.max_health()
	)

	_check(_end_turns_until_queued("Firewall"), "BLACK MONOLITH queues its Firewall")
	_check(
		_combat.intent_label.text.contains("Firewall")
		and _combat.intent_label.text.contains("blocks %d damage" % FIREWALL_BLOCK),
		"the Firewall is announced with what it does (got '%s')" % _combat.intent_label.text
	)
	_press_end_turn()
	_check(
		_combat.sentry.firewall() == FIREWALL_BLOCK,
		"the Firewall is up for the player's turn (got %d)" % _combat.sentry.firewall()
	)
	_check(
		_combat.status_label.text.contains(
			"Firewall will block the next %d damage" % FIREWALL_BLOCK
		),
		"and the screen says so (got '%s')" % _combat.status_label.text
	)
	_check(
		_combat.health_label.text.contains("Firewall %d" % FIREWALL_BLOCK),
		"the health line shows the Firewall (got '%s')" % _combat.health_label.text
	)

	_draw_whole_deck()
	var health_before: int = _combat.sentry.health()
	_check(_play("Strike"), "Strike is in the full-deck hand")
	_check(
		_combat.sentry.health() == health_before,
		"the Firewall soaks up a Strike before integrity (%d -> %d)"
		% [health_before, _combat.sentry.health()]
	)
	_check(
		_combat.status_label.text.contains("Firewall blocked %d" % STRIKE_DAMAGE),
		"and the card says what was blocked (got '%s')" % _combat.status_label.text
	)
	_check(
		_combat.sentry.firewall() == FIREWALL_BLOCK - STRIKE_DAMAGE,
		"the Firewall wears down by what it blocked (got %d)" % _combat.sentry.firewall()
	)

	_check(_play("Trojan"), "Trojan is in the full-deck hand")
	_check(
		_combat.sentry.health() == health_before - TROJAN_DAMAGE,
		"Trojan's penetrating damage goes straight through (%d -> %d)"
		% [health_before, _combat.sentry.health()]
	)
	_check(
		_combat.sentry.firewall() == FIREWALL_BLOCK - STRIKE_DAMAGE,
		"and leaves the Firewall where it was"
	)

	_press_end_turn()
	_check(_combat.sentry.firewall() == 0, "the Firewall comes down when the boss's turn starts")
	_check(
		not _combat.health_label.text.contains("Firewall"),
		"and the health line stops showing it (got '%s')" % _combat.health_label.text
	)

	# --- zone 3: THE ARCHITECT ---------------------------------------------
	var left: Dictionary = _flow.complete_active_encounter()
	_check(left.get("ok", false), "the zone-2 boss can be completed")
	_flow.finish_reward()
	await process_frame
	await process_frame

	_check(await _advance_to_boss(0), "the run reaches the zone-3 boss")
	if _combat == null or not _combat.has_node("Sentry"):
		_finish()
		return

	_check(
		_combat.sentry.construct_name() == "THE ARCHITECT",
		"the zone-3 boss is THE ARCHITECT (got '%s')" % _combat.sentry.construct_name()
	)
	_check(
		_combat.sentry.max_health() == 120,
		"THE ARCHITECT has 120 integrity (got %d)" % _combat.sentry.max_health()
	)

	var max_energy: int = _combat.draw_phase.max_energy
	_check(_end_turns_until_queued("Grid Lockdown"), "THE ARCHITECT queues its Grid Lockdown")
	_check(
		_combat.intent_label.text.contains("-3 energy"),
		"the lockdown is announced before it lands (got '%s')" % _combat.intent_label.text
	)
	_press_end_turn()
	var drained: int = mini(3, max_energy)
	_check(
		_combat.draw_phase.energy == max_energy - drained,
		(
			"the lockdown leaves the new turn short of energy (%d of %d)"
			% [_combat.draw_phase.energy, max_energy]
		)
	)
	_check(
		_combat.status_label.text.contains("Lockdown cost you %d energy" % drained),
		"and says why (got '%s')" % _combat.status_label.text
	)
	var costs: PackedInt32Array = _combat.draw_phase.hand_costs()
	for index in costs.size():
		_check(
			_combat.card_buttons[index].disabled == (costs[index] > max_energy - drained),
			"lockdown updates card %d's affordability" % index
		)
	_press_end_turn()
	_check(
		_combat.draw_phase.energy == max_energy,
		"lockdown lasts one turn, not the rest of the encounter"
	)

	_finish()

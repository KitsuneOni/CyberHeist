# Real scene/extension coverage for safehouse nodes (Trello #112). The Rust
# tests own the rules; this enters a safehouse from the map the way a player
# would and checks what only a running game can show.
#
#   Scenario: Lie low
#     Given I enter a safehouse with noise at 60
#     When I choose Lie Low
#     Then my noise drops by 30 to 30
#     And the node is completed
#
#   Scenario: Upgrade instead
#     Given I enter a safehouse
#     When I choose Upgrade and pick a card
#     Then that card is upgraded for the rest of the run
#     And I cannot also Lie Low at this safehouse
#
# Also checks the safehouse has its own marker and key entry on the map.
#
# Run: godot --headless --path godot -s res://tests/safehouse_test.gd
extends SceneTree

const NOISE_ON_ENTRY := 60
const LIE_LOW_REDUCTION := 30

# Generated contracts do not guarantee a safehouse, so the test rolls new runs
# until a first-zone route offers one. About one node in ten is a safehouse,
# so this many runs without one would mean they had stopped being generated.
const MAX_RUNS := 80

var _flow: Node
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
		print("safehouse test: PASSED")
		quit(0)
	else:
		print("safehouse test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _settle() -> void:
	await process_frame
	await process_frame


func _set_noise(level: int) -> void:
	_noise_meter.add_noise(level - _noise_meter.noise)


# From the computer desktop of a new run, accepts the offer the picker shows,
# then walks the first zone until the map offers a safehouse and clicks it.
# Encounters on the way are completed without being played. Returns the
# safehouse's node id, or -1 if this run's first zone had none.
func _walk_to_first_zone_safehouse() -> int:
	var desktop := _screen()
	if desktop != null and desktop.name == "ComputerDesktop":
		desktop.contracts_app.pressed.emit()
		await _settle()
	var hub := _screen()
	if hub == null or hub.name != "ContractHub":
		return -1
	if hub.is_choosing_contract():
		hub.offer_picker.accept_button.pressed.emit()
		await _settle()
		if hub.is_choosing_contract():
			return -1

	for step in 10:
		hub = _screen()
		if hub == null or hub.name != "ContractHub":
			return -1

		var options: Array = _flow.selectable_encounters()
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "safehouse":
				var safehouse_id := int(candidate.id)
				_set_noise(NOISE_ON_ENTRY)
				hub.map_graph.node_selected.emit(safehouse_id)
				await _settle()
				return safehouse_id

		# Bosses close the zone, so a safehouse past one would not be zone 1.
		var chosen: Dictionary = {}
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) != "boss":
				chosen = candidate
				break
		if chosen.is_empty():
			return -1

		if not _flow.select_encounter(chosen.id).get("ok", false):
			return -1
		if not _flow.complete_active_encounter().get("ok", false):
			return -1
		if _screen() != null and _screen().name == "CardReward":
			_flow.finish_reward()
		await _settle()
	return -1


# Rolls runs until one's first zone offers a safehouse, and enters it.
func _enter_a_safehouse() -> Node:
	for attempt in MAX_RUNS:
		if attempt > 0 or _screen().name != "ComputerDesktop":
			_flow.start_new_run()
			await _settle()
		if await _walk_to_first_zone_safehouse() >= 0:
			return _screen()
	return null


func _node_status(node_id: int) -> String:
	for entry: Dictionary in _flow.map_progress():
		if int(entry.get("id", -1)) == node_id:
			return str(entry.get("status", ""))
	return ""


func _deck_count(card_id: String) -> int:
	for entry: Dictionary in _flow.run_deck_listing():
		if str(entry.get("id", "")) == card_id:
			return int(entry.get("count", 0))
	return 0


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
	await _settle()

	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null:
		_finish()
		return
	menu.get_node("%NewContractButton").pressed.emit()
	await _settle()

	# --- the map marks safehouses and explains them in the key ---------------
	var key_types: Array = []
	for entry: Dictionary in _flow.map_key().get("encounter_types", []):
		key_types.append(str(entry.get("type", "")))
	_check(key_types.has("safehouse"), "the map key explains the safehouse marker")

	# --- Scenario: Lie low ----------------------------------------------------
	var safehouse := await _enter_a_safehouse()
	_check(
		safehouse != null and safehouse.name == "SafehouseScreen",
		"a first-zone safehouse opens the safehouse screen within %d runs" % MAX_RUNS
	)
	if safehouse == null or safehouse.name != "SafehouseScreen":
		_finish()
		return
	var safehouse_id := int(_flow.run_snapshot().get("active_encounter", {}).get("id", -1))
	_check(_noise_meter.noise == NOISE_ON_ENTRY, "noise is %d on entering" % NOISE_ON_ENTRY)
	_check(not safehouse.lie_low_button.disabled, "Lie Low is offered")
	_check(
		safehouse.lie_low_label.text.contains("%d → %d" % [NOISE_ON_ENTRY, NOISE_ON_ENTRY - LIE_LOW_REDUCTION]),
		"Lie Low previews the noise it leaves (%s)" % safehouse.lie_low_label.text
	)

	safehouse.lie_low_button.pressed.emit()
	await _settle()
	_check(
		_noise_meter.noise == NOISE_ON_ENTRY - LIE_LOW_REDUCTION,
		"Lie Low drops noise by %d to %d (got %d)" % [LIE_LOW_REDUCTION, NOISE_ON_ENTRY - LIE_LOW_REDUCTION, _noise_meter.noise]
	)
	_check(safehouse.lie_low_button.disabled, "Lie Low cannot be chosen twice")
	var any_upgrade_enabled := false
	for button: Button in safehouse._upgrade_buttons:
		any_upgrade_enabled = any_upgrade_enabled or not button.disabled
	_check(not any_upgrade_enabled, "after lying low no card can also be upgraded")
	var forced: Dictionary = safehouse.safehouse_node.lie_low()
	_check(
		not forced.get("ok", true) and _noise_meter.noise == NOISE_ON_ENTRY - LIE_LOW_REDUCTION,
		"a forced second Lie Low is refused and changes nothing"
	)

	safehouse.leave_button.pressed.emit()
	await _settle()
	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "Move on returns to the map")
	_check(_node_status(safehouse_id) == "completed", "the safehouse node is completed")

	# --- Scenario: Upgrade instead ---------------------------------------------
	safehouse = await _enter_a_safehouse()
	_check(
		safehouse != null and safehouse.name == "SafehouseScreen",
		"a second run reaches a safehouse"
	)
	if safehouse == null or safehouse.name != "SafehouseScreen":
		_finish()
		return

	var cards: Array = safehouse.safehouse_node.upgradable_cards()
	_check(not cards.is_empty(), "the starter deck has a card the safehouse can upgrade")
	if cards.is_empty():
		_finish()
		return
	var card_id := str(cards[0].get("id", ""))
	var copies_before := _deck_count(card_id)
	var deck_size_before := int(_flow.run_deck_size())

	safehouse._upgrade_buttons[0].pressed.emit()
	await _settle()
	# An upgraded card's id is its own with a + on the end (card_data.rs).
	var upgraded_id := card_id + "+"
	_check(_deck_count(card_id) == copies_before - 1, "one copy of %s is upgraded" % card_id)
	_check(_deck_count(upgraded_id) >= 1, "%s is in the run's deck" % upgraded_id)
	_check(int(_flow.run_deck_size()) == deck_size_before, "the deck keeps its size")
	_check(safehouse.lie_low_button.disabled, "after upgrading, Lie Low is not offered")
	forced = safehouse.safehouse_node.lie_low()
	_check(
		not forced.get("ok", true) and str(forced.get("error", "")) == "already_used",
		"a forced Lie Low after upgrading is refused"
	)
	_check(_noise_meter.noise == NOISE_ON_ENTRY, "noise is untouched by the upgrade")

	safehouse.leave_button.pressed.emit()
	await _settle()
	_check(_screen() != null and _screen().name == "ContractHub", "Move on returns to the map")
	_check(_deck_count(upgraded_id) >= 1, "the upgrade lasts after leaving the safehouse")

	_finish()

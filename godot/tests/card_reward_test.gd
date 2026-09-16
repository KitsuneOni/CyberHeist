# Real scene/extension coverage for story #53: card rewards after an encounter,
# and the run deck those rewards go into.
#
# The second acceptance test ("available to draw in subsequent encounters") is
# the whole reason this needs a headless run rather than a Rust unit test: it is
# a property of state surviving a scene change, which unit tests cannot observe.
#
# Run: godot --headless --path godot -s res://tests/card_reward_test.gd
extends SceneTree

var _flow: Node
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
	if _failures.is_empty():
		print("card reward test: PASSED")
		quit(0)
	else:
		print("card reward test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# Every card the encounter deck holds, wherever it currently sits.
func _encounter_deck_size(combat: Node) -> int:
	var draw: Node = combat.draw_phase
	return draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()


func _run() -> void:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	# --- the run owns a deck before any encounter ---
	var starting_size: int = _flow.run_deck_size()
	_check(starting_size > 0, "the run starts owning a deck of cards")

	# --- acceptance test 1: an offer of cards, each showing what it does ---
	var offer: Array = _flow.offer_card_reward(3)
	_check(offer.size() == 3, "clearing an encounter offers a selection of cards")

	var offered_ids := {}
	var described := true
	for card: Dictionary in offer:
		offered_ids[card.get("id", "")] = true
		if (
			String(card.get("name", "")).is_empty()
			or String(card.get("type", "")).is_empty()
			or String(card.get("description", "")).is_empty()
			or String(card.get("noise_text", "")).is_empty()
			or String(card.get("cost_text", "")).is_empty()
		):
			described = false
	_check(described, "each offered card shows its name, type, effect and noise value")
	_check(offered_ids.size() == offer.size(), "an offer never repeats the same card")

	# --- acceptance test 2, part one: taking a card grows the run's deck ---
	var chosen: Dictionary = offer[0]
	var chosen_id: String = chosen.get("id", "")
	var owned_before: int = _flow.run_deck_size()
	_check(_flow.take_card_reward(chosen_id), "the chosen card is accepted into the deck")
	_check(
		_flow.run_deck_size() == owned_before + 1,
		"the run's deck grows by exactly the card that was taken"
	)

	# An id the database does not know is refused rather than stored.
	_check(
		not _flow.take_card_reward("not_a_real_card"),
		"an unknown card is refused instead of being added"
	)
	_check(
		_flow.run_deck_size() == owned_before + 1,
		"a refused card leaves the deck untouched"
	)

	# --- acceptance test 2, part two: it is there in the next encounter ---
	var expected_type: String = ""
	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "the next encounter loads through the real flow")
	if not selected.get("ok", false):
		_finish()
		return
	var active = _flow.run_snapshot().get("active_encounter")
	if typeof(active) == TYPE_DICTIONARY:
		expected_type = active.get("type", "")
	await process_frame
	await process_frame

	var combat := _screen()
	_check(combat != null and combat.name == "CombatScreen", "the combat screen is showing")
	if combat == null:
		_finish()
		return

	_check(
		_encounter_deck_size(combat) == _flow.run_deck_size(),
		"the encounter deck is built from exactly what the run owns, not the starter list"
	)

	# Draw the entire deck so the taken card must appear if it is really there.
	var draw: Node = combat.draw_phase
	draw.hand_size = _encounter_deck_size(combat)
	combat._draw_hand()
	var chosen_name: String = chosen.get("name", "")
	_check(
		draw.hand_names().has(chosen_name),
		"the card taken as a reward can be drawn in a later encounter ('%s')" % chosen_name
	)

	# --- clearing a combat encounter routes to the reward screen ---
	var completed: Dictionary = _flow.complete_active_encounter()
	_check(completed.get("ok", false), "the encounter completes")
	await process_frame
	await process_frame

	var after := _screen()
	if expected_type == "combat" or expected_type == "elite":
		_check(
			after != null and after.name == "CardReward",
			"clearing a %s encounter offers a card reward" % expected_type
		)
		if after != null and after.name == "CardReward":
			_check(
				after.offered.size() > 0,
				"the reward screen presents cards to choose from"
			)
			# Skipping is allowed and must still return to the hub.
			after._on_skip_pressed()
			await process_frame
			await process_frame
			var hub := _screen()
			_check(
				hub != null and hub.name == "ContractHub",
				"skipping the reward returns to the contract hub"
			)
			_check(
				_flow.run_deck_size() == owned_before + 1,
				"skipping a reward adds nothing to the deck"
			)
	else:
		_check(
			after != null and after.name == "ContractHub",
			"clearing a %s encounter goes straight back to the hub" % expected_type
		)

	_finish()

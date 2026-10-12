# Real scene/extension coverage for saving the game between sessions (Trello
# #10). The Rust tests own the save format: the round trip, the version check
# and refusing a corrupt or impossible save. This checks the game around it:
#
#   Scenario: First launch with no saved game
#     Given no save file exists
#     When the game opens
#     Then Continue run is unavailable, no error is shown and New contract
#     starts from the beginning
#
#   Scenario: A corrupt or incompatible save starts safely
#     Given the save file is corrupt, or from another save version
#     When the game opens
#     Then nothing from it is loaded, Continue run is unavailable and the
#     menu says the save could not be loaded
#     And New contract replaces it with a good save
#
#   Scenario: The game saves without the player asking
#     Given the player finishes an encounter
#     Then the run is saved with that encounter completed, without the
#     player choosing to save
#     And entering a fight is not a safe point, so it is not saved
#
#   Scenario: Progress is kept after closing the game
#     Given the player has completed encounters on an accepted contract
#     When the game is closed and opened again and Continue run is pressed
#     Then the contract, map, completed encounters, zone, credits, deck,
#     upgrades, noise, knowledge, stats and phase are exactly as they were
#     And the same holds on the caught screen, where the fine is not charged
#     a second time, in a shop after buying a card, and on a fight's card
#     reward, which offers the same cards rather than rerolling them
#
#   Scenario: Starting a new contract replaces the saved run
#
# Closing and reopening is real: this process plays and saves, writes down
# what the run looked like at its last safe point, then starts a second Godot
# process on the same project and save file. That process opens the main
# menu, presses Continue and compares what it finds (see _resume_and_compare).
#
# Run: godot --headless --path godot -s res://tests/save_continue_test.gd
extends SceneTree

# Kept apart from the player's own save. Tests have saving switched off
# unless they choose a file (see FlowCoordinator._ready).
const SAVE_PATH := "user://save_continue_test.json"
const EXPECTED_PATH := "user://save_continue_test_expected.json"
# Passed to the second process, after "--", to make it resume and compare.
const RESUME_ARGUMENT := "--resume-and-compare"
const RESUME_PASSED_MARKER := "save continue resume: PASSED"
# A resumed process that hangs fails rather than holding up the whole job.
const RESUME_TIMEOUT_SECONDS := 60.0
# Fresh runs to roll looking for a shop on the first step. Each roll offers
# one about a quarter of the time, so this only bounds a very bad streak.
const MAX_SHOP_ROLLS := 60
# Fresh runs to roll looking for a fight on the first step, which most offer.
const MAX_FIGHT_ROLLS := 60
# Credits to walk into the shop with, enough for any card on sale.
const SHOP_CREDITS := 200

var _flow: Node
var _player: Node
var _meter: Node
var _knowledge: Node
var _run_deck: Node
var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, description: String) -> void:
	if condition:
		print("  ok    %s" % description)
	else:
		print("  FAIL  %s" % description)
		_failures.append(description)


func _finish(passed_marker: String) -> void:
	print("")
	if _failures.is_empty():
		print(passed_marker)
		quit(0)
	else:
		print("save continue test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _screen() -> Node:
	var host := root.get_node_or_null("Main/ScreenHost")
	if host == null or host.get_child_count() == 0:
		return null
	return host.get_child(0)


func _screen_name() -> String:
	var screen := _screen()
	return str(screen.name) if screen != null else ""


func _settle() -> void:
	# Screen replacement frees the old screen at the end of the frame.
	await process_frame
	await process_frame


func _find_autoloads() -> bool:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_player = root.get_node_or_null("/root/PlayerStateGlobal")
	_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_knowledge = root.get_node_or_null("/root/KnowledgeMeterGlobal")
	_run_deck = root.get_node_or_null("/root/FlowCoordinator/RunDeck")
	var found := (
		_flow != null and _player != null and _meter != null
		and _knowledge != null and _run_deck != null
	)
	_check(found, "the game's autoloads are present")
	return found


func _open_game() -> void:
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()


# Shows the main menu again, as a fresh launch would after reading the save.
func _reopen_menu() -> Node:
	_flow.bind_screen_host(root.get_node("Main/ScreenHost"))
	await _settle()
	return _screen()


func _write_file(path: String, text: String) -> void:
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(text)
	file.close()


func _saved() -> Dictionary:
	var parsed = JSON.parse_string(FileAccess.get_file_as_string(SAVE_PATH))
	return parsed if typeof(parsed) == TYPE_DICTIONARY else {}


func _saved_phase() -> String:
	return str(_saved().get("run", {}).get("run_state", {}).get("phase", ""))


# Everything Continue has to bring back, read the way the screens read it.
func _capture_state() -> Dictionary:
	var state := {
		"screen": _screen_name(),
		"snapshot": _flow.run_snapshot(),
		"accepted_offer": _flow.accepted_contract_offer(),
		"offers": _flow.contract_offers(),
		"map": _flow.map_progress(),
		"zone": _flow.zone_progress(),
		"progress": _flow.contract_progress(),
		"selectable": _flow.selectable_encounters(),
		"summary": _flow.run_summary(),
		"deck": Array(_run_deck.card_ids()),
		"credits": _player.money(),
		"upgrades": Array(_player.upgrades()),
		"noise": _meter.noise,
		"knowledge": _knowledge.knowledge,
	}

	var screen := _screen()
	if state.screen == "ShopScreen":
		state["shop_stock"] = screen.shop_node.stock()
	elif state.screen == "CaughtScreen":
		state["caught_fine"] = screen.get_node("VBoxContainer/FineLabel").text
		state["caught_upgrades"] = screen.get_node("VBoxContainer/UpgradesLabel").text
	elif state.screen == "CardReward":
		var offered_ids := []
		for card: Dictionary in screen.offered:
			offered_ids.append(card.get("id", ""))
		state["reward_offer"] = offered_ids
		state["reward_status"] = screen.status_label.text
	return state


# JSON keeps every number as a float, so both sides of a comparison go
# through it to be compared like for like.
func _through_json(value: Variant) -> Variant:
	return JSON.parse_string(JSON.stringify(value))


func _completed_encounters(map: Array) -> int:
	var completed := 0
	for entry: Dictionary in map:
		if entry.get("status", "") == "completed" and not entry.get("is_entry", false):
			completed += 1
	return completed


# --- the first process: play, save and hand over ---------------------------


func _run() -> void:
	await process_frame
	if not _find_autoloads():
		_finish("")
		return

	if OS.get_cmdline_user_args().has(RESUME_ARGUMENT):
		await _resume_and_compare()
		return

	_remove_test_files()
	await _check_first_launch()
	await _check_unreadable_saves()
	if not await _start_and_accept_a_contract():
		_finish("")
		return
	if not await _play_two_encounters():
		_finish("")
		return

	# Written down at the hub, then the next encounter is entered and the
	# game "closed" partway through it: Continue resumes at the hub.
	print("-- resuming at the hub after two encounters and quitting mid-encounter")
	_write_expected_state("hub")
	await _check_a_fight_is_not_a_safe_point()
	_resume_in_a_new_process("hub")

	print("-- resuming on the caught screen")
	await _check_caught_resume()

	print("-- resuming in a shop after a purchase")
	await _check_shop_resume()

	print("-- resuming on a fight's card reward")
	await _check_card_reward_resume()

	_remove_test_files()
	_finish("save continue test: PASSED")


func _remove_test_files() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(EXPECTED_PATH))


func _check_first_launch() -> void:
	var loaded: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(
		loaded.get("ok", false) and not loaded.get("has_run", true),
		"with no save file, loading succeeds and finds no run"
	)
	_check(_flow.save_load_error().is_empty(), "a missing save is not reported as an error")

	await _open_game()
	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null:
		return
	_check(menu.continue_button.disabled, "Continue run is unavailable on a first launch")
	_check(menu.status_label.text.is_empty(), "no error is shown on a first launch")
	_check(not FileAccess.file_exists(SAVE_PATH), "nothing is saved just by opening the game")


func _check_unreadable_saves() -> void:
	var credits_before: int = _player.money()

	_write_file(SAVE_PATH, "{\"version\": 1, \"meta\": {\"credits\": 999")
	var corrupt: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(not corrupt.get("ok", true), "a corrupt save is refused")
	_check(not _flow.has_saved_run(), "a corrupt save leaves no run to continue")
	var menu := await _reopen_menu()
	_check(
		menu != null and menu.continue_button.disabled,
		"Continue run is unavailable when the save is corrupt"
	)
	_check(
		menu != null and menu.status_label.text.contains("could not be loaded"),
		"the menu says the saved run could not be loaded"
	)

	_write_file(
		SAVE_PATH,
		JSON.stringify({"version": 999, "meta": {"credits": 999}, "run": {}})
	)
	var incompatible: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(
		not incompatible.get("ok", true) and str(incompatible.get("error", "")).contains("version"),
		"a save from another version is refused as incompatible"
	)
	_check(not _flow.has_saved_run(), "an incompatible save leaves no run to continue")
	_check(_player.money() == credits_before, "nothing from a refused save is applied")
	await _reopen_menu()


# New contract from the menu, then the offer shown first, accepted on the
# hub's picker as a player would.
func _start_and_accept_a_contract() -> bool:
	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the main menu is showing before New contract")
		return false
	menu.new_contract_button.pressed.emit()
	await _settle()

	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "New contract opens the contract hub")
	var fresh := _saved()
	_check(
		int(fresh.get("version", -1)) == 1 and _saved_phase() == "hub",
		"New contract replaces the unreadable save with a good one at the current version"
	)
	_check(
		fresh.get("run", {}).get("run_state", {}).get("accepted_offer_index") == null,
		"the new contract's save starts before any offer is accepted"
	)
	if hub == null or hub.name != "ContractHub" or not hub.is_choosing_contract():
		_check(false, "the hub asks for a contract")
		return false

	hub.offer_picker.accept_button.pressed.emit()
	await _settle()
	var accepted: Dictionary = _flow.accepted_contract_offer()
	_check(accepted.get("ok", false), "an offer is accepted on the picker")
	_check(
		_saved().get("run", {}).get("run_state", {}).get("accepted_offer_index")
		== float(accepted.get("index", -1)),
		"accepting the offer is saved straight away"
	)
	return accepted.get("ok", false)


# Plays two encounters on the map, away from the zone's boss. A fight picks
# up some noise and knowledge on the way, the way playing cards would, and
# its card reward is taken. Each completion should be saved unprompted.
func _play_two_encounters() -> bool:
	_player.add_upgrade("Lucky Charm")
	for step in 2:
		var hub := _screen()
		var chosen := _first_selectable(["combat", "elite", "event", "shop"])
		if hub == null or hub.name != "ContractHub" or chosen.is_empty():
			_check(false, "encounter %d can be chosen on the map" % (step + 1))
			return false

		hub.map_graph.node_selected.emit(int(chosen.id))
		await _settle()
		if _screen_name() == "CombatScreen":
			_meter.add_noise(17)
			_knowledge.add_knowledge(1)

		var completed: Dictionary = _flow.complete_active_encounter()
		_check(completed.get("ok", false), "encounter %d (%s) completes" % [step + 1, chosen.type])
		await _settle()
		if _screen_name() == "CardReward":
			_check(_saved_phase() == "hub", "a fight's completion is saved before its card is chosen")
			_screen()._on_card_chosen(0)
			await _settle()

		var saved_run: Dictionary = _saved().get("run", {})
		var completed_nodes: Array = saved_run.get("run_state", {}).get("contract_map", {}).get(
			"completed_nodes", []
		)
		_check(
			_saved_phase() == "hub" and completed_nodes.has(float(chosen.id)),
			"encounter %d is saved as completed without the player saving" % (step + 1)
		)
		_check(
			int(saved_run.get("noise", -1)) == _meter.noise,
			"the save after encounter %d holds the current noise" % (step + 1)
		)
	return _screen_name() == "ContractHub"


# The first selectable encounter of one of `types`, in the order given.
func _first_selectable(types: Array) -> Dictionary:
	var options: Array = _flow.selectable_encounters()
	for wanted in types:
		for option: Dictionary in options:
			if option.get("type", "") == wanted:
				return option
	return {}


# Entering a fight or an event is not a safe point. Noise picked up in one
# stays out of the save until it is over, so quitting there resumes at the hub
# before it.
func _check_a_fight_is_not_a_safe_point() -> void:
	var fight := _first_selectable(["combat", "elite", "boss", "event"])
	if fight.is_empty():
		_check(false, "a fight or an event is on offer after two encounters")
		return
	var noise_at_hub: int = _meter.noise
	var entered: Dictionary = _flow.select_encounter(int(fight.id))
	await _settle()
	_meter.add_noise(30)
	_check(entered.get("ok", false), "the next %s can be entered" % fight.type)
	_check(
		_saved_phase() == "hub" and int(_saved().get("run", {}).get("noise", -1)) == noise_at_hub,
		"nothing from inside the %s is saved" % fight.type
	)


# Caught in the fight: the caught screen takes the fine and saves. Resuming
# it must show the same penalty without fining the player again.
func _check_caught_resume() -> void:
	var caught: Dictionary = _flow.report_caught()
	await _settle()
	_check(caught.get("ok", false), "the player is caught in the fight")
	_check(_saved_phase() == "caught", "being caught is saved")
	_write_expected_state("caught")
	_resume_in_a_new_process("caught")


# Rolls new runs until the first step offers a shop, walks in with credits,
# buys the cheapest card and resumes from there.
func _check_shop_resume() -> void:
	var left: Dictionary = _flow.finish_caught()
	await _settle()
	_check(left.get("ok", false), "the caught screen leaves for the hub")

	var shop := {}
	for _roll in MAX_SHOP_ROLLS:
		_flow.start_new_run()
		await _settle()
		shop = _first_selectable(["shop"])
		if not shop.is_empty():
			break
	if shop.is_empty():
		_check(false, "a shop is offered on the first step within %d runs" % MAX_SHOP_ROLLS)
		return

	_flow.change_credits(SHOP_CREDITS)
	_flow.select_encounter(int(shop.id))
	await _settle()
	var screen := _screen()
	_check(screen != null and screen.name == "ShopScreen", "the shop opens")
	if screen == null or screen.name != "ShopScreen":
		return
	_check(_saved_phase() == "encounter_active", "entering a shop is saved")

	var slot := _cheapest_slot(screen.shop_node.stock())
	if slot < 0:
		_check(false, "the shop has a card to buy")
		return
	screen._buy_buttons[slot].pressed.emit()
	await _settle()
	var saved_stock: Array = _saved().get("run", {}).get("shop_visit", {}).get("slots", [])
	_check(
		slot < saved_stock.size() and saved_stock[slot].get("sold", false),
		"buying a card is saved with its slot sold"
	)
	_write_expected_state("shop")
	_resume_in_a_new_process("shop")


# Leaves the shop, rolls new runs until the first step offers a fight, wins
# it and resumes on its card reward before a card is chosen. The resumed
# screen must offer the cards this one did.
func _check_card_reward_resume() -> void:
	var left: Dictionary = _flow.complete_active_encounter()
	await _settle()
	_check(left.get("ok", false), "the shop is left for the hub")

	var fight := {}
	for _roll in MAX_FIGHT_ROLLS:
		_flow.start_new_run()
		await _settle()
		fight = _first_selectable(["combat", "elite"])
		if not fight.is_empty():
			break
	if fight.is_empty():
		_check(false, "a fight is offered on the first step within %d runs" % MAX_FIGHT_ROLLS)
		return

	_flow.select_encounter(int(fight.id))
	await _settle()
	var completed: Dictionary = _flow.complete_active_encounter()
	await _settle()
	var screen := _screen()
	_check(
		completed.get("ok", false) and screen != null and screen.name == "CardReward",
		"winning the %s opens its card reward" % fight.type
	)
	if screen == null or screen.name != "CardReward":
		return

	var offered_ids := []
	for card: Dictionary in screen.offered:
		offered_ids.append(card.get("id", ""))
	var saved_reward: Dictionary = _saved().get("run", {}).get("screen", {}).get(
		"pending_card_reward", {}
	)
	_check(
		not offered_ids.is_empty() and saved_reward.get("offered_card_ids", []) == offered_ids,
		"the card reward is saved with the cards it offers"
	)
	_write_expected_state("card_reward")
	_resume_in_a_new_process("card_reward")


func _cheapest_slot(stock: Array) -> int:
	var cheapest := -1
	for card: Dictionary in stock:
		if card.get("sold", false):
			continue
		if cheapest < 0 or int(card.price) < int(stock[cheapest].price):
			cheapest = int(card.slot)
	return cheapest


# Writes down the run as it stands, for the second process to compare with.
# It must be a safe point, saved as it stands.
func _write_expected_state(scenario: String) -> void:
	_check(_flow.save_progress().get("ok", false), "the %s state is at a safe point" % scenario)
	var state := _capture_state()
	state["scenario"] = scenario
	_write_file(EXPECTED_PATH, JSON.stringify(state))


# Starts a second Godot process on the same project, which reads the save,
# presses Continue and compares against the state written down for it.
func _resume_in_a_new_process(scenario: String) -> void:
	var arguments := PackedStringArray([
		"--headless",
		"--path",
		ProjectSettings.globalize_path("res://"),
		"-s",
		"res://tests/save_continue_test.gd",
		"--",
		RESUME_ARGUMENT,
	])
	var output := []
	var exit_code := OS.execute(OS.get_executable_path(), arguments, output, true)
	for chunk in output:
		for line in str(chunk).split("\n"):
			if not line.strip_edges().is_empty():
				print("    | %s" % line)

	var passed := exit_code == 0 and "\n".join(output).contains(RESUME_PASSED_MARKER)
	_check(passed, "a new game process continues the %s run exactly" % scenario)


# --- the second process: reopen, continue and compare ----------------------


func _resume_and_compare() -> void:
	create_timer(RESUME_TIMEOUT_SECONDS).timeout.connect(_on_resume_timed_out)

	var expected = JSON.parse_string(FileAccess.get_file_as_string(EXPECTED_PATH))
	if typeof(expected) != TYPE_DICTIONARY:
		_check(false, "the first process wrote down what to expect")
		_finish(RESUME_PASSED_MARKER)
		return
	var scenario := str(expected.get("scenario", ""))
	print("resuming the %s run" % scenario)

	var loaded: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(loaded.get("ok", false) and loaded.get("has_run", false), "the save has a run to continue")
	_check(
		_player.money() == int(expected.get("credits", -1)),
		"the credit balance is back as soon as the game opens"
	)

	await _open_game()
	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the game opens on the main menu")
		_finish(RESUME_PASSED_MARKER)
		return
	_check(not menu.continue_button.disabled, "Continue run is available")
	menu.continue_button.pressed.emit()
	await _settle()

	var actual: Dictionary = _through_json(_capture_state())
	for key in expected:
		if key == "scenario":
			continue
		_check(
			JSON.stringify(actual.get(key)) == JSON.stringify(expected[key]),
			"%s is as it was when the game closed" % key
		)
	if scenario == "hub":
		_check(
			_completed_encounters(actual.get("map", [])) == 2,
			"both completed encounters are still marked completed"
		)
		await _check_new_run_replaces_the_save()

	_finish(RESUME_PASSED_MARKER)


func _check_new_run_replaces_the_save() -> void:
	var started: Dictionary = _flow.start_new_run()
	await _settle()
	var run_state: Dictionary = _saved().get("run", {}).get("run_state", {})
	_check(
		started.get("ok", false)
		and run_state.get("accepted_offer_index") == null
		and run_state.get("contract_map", {}).get("completed_nodes", [0]).is_empty(),
		"starting a new contract replaces the saved run with the new one"
	)


func _on_resume_timed_out() -> void:
	print("save continue resume: timed out after %d seconds" % RESUME_TIMEOUT_SECONDS)
	quit(1)

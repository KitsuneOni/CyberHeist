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
#     Given the save file is corrupt, or from another save version, including
#     version 1, which kept a claimed flag and a separate contract payout
#     When the game opens
#     Then its run is not loaded, Continue run is unavailable and the menu
#     says the save could not be loaded
#     And the file is kept as a .unreadable copy, and the credit balance it
#     names is kept when it still names one, since that carries between runs
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
#   Scenario: A contract can still be changed after continuing
#     Given the player accepted a contract, changed it, and entered nothing
#     When the game is closed and Continue run is pressed
#     Then the map shows the changed contract with Change contract offered,
#     and changing again works and is saved
#     And once an encounter has been played, Continue no longer offers it
#
#   Scenario: Continuing on the victory screen pays nothing twice
#     Given the player beat the final boss and was paid the contract reward
#     When the game is closed and Continue run is pressed
#     Then the victory screen shows the same contract reward and boss payout
#     and counts up to the same balance, the balance does not grow, and
#     Take another contract starts a new run at the offer picker
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
# Shops to pass through when nothing else is on offer after two encounters.
const MAX_SHOPS_IN_A_ROW := 4
# Credits to walk into the shop with, enough for any card on sale.
const SHOP_CREDITS := 200
# The save layout this build writes (SAVE_VERSION in save_game.rs).
const SAVE_VERSION := 3
# Where a save that will not load is copied before it can be replaced.
const UNREADABLE_COPY_PATH := SAVE_PATH + ".unreadable"
# How long the resumed victory screen may take to count its balance up.
const BALANCE_COUNT_TIMEOUT_SECONDS := 5.0

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
	if state.screen == "ContractHub":
		state["choosing_contract"] = screen.is_choosing_contract()
		state["change_contract_offered"] = screen.change_contract_button.visible
	elif state.screen == "VictoryScreen":
		state["contract_reward"] = _flow.contract_reward()
		state["victory_contract"] = screen.contract_label.text
		state["victory_reward"] = screen.reward_label.text
		state["victory_target_balance"] = screen.target_balance
	elif state.screen == "ShopScreen":
		state["shop_stock"] = screen.shop_node.stock()
	elif state.screen == "CaughtScreen":
		state["caught_fine"] = screen.get_node("%FineLabel").text
		state["caught_upgrades"] = screen.get_node("%UpgradesLabel").text
	elif state.screen == "CardReward":
		var offered_ids := []
		for card: Dictionary in screen.offered:
			offered_ids.append(card.get("id", ""))
		state["reward_offer"] = offered_ids
		state["reward_status"] = screen.credits_label.text
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
	_check_a_card_reward_naming_a_non_card_is_not_saved()
	if not await _start_and_accept_a_contract():
		_finish("")
		return

	print("-- resuming at the map after changing contract, before any encounter")
	if not await _change_contract_on_the_map():
		_finish("")
		return
	_write_expected_state("contract_chosen")
	_resume_in_a_new_process("contract_chosen")

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

	print("-- resuming on the victory screen")
	await _check_victory_resume()

	_remove_test_files()
	_finish("save continue test: PASSED")


func _remove_test_files() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(EXPECTED_PATH))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(UNREADABLE_COPY_PATH))


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

	var corrupt_text := "{\"version\": 1, \"meta\": {\"credits\": 999"
	_write_file(SAVE_PATH, corrupt_text)
	var corrupt: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(not corrupt.get("ok", true), "a corrupt save is refused")
	_check(not _flow.has_saved_run(), "a corrupt save leaves no run to continue")
	_check(_player.money() == credits_before, "a corrupt save changes no credits")
	_check(
		FileAccess.get_file_as_string(UNREADABLE_COPY_PATH) == corrupt_text,
		"a corrupt save is kept as a .unreadable copy before anything replaces it"
	)
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
	_check(_player.money() == 999, "an incompatible save's credit balance is kept")
	_player.add_credits(credits_before - _player.money())

	# Version 1 kept whether the contract reward was claimed, and the payout
	# beside the screen, rather than the reward run state now records.
	_write_file(
		SAVE_PATH,
		JSON.stringify({
			"version": 1,
			"meta": {"credits": 999},
			"run": {
				"run_state": {"phase": "contract_won", "contract_reward_claimed": true},
				"screen": {
					"contract_payout": {"credits": 500, "balance_before": 499, "balance_after": 999},
				},
			},
		})
	)
	var version_1: Dictionary = _flow.use_save_file(SAVE_PATH)
	_check(
		not version_1.get("ok", true) and str(version_1.get("error", "")).contains("version"),
		"a version 1 save is refused as incompatible"
	)
	_check(not _flow.has_saved_run(), "a version 1 save leaves no run to continue")
	_check(_player.money() == 999, "a version 1 save's credit balance is kept")
	_player.add_credits(credits_before - _player.money())
	await _reopen_menu()


# A card id that is not a string (a null from a missing "id", say) is refused
# when saving, rather than written out as text that would make the next launch
# refuse the whole save as naming an unknown card.
func _check_a_card_reward_naming_a_non_card_is_not_saved() -> void:
	var before := FileAccess.get_file_as_string(SAVE_PATH)
	var screen_results := {
		"pending_reward": {"credits": 25, "guaranteed_rare": false, "offered_card_ids": [null]},
		"caught_penalty": {},
	}
	var written: Dictionary = _flow.get_node("SaveGame").write_save(SAVE_PATH, screen_results)
	_check(
		not written.get("ok", true) and str(written.get("error", "")).contains("not a name"),
		"a card reward naming a non-card is refused for that reason"
	)
	_check(
		FileAccess.get_file_as_string(SAVE_PATH) == before,
		"the refused save leaves the file as it was"
	)


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
		int(fresh.get("version", -1)) == SAVE_VERSION and _saved_phase() == "hub",
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


# A run can only get under way on an accepted offer (a save of one that has
# not is refused), so a freshly rolled run takes its first offer straight away,
# as the hub's picker would.
func _accept_first_offer() -> void:
	var accepted: Dictionary = _flow.accept_contract_offer(0)
	_check(accepted.get("ok", false), "a fresh run accepts its first offer")


# Cycles the hub's offer picker to the offer with `index` and accepts it.
func _choose_offer_on_picker(hub: Node, index: int) -> bool:
	var picker: Node = hub.offer_picker
	for _press in picker.offers.size():
		if int(picker.shown_offer().get("index", -1)) == index:
			break
		picker.next_button.pressed.emit()
	if int(picker.shown_offer().get("index", -1)) != index:
		return false

	picker.accept_button.pressed.emit()
	return not hub.is_choosing_contract()


# Uses Change contract on the map to swap the accepted offer for another one,
# as the player may until they enter an encounter. The swap is saved at once.
func _change_contract_on_the_map() -> bool:
	var hub := _screen()
	if hub == null or hub.name != "ContractHub":
		_check(false, "the map is showing after accepting a contract")
		return false
	_check(hub.change_contract_button.visible, "the map offers Change contract before any encounter")

	var accepted_index := int(_flow.accepted_contract_offer().get("index", -1))
	hub.change_contract_button.pressed.emit()
	_check(
		int(hub.offer_picker.shown_offer().get("index", -1)) == accepted_index,
		"Change contract opens the picker on the accepted offer"
	)
	var other_index := 1 if accepted_index == 0 else 0
	_check(_choose_offer_on_picker(hub, other_index), "another offer can be chosen instead")
	_check(
		int(_flow.accepted_contract_offer().get("index", -1)) == other_index,
		"changing contract puts the run on the other offer"
	)
	var saved_state: Dictionary = _saved().get("run", {}).get("run_state", {})
	_check(
		saved_state.get("accepted_offer_index") == float(other_index)
		and saved_state.get("contract_started") == false,
		"the changed contract is saved, still not started"
	)
	return hub.change_contract_button.visible


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
			_screen()._on_continue_pressed()
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
	await _pass_through_shops()
	return _screen_name() == "ContractHub"


# A map can offer nothing but shops for a step or two. Passing through them,
# saved like any completed encounter, leaves a fight or an event to quit
# partway through next.
func _pass_through_shops() -> void:
	for _shop in MAX_SHOPS_IN_A_ROW:
		if not _first_selectable(["combat", "elite", "boss", "event"]).is_empty():
			return
		var shop := _first_selectable(["shop"])
		if shop.is_empty():
			return
		_flow.select_encounter(int(shop.id))
		await _settle()
		var left: Dictionary = _flow.complete_active_encounter()
		_check(left.get("ok", false), "a shop with nothing else on offer is passed through")
		await _settle()


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
		_accept_first_offer()
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
	var saved_visit = _saved().get("run", {}).get("run_state", {}).get("shop_visit")
	var saved_stock: Array = saved_visit.get("slots", []) if saved_visit is Dictionary else []
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
		_accept_first_offer()
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


# Starts a new run, takes its shortest contract and plays every encounter to
# the final boss, then resumes on the victory screen. The reward is paid when
# the boss goes down and saved in the balance; Continue must show it again
# without paying it a second time.
func _check_victory_resume() -> void:
	_flow.start_new_run()
	await _settle()
	var hub := _screen()
	if hub == null or hub.name != "ContractHub" or not _choose_offer_on_picker(hub, 0):
		_check(false, "the shortest contract is accepted on the picker")
		return
	await _settle()
	var credit_reward := int(_flow.accepted_contract_offer().get("credit_reward", 0))
	_check(credit_reward > 0, "the accepted contract carries a credit reward")

	if not await _play_to_the_final_boss():
		return
	var money_before_boss: int = _player.money()
	var completed: Dictionary = _flow.complete_active_encounter()
	await _settle()
	_check(
		completed.get("ok", false) and _screen_name() == "VictoryScreen",
		"beating the final boss opens the victory screen"
	)
	if _screen_name() != "VictoryScreen":
		return

	var boss_credits: int = _flow.pending_reward_credits()
	var expected_money := money_before_boss + boss_credits + credit_reward
	_check(_player.money() == expected_money, "the boss and the contract reward are paid once")
	var reward_text: String = _screen().reward_label.text
	_check(
		reward_text.contains("+%d" % credit_reward) and reward_text.contains("+%d" % boss_credits),
		"the victory screen shows the contract reward and the boss payout (%s)" % reward_text
	)
	var saved := _saved()
	var saved_run: Dictionary = saved.get("run", {})
	_check(_saved_phase() == "contract_won", "winning the contract is saved")
	_check(
		saved_run.get("run_state", {}).get("contract_reward") == float(credit_reward),
		"the save records the contract reward in run state"
	)
	_check(
		not saved_run.get("screen", {}).has("contract_payout"),
		"the save keeps no separate contract payout"
	)
	_check(
		int(saved.get("meta", {}).get("credits", -1)) == expected_money,
		"the saved balance already includes the reward"
	)
	_write_expected_state("victory")
	_resume_in_a_new_process("victory")


# Plays the accepted contract from the map up to its final boss, preferring
# bosses so the walk is short, taking each card reward on the way. Returns
# whether the final boss is the active encounter.
func _play_to_the_final_boss() -> bool:
	var route_length := int(_flow.contract_progress().get("total", 0))
	for _step in route_length:
		var next := _first_selectable(["boss", "combat", "elite", "event", "shop"])
		if next.is_empty():
			break
		_flow.select_encounter(int(next.id))
		await _settle()
		if _flow.active_encounter_wins_contract():
			return true

		_flow.complete_active_encounter()
		await _settle()
		if _screen_name() == "CardReward":
			_screen()._on_card_chosen(0)
			_screen()._on_continue_pressed()
			await _settle()
	_check(false, "the final boss is reached within the contract's route")
	return false


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
	match scenario:
		"hub":
			# Two, plus any shops passed through to reach a fight.
			_check(
				_completed_encounters(actual.get("map", [])) >= 2,
				"the completed encounters are still marked completed"
			)
			_check(
				not actual.get("change_contract_offered", true),
				"Change contract is not offered once an encounter has been played"
			)
			await _check_new_run_replaces_the_save()
		"contract_chosen":
			await _check_contract_can_still_change()
		"victory":
			await _check_resumed_victory(int(expected.get("credits", -1)))

	_finish(RESUME_PASSED_MARKER)


# The resumed run has not entered an encounter, so the map still offers
# Change contract, and changing works and is saved.
func _check_contract_can_still_change() -> void:
	var hub := _screen()
	if hub == null or hub.name != "ContractHub":
		_check(false, "Continue shows the contract hub")
		return
	_check(not hub.is_choosing_contract(), "Continue shows the map, not the offer picker")
	_check(hub.change_contract_button.visible, "Continue still offers Change contract")

	var accepted_index := int(_flow.accepted_contract_offer().get("index", -1))
	hub.change_contract_button.pressed.emit()
	_check(
		int(hub.offer_picker.shown_offer().get("index", -1)) == accepted_index,
		"Change contract opens the picker on the contract accepted before closing"
	)
	var other_index := 2 if accepted_index != 2 else 0
	_check(_choose_offer_on_picker(hub, other_index), "the contract can be changed after continuing")
	_check(
		_saved().get("run", {}).get("run_state", {}).get("accepted_offer_index")
		== float(other_index),
		"the change after continuing is saved"
	)


# The resumed victory screen counts up to the balance the reward was already
# added to, and finishing the count pays nothing more.
func _check_resumed_victory(expected_money: int) -> void:
	var victory := _screen()
	if victory == null or victory.name != "VictoryScreen":
		_check(false, "Continue shows the victory screen")
		return
	var reward := int(_flow.contract_reward().get("credits", 0))
	_check(reward > 0, "the resumed run still knows what the contract paid")

	var deadline := Time.get_ticks_msec() + int(BALANCE_COUNT_TIMEOUT_SECONDS * 1000)
	while victory.is_counting() and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(not victory.is_counting(), "the resumed victory screen finishes counting")
	_check(
		victory.displayed_balance == expected_money and _player.money() == expected_money,
		"the count ends on the saved balance, so the reward is not paid twice"
	)

	victory.new_contract_button.pressed.emit()
	await _settle()
	var hub := _screen()
	_check(
		hub != null and hub.name == "ContractHub" and hub.is_choosing_contract(),
		"Take another contract after continuing asks for a new contract"
	)
	var saved_state: Dictionary = _saved().get("run", {}).get("run_state", {})
	_check(
		_saved_phase() == "hub" and saved_state.get("contract_reward") == null,
		"the new run replaces the won one in the save"
	)
	_check(_player.money() == expected_money, "starting a new run pays nothing more")


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

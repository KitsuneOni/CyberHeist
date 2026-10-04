# Real scene/extension coverage for story #121: the run's stats, and starting a
# new run. The Rust tests own the counting and reset rules; this checks that
# they reach GDScript through FlowCoordinator in the shape the end screen will
# read, that credits and cards gained through the coordinator are counted, and
# that a new run also clears what lives outside the Rust run state: the shared
# noise meter, knowledge and the run's deck.
#
# Run: godot --headless --path godot -s res://tests/run_stats_test.gd
extends SceneTree

const SUMMARY_FIELDS := [
	"combats_won",
	"credits_earned",
	"cards_added",
	"encounters_completed",
	"total_encounters",
	"zones_cleared",
	"total_zones",
]
# The encounter types that count as a fight won when completed.
const FIGHT_TYPES := ["combat", "elite", "boss"]

var _flow: Node
var _noise_meter: Node
var _knowledge_meter: Node
var _player_state: Node
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
		print("run stats test: PASSED")
		quit(0)
	else:
		print("run stats test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _has_every_field(summary: Dictionary) -> bool:
	for field in SUMMARY_FIELDS:
		if not summary.has(field):
			return false
	return true


func _settle() -> void:
	# Screen replacement frees the old screen at the end of the frame.
	await process_frame
	await process_frame


# Enters the first node the hub offers and returns its type, or "" if nothing
# could be entered.
func _enter_first_encounter() -> String:
	var options: Array = _flow.selectable_encounters()
	if options.is_empty():
		return ""
	var chosen: Dictionary = options[0]
	var selected: Dictionary = _flow.select_encounter(int(chosen.get("id", -1)))
	if not selected.get("ok", false):
		push_error("could not enter %s: %s" % [chosen, selected])
		return ""
	return str(chosen.get("type", ""))


# Walks the contract until its first boss is beaten, without playing the
# encounters on the way. Returns how many of them were fights, or -1 if the
# walk never got through a boss.
func _clear_first_zone() -> int:
	var fights_won := 0
	for _step in 20:
		var encounter_type := _enter_first_encounter()
		if encounter_type.is_empty():
			return -1
		await _settle()

		var completed: Dictionary = _flow.complete_active_encounter()
		if not completed.get("ok", false):
			push_error("could not complete %s: %s" % [encounter_type, completed])
			return -1
		await _settle()

		if encounter_type in FIGHT_TYPES:
			fights_won += 1
		if encounter_type == "boss":
			return fights_won
	return -1


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_knowledge_meter = root.get_node_or_null("/root/KnowledgeMeterGlobal")
	_player_state = root.get_node_or_null("/root/PlayerStateGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	_check(_knowledge_meter != null, "KnowledgeMeterGlobal autoload is present")
	_check(_player_state != null, "PlayerStateGlobal autoload is present")
	if not _failures.is_empty():
		_finish()
		return

	for method in ["run_summary", "change_credits", "add_card_to_deck", "start_new_run"]:
		_check(_flow.has_method(method), "FlowCoordinator has %s()" % method)
	if not _failures.is_empty():
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()

	var starter_deck_size: int = _flow.run_deck_size()
	var starting_knowledge: int = _knowledge_meter.knowledge

	# --- a fresh run has nothing to report ---------------------------------
	var fresh: Dictionary = _flow.run_summary()
	_check(_has_every_field(fresh), "the summary carries every field: %s" % fresh)
	_check(int(fresh.get("combats_won", -1)) == 0, "a fresh run has won nothing")
	_check(int(fresh.get("credits_earned", -1)) == 0, "a fresh run has earned nothing")
	_check(int(fresh.get("cards_added", -1)) == 0, "a fresh run has added no cards")

	# --- acceptance test 1: stats are tracked during a run -----------------
	var fights_won: int = await _clear_first_zone()
	_check(fights_won > 0, "the walk got through the first zone's boss")
	if fights_won <= 0:
		_finish()
		return

	var money_before: int = _player_state.money()
	_flow.change_credits(120)
	_flow.change_credits(-80)
	_flow.change_credits(60)
	_check(
		_player_state.money() == money_before + 100,
		"credit changes reach the player's balance"
	)

	var offer: Array = _flow.offer_card_reward(1)
	_check(offer.size() == 1, "a card is on offer to add")
	if offer.is_empty():
		_finish()
		return
	var card_id: String = offer[0].get("id", "")
	_check(_flow.add_card_to_deck(card_id), "a card can be added to the deck")
	_check(_flow.take_card_reward(card_id), "a reward card can be taken")
	_check(_flow.add_card_to_deck(card_id), "a second copy can be added")
	_check(not _flow.add_card_to_deck("not_a_real_card"), "an unknown card is refused")

	var summary: Dictionary = _flow.run_summary()
	_check(
		int(summary.get("combats_won", -1)) == fights_won,
		"every fight completed is a win (expected %d, got %s)" % [fights_won, summary]
	)
	_check(
		int(summary.get("credits_earned", -1)) == 180,
		"only credit gains count as earned (got %s)" % summary
	)
	_check(
		int(summary.get("cards_added", -1)) == 3,
		"every card added is counted once, and a refused one is not (got %s)" % summary
	)
	_check(int(summary.get("zones_cleared", -1)) == 1, "beating the boss cleared a zone")
	var completed_on_map: int = _flow.contract_progress().get("completed", -2)
	_check(
		int(summary.get("encounters_completed", -1)) == completed_on_map,
		"encounters completed agrees with the contract progress"
	)

	# --- a new run cannot start in the middle of an encounter --------------
	var entered_type := _enter_first_encounter()
	_check(not entered_type.is_empty(), "the next zone can be entered")
	await _settle()
	var mid_encounter: Dictionary = _flow.start_new_run()
	_check(not mid_encounter.get("ok", true), "a new run is refused during an encounter")
	_check(
		not String(mid_encounter.get("error", "")).is_empty(),
		"the refusal says why"
	)
	_check(
		_flow.run_snapshot().get("phase", "") == "encounter_active",
		"a refused new run leaves the encounter running"
	)
	_check(
		_flow.run_summary() == summary,
		"a refused new run leaves the stats alone"
	)
	_flow.report_caught()
	await _settle()

	# Leave state behind that the new run must not inherit.
	_noise_meter.add_noise(30)
	_knowledge_meter.add_knowledge(1)
	_check(_noise_meter.noise > 0, "the old run ends with noise on the meter")
	_check(_flow.run_deck_size() > starter_deck_size, "the old run ends with a bigger deck")
	var money_at_end: int = _player_state.money()

	# --- acceptance test 2: a new run starts clean -------------------------
	var started: Dictionary = _flow.start_new_run()
	_check(started.get("ok", false), "a new run starts after being caught: %s" % started)
	await _settle()

	_check(_noise_meter.noise == 0, "noise is back to 0")
	_check(
		_knowledge_meter.knowledge == starting_knowledge,
		"knowledge is back to its starting level"
	)
	_check(_flow.run_deck_size() == starter_deck_size, "the deck is the starter deck again")

	var zones: Dictionary = _flow.zone_progress()
	_check(int(zones.get("current_zone", -1)) == 0, "the new run starts in the first zone")
	_check(int(zones.get("unlocked_zones", -1)) == 0, "no zone is cleared on the new run")
	_check(
		int(_flow.contract_progress().get("completed", -1)) == 0,
		"no encounter is completed on the new run"
	)

	var reset: Dictionary = _flow.run_summary()
	_check(int(reset.get("combats_won", -1)) == 0, "the new run has won nothing yet")
	_check(int(reset.get("credits_earned", -1)) == 0, "the new run has earned nothing yet")
	_check(int(reset.get("cards_added", -1)) == 0, "the new run has added no cards yet")

	var snapshot: Dictionary = _flow.run_snapshot()
	_check(snapshot.get("phase", "") == "hub", "the new run starts at the hub")
	_check(snapshot.get("active_encounter") == null, "the new run has no active encounter")
	var screen := _screen()
	_check(
		screen != null and screen.name == "ContractHub",
		"starting a new run shows the hub"
	)
	_check(
		not _flow.accepted_contract_offer().get("ok", true),
		"the new run has not accepted a contract"
	)
	_check(
		_flow.accept_contract_offer(0).get("ok", false),
		"the new run can accept a contract"
	)
	_check(
		_player_state.money() == money_at_end,
		"credits are the player's balance and carry over to the new run"
	)

	_finish()

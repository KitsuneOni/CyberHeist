# Covers story #54: when a run ends in detection the caught screen says so
# clearly and summarises the run, the same way the victory screen does for a
# win. The victory side is covered by contract_victory_test.gd.
#
# Run it with:
#
#     godot --headless --path godot -s res://tests/end_screen_test.gd
extends SceneTree

const RunSummaryText := preload("res://scenes/run_summary_text.gd")
const CombatFixture := preload("res://tests/combat_fixture.gd")
const WATCHDOG_SECONDS := 60.0
# Credits picked up mid-run, so the summary has something to report.
const EARNED := 30

var _flow: Node
var _player: Node
var _failures: Array[String] = []
var _finished := false
var _elapsed := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _process(delta: float) -> bool:
	if _finished:
		return true
	_elapsed += delta
	if _elapsed >= WATCHDOG_SECONDS:
		print("end screen test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
		_finished = true
		quit(1)
		return true
	return false


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
	_finished = true
	if _failures.is_empty():
		print("end screen test: PASSED")
		quit(0)
	else:
		print("end screen test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _label(screen: Node, unique_name: String) -> String:
	var label := screen.get_node_or_null("%" + unique_name) as Label
	return label.text if label != null else "<missing %s>" % unique_name


func _run() -> void:
	await process_frame
	_flow = root.get_node("FlowCoordinator")
	_player = root.get_node("PlayerStateGlobal")
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame

	print("Shared summary text")
	var text := RunSummaryText.summary_text(
		{
			"zones_cleared": 1,
			"total_zones": 3,
			"combats_won": 4,
			"cards_added": 2,
			"credits_earned": 75,
		}
	)
	_check(
		text == "Zones cleared: 1 of 3  ·  Fights won: 4  ·  Cards added: 2\nCredits earned this run: 75",
		"the summary reads the same as before it was shared (got '%s')" % text
	)
	_check(
		RunSummaryText.contract_text({"ok": false}) == "Default contract",
		"a run with no accepted offer is labelled the default contract"
	)

	print("Caught screen")
	var selected: Dictionary = CombatFixture.enter(_flow)
	_check(selected.get("ok", false), "combat fixture loads through the real flow lifecycle")
	if not selected.get("ok", false):
		_finish()
		return
	await process_frame
	_flow.change_credits(EARNED)

	var caught: Dictionary = _flow.report_caught()
	_check(caught.get("ok", false), "the player is caught")
	await process_frame
	await process_frame
	var screen := _screen()
	_check(screen != null and screen.name == "CaughtScreen", "the caught screen is shown")
	if screen == null or screen.name != "CaughtScreen":
		_finish()
		return

	_check(_label(screen, "TitleLabel") == "DETECTED", "the screen says plainly that the run was detected")
	_check(
		_label(screen, "ContractLabel")
		== RunSummaryText.contract_text(_flow.accepted_contract_offer()),
		"it names the contract (got '%s')" % _label(screen, "ContractLabel")
	)
	var summary: Dictionary = _flow.run_summary()
	_check(
		_label(screen, "SummaryLabel") == RunSummaryText.summary_text(summary),
		"it summarises the run like the victory screen (got '%s')" % _label(screen, "SummaryLabel")
	)
	_check(
		_label(screen, "SummaryLabel").contains("Credits earned this run: %d" % EARNED),
		"the summary includes what this run earned before it was caught"
	)
	_check(
		_label(screen, "FineLabel") == "Fined: $%d" % screen.fine_amount,
		"the fine is still shown"
	)
	_check(not _label(screen, "UpgradesLabel").is_empty(), "the upgrades outcome is still shown")
	var button := screen.get_node_or_null("%ContinueButton") as Button
	_check(button != null and button.has_focus(), "the way out has keyboard focus")

	print("Leaving")
	if button != null:
		button.pressed.emit()
	await process_frame
	await process_frame
	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "Back to the map returns to the hub")

	_finish()

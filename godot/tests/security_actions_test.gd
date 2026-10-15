# Real scene/extension coverage for Trello #102: standard security does more
# than add noise. The Rust tests own the rules; this drives a real first-zone
# encounter against WARDEN-7 and checks what the player actually sees.
#
#   - E4a: Firewall Up is announced with the intent before the turn ends,
#     leaves a firewall of 8 standing once it runs, and that firewall is used
#     up by plain damage before any integrity is lost.
#   - E4b: Inject Malware is announced, then puts an Injected Malware card
#     in the discard pile (named apart from the player's own Malware attack).
#     It costs 1 energy, does nothing when played and leaves the encounter,
#     and none of it reaches the run's deck once the encounter is over.
#
# Run: godot --headless --path godot -s res://tests/security_actions_test.gd
extends SceneTree

const FIREWALL_UP_BLOCK := 8
const STRIKE_DAMAGE := 6
const MALWARE_ID := "injected_malware"
const MALWARE_NAME := "Injected Malware"
# A script error inside _run kills the coroutine before it reaches _finish, and
# the engine would then sit there until CI's job timeout. The watchdog turns
# that into a fast failure instead.
const WATCHDOG_SECONDS := 60.0

const CombatFixture := preload("res://tests/combat_fixture.gd")

var _flow: Node
var _combat: Node
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
		print("")
		print("security actions test: TIMED OUT after %.0fs" % WATCHDOG_SECONDS)
		print("  The run stopped part way through, usually a script error above.")
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
	_combat.get_node("%EndTurnButton").pressed.emit()


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
	_combat.get_node("%PlayButton").pressed.emit()
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


# How many Malware cards this encounter's deck holds, wherever they are.
func _malware_in_encounter() -> int:
	var draw: Node = _combat.draw_phase
	var count := 0
	for card_name in draw.hand_names():
		if card_name == MALWARE_NAME:
			count += 1
	for pile in [draw.draw_pile_ids(), draw.discard_pile_ids()]:
		for id in pile:
			if id == MALWARE_ID:
				count += 1
	return count


# Ends the turn on Inject Malware without drawing a new hand, so the piles are
# left exactly as the security phase made them.
func _end_turn_on_inject_malware() -> void:
	var draw: Node = _combat.draw_phase
	var hand_size: int = draw.hand_size
	draw.hand_size = 0
	_press_end_turn()
	draw.hand_size = hand_size


# E4b scenario 3: Inject Malware is announced, then adds Malware to the
# discard pile.
func _check_inject_malware() -> void:
	_check(_end_turns_until_queued("Inject Malware"), "WARDEN-7 queues Inject Malware")
	_check(
		_combat.intent_label.text
		== "WARDEN-7 will: Inject Malware (+1 noise, adds 1 Injected Malware to your discard)",
		"the intent shows the Malware before the turn ends (got '%s')" % _combat.intent_label.text
	)
	var malware_before := _malware_in_encounter()
	var discard_before: int = _combat.draw_phase.discard_pile_ids().count(MALWARE_ID)

	_end_turn_on_inject_malware()
	_check(
		_combat.draw_phase.discard_pile_ids().count(MALWARE_ID) == discard_before + 1,
		"one Malware lands in the discard pile"
	)
	_check(_malware_in_encounter() == malware_before + 1, "and nowhere else")
	_check(
		_combat.status_label.text.contains("1 Injected Malware added to your discard pile."),
		"the turn report says Malware was planted (got '%s')" % _combat.status_label.text
	)


# E4b scenario 4: Malware is a dead card that leaves the encounter.
func _check_malware_is_dead() -> void:
	_draw_whole_deck()
	var draw: Node = _combat.draw_phase
	var index: int = draw.hand_names().find(MALWARE_NAME)
	_check(index >= 0, "Malware can be drawn")
	if index < 0:
		return
	_check(draw.hand_costs()[index] == 1, "Malware costs 1 energy")

	var malware_before := _malware_in_encounter()
	var energy_before: int = draw.energy
	var noise_before: int = _combat.sentry.noise()
	var health_before: int = _combat.sentry.health()
	var hand_before: int = draw.hand_names().size()
	if not _play(MALWARE_NAME):
		return

	_check(draw.energy == energy_before - 1, "playing Malware spends 1 energy")
	_check(_combat.sentry.noise() == noise_before, "Malware makes no noise")
	_check(_combat.sentry.health() == health_before, "Malware deals no damage")
	_check(draw.hand_names().size() == hand_before - 1, "Malware leaves the hand")
	_check(_malware_in_encounter() == malware_before - 1, "and is removed from the encounter")
	_check(
		_combat.status_label.text.contains("Injected Malware leaves the encounter."),
		"the play report says it is gone (got '%s')" % _combat.status_label.text
	)


# E4b scenario 5: Malware does not follow the player out of the encounter.
func _check_malware_stays_behind() -> void:
	var run_deck: Node = _flow.get_node("RunDeck")
	var owned_before: int = _flow.run_deck_size()

	_check(_end_turns_until_queued("Inject Malware"), "WARDEN-7 queues Inject Malware again")
	_end_turn_on_inject_malware()
	_check(_malware_in_encounter() > 0, "Malware is in the deck when the encounter ends")

	var completed: Dictionary = _flow.complete_active_encounter()
	_check(completed.get("ok", false), "the encounter can be completed")
	_check(run_deck.count_of(MALWARE_ID) == 0, "no Malware is in the run's deck afterwards")
	_check(_flow.run_deck_size() == owned_before, "the run's deck is the size it was")

	await process_frame
	# A fight offers a card reward first; the fixture's combat on an event or
	# shop node goes straight back to the map.
	var after := _screen()
	if after != null and after.name == "CardReward":
		_check(_flow.finish_reward().get("ok", false), "the card reward can be skipped")
		await process_frame
	# The fixture never replaces a run part-way through, so when the next
	# column only offers elites or the boss, move to a fresh contract on
	# purpose. The run's deck is what is being checked, and rolling the
	# contract leaves it alone.
	if not CombatFixture.offers_standard_security(_flow):
		print("  note  the next column only offers elites or the boss; rolling a fresh contract")
		CombatFixture.roll_new_contract(_flow)
	var selected: Dictionary = CombatFixture.enter(_flow)
	_check(selected.get("ok", false), "the next encounter can be entered")
	await process_frame
	await process_frame
	_combat = _screen()
	if _combat == null or _combat.name != "CombatScreen":
		_check(false, "the next encounter is a combat screen")
		return
	_check(_malware_in_encounter() == 0, "the next encounter's deck has no Malware")


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
	# Nothing has been entered on this contract, so the fixture enters a real
	# combat node, guarded by standard security, rolling contracts if it must.
	var entered: Dictionary = CombatFixture.enter(_flow)
	_check(entered.get("ok", false), "a first-zone combat node can be entered (%s)" % entered)
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
	_check_inject_malware()
	_check_malware_is_dead()
	await _check_malware_stays_behind()
	_finish()

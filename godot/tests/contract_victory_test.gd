# Real scene/extension coverage for winning a contract (Trello #89) and being
# paid for it (Trello #23). The Rust tests own the rules: which boss wins the
# contract, and that the reward is paid once and never for a caught contract.
# This plays a whole contract, chosen on the hub's offer picker, through
# FlowCoordinator and checks what the player actually sees:
#
#   - a new run asks for a contract before showing the map, and accepting one
#     through the picker puts the run on that contract,
#   - an earlier zone's boss only opens the next zone and returns to the map,
#   - beating the final zone's boss in real combat shows the victory screen
#     instead of the map,
#   - the accepted offer's credit reward lands in the balance exactly once,
#     and the victory screen counts the balance up to it in steps,
#   - Take another contract starts a clean new run at the hub's offer picker,
#     where the next contract is chosen, and
#   - a contract the player is caught on pays no contract reward.
#
# Run: godot --headless --path godot -s res://tests/contract_victory_test.gd
extends SceneTree

# How many fresh runs to roll looking for an offer with more than one zone.
# Every offer set so far has had one, so this only bounds a bad roll.
const MAX_RUN_ROLLS := 10
# Longer than the victory screen's count, so a count that never finishes fails
# the test instead of hanging it.
const BALANCE_COUNT_TIMEOUT_SECONDS := 5.0

var _flow: Node
var _run_state: Node
var _player_state: Node
var _failures: Array[String] = []
var _counted_balances: Array[int] = []
var _count_finished := false


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
		print("contract victory test: PASSED")
		quit(0)
	else:
		print("contract victory test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _settle() -> void:
	# Screen replacement frees the old screen at the end of the frame.
	await process_frame
	await process_frame


func _phase() -> String:
	return str(_flow.run_snapshot().get("phase", ""))


# Accepts the offer with the most zones through the hub's offer picker, so the
# contract has an earlier boss as well as a final one. Rolls fresh runs until
# one is on offer. Returns the accepted offer, or an empty dictionary if none
# could be accepted.
func _accept_a_multi_zone_offer() -> Dictionary:
	for _roll in MAX_RUN_ROLLS:
		var hub := _screen()
		if hub == null or hub.name != "ContractHub" or not hub.is_choosing_contract():
			push_error("the hub is not asking for a contract: %s" % hub)
			return {}

		var best: Dictionary = {}
		for offer: Dictionary in hub.offer_picker.offers:
			if best.is_empty() or int(offer.get("zone_count", 0)) > int(best.get("zone_count", 0)):
				best = offer

		if int(best.get("zone_count", 0)) >= 2:
			if _choose_offer_on_picker(hub, int(best.get("index", -1))):
				return best
			return {}

		_flow.start_new_run()
		await _settle()
	return {}


# Cycles the hub's offer picker to the offer with `index` and presses Accept,
# as a player would. True once the hub has swapped to the map.
func _choose_offer_on_picker(hub: Node, index: int) -> bool:
	var picker: Node = hub.offer_picker
	for _press in picker.offers.size():
		if int(picker.shown_offer().get("index", -1)) == index:
			break
		picker.next_button.pressed.emit()

	if int(picker.shown_offer().get("index", -1)) != index:
		push_error("offer %d never came up on the picker" % index)
		return false

	picker.accept_button.pressed.emit()
	if hub.is_choosing_contract():
		push_error("accepting offer %d left the picker up: %s" % [index, picker.status_label.text])
		return false
	return true


# The offer picker is the only way a player takes on a contract, so check it
# is what a new run opens on and that it describes the offers.
func _check_offer_picker(hub: Node, context: String) -> void:
	_check(
		hub != null and hub.name == "ContractHub" and hub.is_choosing_contract(),
		"%s asks for a contract before showing the map" % context
	)
	if hub == null or not hub.has_method("is_choosing_contract"):
		return
	_check(not hub.map_panel.visible, "%s hides the map until a contract is chosen" % context)

	var picker: Node = hub.offer_picker
	_check(
		picker.offers.size() == _flow.contract_offers().size() and not picker.offers.is_empty(),
		"%s offers every contract on the table" % context
	)
	var first: Dictionary = picker.shown_offer()
	_check(
		picker.name_label.text == str(first.get("name", "?"))
		and picker.payout_label.text.contains(str(first.get("credit_reward", "?"))),
		"%s shows the offer's name and payout (%s, %s)"
		% [context, picker.name_label.text, picker.payout_label.text]
	)

	picker.next_button.pressed.emit()
	_check(
		picker.shown_position == 1 and picker.name_label.text != str(first.get("name", "?")),
		"%s cycles to the next offer" % context
	)
	picker.previous_button.pressed.emit()
	_check(picker.shown_position == 0, "%s cycles back to the first offer" % context)


# Enters the next node on the route, preferring a boss so the walk is short.
# Returns the selection result, which carries the node's type and zone.
func _enter_next_encounter() -> Dictionary:
	var options: Array = _flow.selectable_encounters()
	if options.is_empty():
		return {"ok": false, "error": "the hub offers nothing to enter"}

	var chosen: Dictionary = options[0]
	for candidate: Dictionary in options:
		if str(candidate.get("type", "")) == "boss":
			chosen = candidate
			break
	return _flow.select_encounter(int(chosen.get("id", -1)))


# Completes everything up to the final zone's boss without playing it, checking
# each earlier boss on the way. Returns true once the final boss's combat
# screen is up.
func _advance_to_final_boss(total_zones: int) -> bool:
	for _step in 40:
		var entered := _enter_next_encounter()
		if not entered.get("ok", false):
			push_error("could not enter the next encounter: %s" % entered)
			return false
		await _settle()

		var is_boss: bool = entered.get("is_boss", false)
		var zone := int(entered.get("zone", -1))
		if is_boss and zone == total_zones - 1:
			return true

		if is_boss:
			await _check_earlier_boss(zone)
		else:
			var completed: Dictionary = _flow.complete_active_encounter()
			if not completed.get("ok", false):
				push_error("could not complete %s: %s" % [entered, completed])
				return false

		if _phase() == "hub" and _screen() != null and _screen().name == "CardReward":
			var left: Dictionary = _flow.finish_reward()
			if not left.get("ok", false):
				push_error("could not leave the card reward: %s" % left)
				return false
		await _settle()
	return false


# Acceptance scenario 3 (#89): an earlier zone's boss opens the next zone and
# leads back to the map, with no contract reward and no victory.
func _check_earlier_boss(zone: int) -> void:
	var money_before: int = _player_state.money()
	var unlocked_before := int(_flow.zone_progress().get("unlocked_zones", -1))

	var completed: Dictionary = _flow.complete_active_encounter()
	_check(completed.get("ok", false), "the zone-%d boss can be completed" % (zone + 1))
	_check(_phase() == "hub", "beating the zone-%d boss returns the run to the hub" % (zone + 1))
	_check(
		int(_flow.zone_progress().get("unlocked_zones", -1)) == unlocked_before + 1,
		"beating the zone-%d boss unlocks the next zone" % (zone + 1)
	)
	_check(
		_player_state.money() == money_before + _flow.pending_reward_credits(),
		"an earlier boss pays only its own encounter credits, not the contract reward"
	)
	_check(_flow.contract_payout().is_empty(), "an earlier boss does not win the contract")

	var screen := _screen()
	_check(
		screen != null and screen.name == "CardReward",
		"an earlier boss leads to its card reward, not a victory screen"
	)
	var left: Dictionary = _flow.finish_reward()
	_check(left.get("ok", false), "the card reward can be left")
	await _settle()
	screen = _screen()
	_check(
		screen != null and screen.name == "ContractHub",
		"the player returns to the contract map after an earlier boss"
	)
	_check(not _flow.selectable_encounters().is_empty(), "the next zone is on offer")


# Brings the boss down to one point of integrity and finishes it with a real
# Trojan play, so the win goes through the combat screen's defeat path.
func _defeat_boss_in_combat(combat: Node) -> bool:
	combat.sentry.take_damage(combat.sentry.health() - 1)

	# Fixture only: reuse the real draw API rather than add a debug gameplay API.
	var draw: Node = combat.draw_phase
	draw.hand_size = (draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count())
	combat.selected_index = -1
	combat._draw_hand()

	var index: int = draw.hand_names().find("Trojan")
	if index < 0:
		return false
	combat.card_buttons[index].pressed.emit()
	combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()
	return true


func _on_displayed_balance_changed(balance: int) -> void:
	_counted_balances.append(balance)


func _on_balance_count_finished() -> void:
	_count_finished = true


func _is_strictly_increasing(values: Array[int]) -> bool:
	for i in range(1, values.size()):
		if values[i] <= values[i - 1]:
			return false
	return true


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_player_state = root.get_node_or_null("/root/PlayerStateGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_player_state != null, "PlayerStateGlobal autoload is present")
	if not _failures.is_empty():
		_finish()
		return
	_run_state = _flow.get_node("RunState")

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()
	_flow.start_new_contract()
	await _settle()

	# --- set up: choose a contract with more than one zone -----------------
	_check_offer_picker(_screen(), "a new contract")
	var offer: Dictionary = await _accept_a_multi_zone_offer()
	_check(not offer.is_empty(), "an offer with more than one zone is accepted: %s" % offer)
	if offer.is_empty():
		_finish()
		return
	var accepted_offer: Dictionary = _flow.accepted_contract_offer()
	_check(
		accepted_offer.get("ok", false)
		and int(accepted_offer.get("index", -1)) == int(offer.get("index", -2)),
		"the picker accepts the chosen offer (%s)" % accepted_offer
	)
	var map_hub := _screen()
	_check(
		map_hub != null and map_hub.map_panel.visible and not map_hub.is_choosing_contract(),
		"accepting a contract shows its map"
	)
	var credit_reward := int(offer.get("credit_reward", 0))
	var total_zones := int(_flow.zone_progress().get("total_zones", 0))
	_check(credit_reward > 0, "the accepted contract carries a credit reward")
	_check(
		total_zones == int(offer.get("zone_count", -1)),
		"the run is on the accepted contract's %d zones" % total_zones
	)

	# --- #89 scenario 3: earlier bosses only unlock the next zone ----------
	var reached: bool = await _advance_to_final_boss(total_zones)
	_check(reached, "the run reaches the final zone's boss")
	var combat := _screen()
	_check(combat != null, "a screen is up for the final boss")
	_check(
		combat != null and combat.has_node("Sentry"),
		"the final boss loads the combat screen with its Sentry"
	)
	if not reached or combat == null or not combat.has_node("Sentry"):
		_finish()
		return
	_check(
		int(_flow.zone_progress().get("unlocked_zones", -1)) == total_zones - 1,
		"every earlier zone was opened on the way"
	)
	_check(_flow.contract_payout().is_empty(), "nothing has been paid for the contract yet")

	# --- #89 scenario 1: the final boss completes the contract -------------
	var money_before_boss: int = _player_state.money()
	var earned_before_boss := int(_flow.run_summary().get("credits_earned", -1))
	_check(_defeat_boss_in_combat(combat), "a Trojan is played at the final boss")

	_check(_phase() == "contract_won", "the contract is marked complete (phase %s)" % _phase())
	var victory := _screen()
	_check(
		victory != null and victory.name == "VictoryScreen",
		"the victory screen is shown instead of the contract map"
	)
	if victory == null or victory.name != "VictoryScreen":
		_finish()
		return
	_check(_flow.selectable_encounters().is_empty(), "a won contract offers nothing to enter")

	# --- #89 scenario 2 and #23: the reward is paid once -------------------
	var boss_credits: int = _flow.pending_reward_credits()
	var payout: Dictionary = _flow.contract_payout()
	var expected_balance := money_before_boss + boss_credits + credit_reward
	_check(
		int(payout.get("credits", -1)) == credit_reward,
		"the contract pays the accepted offer's credit reward (%s)" % payout
	)
	_check(
		_player_state.money() == expected_balance,
		"the reward is added to the balance (expected %d, got %d)"
		% [expected_balance, _player_state.money()]
	)
	_check(
		int(payout.get("balance_before", -1)) == money_before_boss + boss_credits
		and int(payout.get("balance_after", -1)) == expected_balance,
		"the payout records the balance before and after the reward"
	)
	_check(
		int(_flow.run_summary().get("credits_earned", -1))
		== earned_before_boss + boss_credits + credit_reward,
		"the reward counts as credits earned on the run"
	)

	var second_claim: Dictionary = _run_state.claim_contract_reward()
	_check(not second_claim.get("ok", true), "a second claim on the reward is refused")
	var completed_again: Dictionary = _flow.complete_active_encounter()
	_check(not completed_again.get("ok", true), "the final boss cannot be completed twice")
	_check(_player_state.money() == expected_balance, "the reward is paid exactly once")

	# --- #23: the balance counts up to its new total -----------------------
	_check(
		victory.displayed_balance == money_before_boss + boss_credits,
		"the count starts from the balance before the reward (%d)" % victory.displayed_balance
	)
	_check(victory.is_counting(), "the balance is still counting when the screen opens")
	victory.displayed_balance_changed.connect(_on_displayed_balance_changed)
	victory.balance_count_finished.connect(_on_balance_count_finished)
	var deadline := Time.get_ticks_msec() + int(BALANCE_COUNT_TIMEOUT_SECONDS * 1000.0)
	while not _count_finished and Time.get_ticks_msec() < deadline:
		await process_frame

	_check(_count_finished, "the balance finishes counting")
	_check(
		_counted_balances.size() >= 3,
		"the balance rises in several steps, not one jump (%d steps)" % _counted_balances.size()
	)
	_check(_is_strictly_increasing(_counted_balances), "the count only ever goes up")
	_check(
		not _counted_balances.is_empty() and _counted_balances[-1] == expected_balance,
		"the count ends on the new total"
	)
	_check(not victory.is_counting(), "the screen reports the count as done")
	_check(
		victory.balance_label.text.contains(str(expected_balance)),
		"the balance label shows the new total (%s)" % victory.balance_label.text
	)
	_check(
		victory.reward_label.text.contains("+%d" % credit_reward),
		"the screen says what the contract paid (%s)" % victory.reward_label.text
	)
	_check(
		victory.contract_label.text.contains(str(offer.get("name", "?"))),
		"the screen names the contract (%s)" % victory.contract_label.text
	)

	# --- #89 scenario 2: take another contract from the victory screen -----
	victory.new_contract_button.pressed.emit()
	await _settle()
	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "Take another contract shows the hub")
	_check(_phase() == "hub", "the new run starts at the hub")
	_check_offer_picker(hub, "the hub after a won contract")
	_check(
		not _flow.accepted_contract_offer().get("ok", true),
		"the new run has not accepted a contract yet"
	)
	_check(int(_flow.zone_progress().get("unlocked_zones", -1)) == 0, "no zone is open")
	_check(int(_flow.run_summary().get("credits_earned", -1)) == 0, "the new run has earned nothing")
	_check(_flow.contract_payout().is_empty(), "the old contract's payout is cleared")
	_check(_player_state.money() == expected_balance, "the credit balance carries over")
	_check(not _run_state.claim_contract_reward().get("ok", true), "the new contract has not been won")

	# --- a caught contract pays no contract reward -------------------------
	var next_offer: Dictionary = {}
	var offers_after_victory: Array = _flow.contract_offers()
	if not offers_after_victory.is_empty():
		next_offer = offers_after_victory[0]
	_check(
		hub != null and _choose_offer_on_picker(hub, 0),
		"the new run takes another contract through the picker"
	)
	_check(
		str(_flow.accepted_contract_offer().get("name", "")) == str(next_offer.get("name", "?")),
		"the new run is on the contract chosen after the victory"
	)
	var entered := _enter_next_encounter()
	_check(entered.get("ok", false), "the new contract's first encounter can be entered")
	await _settle()
	var money_before_caught: int = _player_state.money()
	var caught: Dictionary = _flow.report_caught()
	_check(caught.get("ok", false), "the player is caught")
	await _settle()
	var caught_screen := _screen()
	_check(caught_screen != null and caught_screen.name == "CaughtScreen", "the caught screen is shown")
	var fine: int = caught_screen.fine_amount if caught_screen != null else 0
	_check(not _run_state.claim_contract_reward().get("ok", true), "a caught contract cannot claim")
	_check(
		_player_state.money() == money_before_caught - fine,
		"being caught costs the fine and pays no contract reward"
	)
	_check(_flow.contract_payout().is_empty(), "a caught contract records no payout")

	_finish()

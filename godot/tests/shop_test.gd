# Real scene/extension coverage for the dark web shop (Trello #38), through
# its two stories S1 shop stock and pricing (#104) and S2 buying with credits
# (#105). The Rust tests own the rules; this plays a shop visit the way a player
# would, through the screens, and checks what only a running game can show.
#
#   S1 Scenario: Shop generates stock on entry
#     Given I have accepted a contract on the offer picker
#     And I select a shop node on the contract map
#     When the shop screen opens
#     Then I see 5 cards for sale with their full details
#     And each card's price is set by its rarity
#
#   S1 Scenario: Stock is fixed for the visit
#     Given I am in a shop
#     When I buy a card
#     Then that slot shows as sold
#     And the other cards stay the same
#     And reopening the shop screen shows the same stock, still sold
#
#   S2 Scenario: Successful purchase
#     When I buy a card I can afford
#     Then my credits drop by exactly its price
#     And the card is added to my run deck
#
#   S2 Scenario: Not enough credits
#     Given a card costs more than I have
#     Then the buy button for that card is disabled
#     And the price is shown in red
#     And my credits and deck do not change
#
#   S1 Scenario: Leaving the shop completes the node
#     When I press Leave
#     Then I return to the contract map
#     And the shop node is marked completed
#
#   #38: a purchased card appears in later draws during the run.
#
# Run: godot --headless --path godot -s res://tests/shop_test.gd
extends SceneTree

# Mirrors price_for in cyber_heist/src/shop.rs.
const PRICE_BY_RARITY := {
	"Common": 50,
	"Uncommon": 75,
	"Rare": 100,
	"Legendary": 150,
}
const STOCK_SIZE := 5
# The S2 scenario's balance. Every card costs at least 50, so a couple of
# purchases always leaves something the player cannot afford.
const STARTING_CREDITS := 120
const UNAFFORDABLE_COLOUR := Color(1, 0.2, 0.333333, 1)

# Generated contracts do not guarantee a shop, so the test rolls new runs until
# a first-zone route offers one. Roughly one node in seven is a shop, so this
# many runs without one would mean shops had stopped being generated.
const MAX_RUNS := 60

var _flow: Node
var _player: Node
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
		print("shop test: PASSED")
		quit(0)
	else:
		print("shop test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _settle() -> void:
	await process_frame
	await process_frame


# A new run opens the hub on the offer picker with the map hidden, so accept
# the offer it shows first, as a player must before the map appears. Returns
# whether the hub is showing the map.
func _accept_shown_offer(hub: Node) -> bool:
	if not hub.is_choosing_contract():
		return true
	hub.offer_picker.accept_button.pressed.emit()
	await _settle()
	if hub.is_choosing_contract():
		push_error(
			"accepting the shown offer left the picker up: %s"
			% hub.offer_picker.status_label.text
		)
		return false
	return true


# Accepts a contract on the offer picker, then walks the first zone of the run
# until the map offers a shop and clicks it on the map like a player would.
# Encounters on the way are completed without being played. Returns the shop's
# node id, or -1 if this run's first zone had none.
func _walk_to_first_zone_shop() -> int:
	var first_hub := _screen()
	if first_hub == null or first_hub.name != "ContractHub":
		return -1
	if not await _accept_shown_offer(first_hub):
		return -1

	for step in 10:
		var hub := _screen()
		if hub == null or hub.name != "ContractHub" or hub.is_choosing_contract():
			return -1

		var options: Array = _flow.selectable_encounters()
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) == "shop":
				var shop_id := int(candidate.id)
				# Walk in with the S2 scenario's balance, as if it had been
				# earned on the way here.
				_player.add_credits(STARTING_CREDITS - _player.money())
				hub.map_graph.node_selected.emit(shop_id)
				await _settle()
				return shop_id

		# Bosses close the zone, so a shop past one would not be zone 1.
		var chosen: Dictionary = {}
		for candidate: Dictionary in options:
			if str(candidate.get("type", "")) != "boss":
				chosen = candidate
				break
		if chosen.is_empty():
			return -1

		var selected: Dictionary = _flow.select_encounter(chosen.id)
		if not selected.get("ok", false):
			push_error("could not enter %s: %s" % [chosen, selected])
			return -1
		var completed: Dictionary = _flow.complete_active_encounter()
		if not completed.get("ok", false):
			push_error("could not complete %s: %s" % [chosen, completed])
			return -1
		if _screen() != null and _screen().name == "CardReward":
			_flow.finish_reward()
		await _settle()
	return -1


func _node_status(node_id: int) -> String:
	for entry: Dictionary in _flow.map_progress():
		if int(entry.get("id", -1)) == node_id:
			return str(entry.get("status", ""))
	return ""


func _stock_ids(stock: Array) -> Array[String]:
	var ids: Array[String] = []
	for card: Dictionary in stock:
		ids.append(str(card.get("id", "")))
	return ids


# The cheapest unsold card the player can afford, or -1 if there is none.
func _cheapest_affordable_slot(stock: Array) -> int:
	var best := -1
	for card: Dictionary in stock:
		if card.get("sold", false) or not card.get("affordable", false):
			continue
		if best < 0 or int(card.price) < int(stock[best].price):
			best = int(card.slot)
	return best


func _first_unaffordable_slot(stock: Array) -> int:
	for card: Dictionary in stock:
		if not card.get("sold", false) and not card.get("affordable", false):
			return int(card.slot)
	return -1


# Presses a Buy button the way a player could: only when it is enabled.
# Emitting `pressed` alone would bypass the disabled state, so a screen that
# wrongly disabled every affordable card would still appear to sell them.
# Returns whether the button could be pressed.
func _press_buy(shop: Node, slot: int, card_name: String) -> bool:
	var button: Button = shop._buy_buttons[slot]
	_check(not button.disabled, "the buy button for affordable %s is enabled" % card_name)
	if button.disabled:
		return false
	button.pressed.emit()
	return true


# Counts by id rather than display name: a card's name flips to its weak side
# at high noise, and two cards could share a name.
func _count_in(card_ids: PackedStringArray, card_id: String) -> int:
	var count := 0
	for drawn_id in card_ids:
		if drawn_id == card_id:
			count += 1
	return count


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_player = root.get_node_or_null("/root/PlayerStateGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_player != null, "PlayerStateGlobal autoload is present")
	if _flow == null or _player == null:
		_finish()
		return

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()

	# Start from the main menu, as a player does.
	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null:
		_finish()
		return
	menu.get_node("%NewContractButton").pressed.emit()
	await _settle()

	# --- find a shop on the map and click it ---------------------------------
	var shop_id := -1
	for attempt in MAX_RUNS:
		if attempt > 0:
			_flow.start_new_run()
			await _settle()
		shop_id = await _walk_to_first_zone_shop()
		if shop_id >= 0:
			break
	_check(shop_id >= 0, "a first-zone route offers a shop within %d runs" % MAX_RUNS)
	if shop_id < 0:
		_finish()
		return

	var shop := _screen()
	_check(shop != null and shop.name == "ShopScreen", "a shop node opens the shop screen")
	if shop == null or shop.name != "ShopScreen":
		_finish()
		return
	var active = _flow.run_snapshot().get("active_encounter")
	_check(
		typeof(active) == TYPE_DICTIONARY and active.get("type", "") == "shop",
		"the shop is the active encounter"
	)

	# --- S1: five cards with their full details, priced by rarity ------------
	var shop_node: Node = shop.get_node("ShopNode")
	var stock: Array = shop_node.stock()
	_check(stock.size() == STOCK_SIZE, "the shop has %d cards for sale (got %d)" % [STOCK_SIZE, stock.size()])
	_check(shop._rows.size() == STOCK_SIZE, "the screen shows a row for each card")
	_check(
		shop.get_node("%CreditsLabel").text.contains(str(STARTING_CREDITS)),
		"the credits on hand are shown"
	)

	var distinct := {}
	for card: Dictionary in stock:
		distinct[card.id] = true
		var slot := int(card.slot)
		var row_text: String = shop._rows[slot].get_child(0).text
		var detailed := (
			row_text.contains(String(card.name).to_upper())
			and row_text.contains(String(card.type))
			and row_text.contains(String(card.rarity))
			and row_text.contains(String(card.cost_text))
			and row_text.contains(String(card.noise_text))
			and row_text.contains(String(card.description))
		)
		_check(detailed, "slot %d shows the card's full details (%s)" % [slot, card.name])
		_check(
			int(card.price) == PRICE_BY_RARITY.get(card.rarity, -1),
			"slot %d's price is set by its rarity (%s %d cr)" % [slot, card.rarity, card.price]
		)
		_check(
			shop._price_labels[slot].text == "%d cr" % int(card.price),
			"slot %d shows its price" % slot
		)
	_check(distinct.size() == stock.size(), "the shop never stocks the same card twice")

	# --- S2: a successful purchase, and S1: the slot shows sold --------------
	var stock_before_buying := _stock_ids(stock)
	var deck_before: int = _flow.run_deck_size()
	var cards_added_before: int = _flow.run_summary().cards_added
	var earned_before: int = _flow.run_summary().credits_earned
	# Ids of the cards bought, one entry per copy.
	var bought: Array[String] = []

	# Stock can include starter cards, so remember how many copies the run
	# already owns; the later draw must show the bought copies on top of those.
	var run_deck: Node = _flow.get_node("RunDeck")
	var copies_before := {}
	for card: Dictionary in stock:
		copies_before[String(card.id)] = int(run_deck.count_of(String(card.id)))

	var slot_to_buy := _cheapest_affordable_slot(stock)
	_check(slot_to_buy >= 0, "with %d credits a card is affordable" % STARTING_CREDITS)
	if slot_to_buy < 0:
		_finish()
		return
	var target: Dictionary = stock[slot_to_buy]
	var credits_before: int = _player.money()

	if not _press_buy(shop, slot_to_buy, String(target.name)):
		_finish()
		return
	bought.append(String(target.id))
	stock = shop_node.stock()

	_check(
		_player.money() == credits_before - int(target.price),
		"buying %s takes exactly its price (%d -> %d)" % [target.name, credits_before, _player.money()]
	)
	_check(_flow.run_deck_size() == deck_before + 1, "the bought card is added to the run deck")
	_check(stock[slot_to_buy].sold, "the bought slot is sold")
	_check(shop._price_labels[slot_to_buy].text == "SOLD", "the bought slot shows as sold")
	_check(shop._buy_buttons[slot_to_buy].disabled, "a sold slot cannot be bought again")
	_check(
		_stock_ids(stock) == stock_before_buying,
		"the other cards stay the same after a purchase"
	)
	_check(
		shop.get_node("%CreditsLabel").text.contains(str(_player.money())),
		"the credits shown drop with the purchase"
	)

	# Rebuild the screen mid-visit, as resuming the run onto the shop does.
	# The visit belongs to the run, so it must not roll new stock or restock
	# the sold slot.
	_check(_flow._show_current_run_screen().get("ok", false), "the shop screen can be reopened")
	await _settle()
	shop = _screen()
	_check(shop != null and shop.name == "ShopScreen", "reopening shows the shop screen again")
	if shop == null or shop.name != "ShopScreen":
		_finish()
		return
	shop_node = shop.get_node("ShopNode")
	stock = shop_node.stock()
	_check(_stock_ids(stock) == stock_before_buying, "reopening the shop keeps its stock")
	_check(stock[slot_to_buy].sold, "reopening the shop keeps the bought slot sold")
	_check(shop._price_labels[slot_to_buy].text == "SOLD", "the reopened screen shows it as sold")

	# Spend down until some card is out of reach. Every card costs at least 50,
	# so from 120 this takes at most two purchases.
	var unaffordable_slot := _first_unaffordable_slot(stock)
	while unaffordable_slot < 0:
		var next_slot := _cheapest_affordable_slot(stock)
		if next_slot < 0:
			break
		var next_name := String(stock[next_slot].name)
		if not _press_buy(shop, next_slot, next_name):
			break
		bought.append(String(stock[next_slot].id))
		stock = shop_node.stock()
		unaffordable_slot = _first_unaffordable_slot(stock)

	# --- S2: not enough credits ----------------------------------------------
	_check(unaffordable_slot >= 0, "spending down leaves a card the player cannot afford")
	if unaffordable_slot >= 0:
		var out_of_reach: Dictionary = stock[unaffordable_slot]
		var credits_now: int = _player.money()
		var deck_now: int = _flow.run_deck_size()
		var price_label: Label = shop._price_labels[unaffordable_slot]
		_check(credits_now < int(out_of_reach.price), "the card costs more than the player has")
		_check(shop._buy_buttons[unaffordable_slot].disabled, "its buy button is disabled")
		_check(
			price_label.get_theme_color("font_color") == UNAFFORDABLE_COLOUR,
			"its price is shown in red"
		)

		# Force the attempt past the disabled button, as a stale click would.
		shop._on_buy_pressed(unaffordable_slot)
		var refused: Dictionary = shop_node.buy(unaffordable_slot)
		_check(refused.get("error", "") == "not_enough_credits", "the shop refuses the purchase")
		_check(_player.money() == credits_now, "a refused purchase leaves the credits alone")
		_check(_flow.run_deck_size() == deck_now, "a refused purchase leaves the deck alone")
		_check(
			not shop_node.stock()[unaffordable_slot].sold,
			"a refused card is still for sale"
		)
		_check(_player.money() >= 0, "credits never go negative")

	_check(
		_flow.run_summary().cards_added == cards_added_before + bought.size(),
		"bought cards count as cards added in the run summary"
	)
	_check(
		_flow.run_summary().credits_earned == earned_before,
		"spending credits does not change what the run earned"
	)

	# --- S1: leaving completes the node --------------------------------------
	shop.get_node("%LeaveButton").pressed.emit()
	await _settle()
	var hub := _screen()
	_check(
		hub != null and hub.name == "ContractHub" and not hub.is_choosing_contract(),
		"Leave returns to the contract map, not the offer picker"
	)
	_check(_flow.run_snapshot().phase == "hub", "no encounter is active after leaving")
	_check(_node_status(shop_id) == "completed", "the shop node is marked completed")

	# --- #38: bought cards are drawable in a later encounter ------------------
	var entered: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(entered.get("ok", false), "the next encounter loads")
	await _settle()
	var combat := _screen()
	if combat == null or combat.name != "CombatScreen":
		_check(false, "the combat screen is showing")
		_finish()
		return

	# Draw the whole deck so every bought card must turn up if it is there.
	var draw: Node = combat.draw_phase
	draw.hand_size = draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	combat._draw_hand()
	var hand: PackedStringArray = draw.hand_ids()
	var checked := {}
	for card_id in bought:
		if checked.has(card_id):
			continue
		checked[card_id] = true
		var expected: int = int(copies_before[card_id]) + bought.count(card_id)
		var drawn := _count_in(hand, card_id)
		_check(
			drawn == expected,
			(
				"every copy of the bought card '%s' can be drawn in a later encounter (%d of %d)"
				% [card_id, drawn, expected]
			)
		)

	_finish()

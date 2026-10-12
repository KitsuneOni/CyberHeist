# Real scene/extension coverage for the dark web shop (Trello #38), through
# its stories S1 shop stock and pricing (#104), S2 buying with credits (#105),
# S3 Scrub Logs (#106) and S4 rerolling the stock (#107). The Rust tests own
# the rules; this plays a shop visit the way a player would, through the
# screens, and checks what only a running game can show.
#
#   S1 Scenario: Shop generates stock on entry
#     Given I select a shop node on the contract map
#     When the shop screen opens
#     Then I see 5 cards for sale with their full details
#     And each card's price is set by its rarity
#
#   S1 Scenario: Stock is fixed for the visit
#     Given I am in a shop
#     When I buy a card
#     Then that slot shows as sold
#     And the other cards stay the same
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
#   S3 Scenario: Pay to lower noise
#     Given my noise is 70 out of 100
#     And I have enough credits for the "Scrub Logs" service
#     When I buy Scrub Logs
#     Then my noise drops by 20 to 50
#     And my credits are reduced by its price
#
#   S3 Scenario: Service has a limit
#     Given I have already bought Scrub Logs at this shop
#     Then Scrub Logs is shown as sold out for this visit
#
#   S4 Scenario: Reroll the stock
#     Given I am in a shop with 50 credits
#     And a reroll costs 25 credits
#     When I press Reroll
#     Then every unsold card is replaced with a new random card
#     And my credits drop to 25
#
#   S4 Scenario: Reroll cost increases
#     Given I have rerolled once at this shop
#     Then the next reroll costs more than the first
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
# Mirror the service prices in cyber_heist/src/shop.rs.
const SCRUB_LOGS_PRICE := 60
const SCRUB_LOGS_NOISE_REDUCTION := 20
const REROLL_BASE_PRICE := 25
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


# Walks the first zone of the current run until the map offers a shop, then
# clicks it on the map like a player would. Encounters on the way are completed
# without being played. Returns the shop's node id, or -1 if this run's first
# zone had none.
func _walk_to_first_zone_shop() -> int:
	for step in 10:
		var hub := _screen()
		if hub == null or hub.name != "ContractHub":
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


# Presses a service button the way a player could: only when it is enabled.
# Returns whether the button could be pressed.
func _press_service(button: Button, service: String) -> bool:
	_check(not button.disabled, "the %s button is enabled" % service)
	if button.disabled:
		return false
	button.pressed.emit()
	return true


# Sets the balance and noise directly, then redraws the screen, as if the
# player had walked in with them. The screen only re-reads its state after an
# action, so it needs telling about a change made from outside it.
func _set_credits_and_noise(shop: Node, credits: int, noise: int) -> void:
	_player.add_credits(credits - _player.money())
	_noise_meter.add_noise(noise - _noise_meter.noise)
	shop._refresh()


func _count_in(names: PackedStringArray, card_name: String) -> int:
	var count := 0
	for drawn_name in names:
		if drawn_name == card_name:
			count += 1
	return count


func _run() -> void:
	await process_frame

	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_player = root.get_node_or_null("/root/PlayerStateGlobal")
	_noise_meter = root.get_node_or_null("/root/NoiseMeterGlobal")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_player != null, "PlayerStateGlobal autoload is present")
	_check(_noise_meter != null, "NoiseMeterGlobal autoload is present")
	if _flow == null or _player == null or _noise_meter == null:
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
	var bought: Array[String] = []

	# Stock can include starter cards, so remember how many copies the run
	# already owns; the later draw must show the bought copies on top of those.
	var run_deck: Node = _flow.get_node("RunDeck")
	var copies_before := {}
	for card: Dictionary in stock:
		copies_before[String(card.name)] = int(run_deck.count_of(String(card.id)))

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
	bought.append(String(target.name))
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
		bought.append(next_name)
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

	_play_scrub_logs(shop, shop_node)
	_play_reroll(shop, shop_node, run_deck, copies_before, bought)

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
	_check(hub != null and hub.name == "ContractHub", "Leave returns to the contract map")
	_check(_flow.run_snapshot().phase == "hub", "no encounter is active after leaving")
	_check(_node_status(shop_id) == "completed", "the shop node is marked completed")

	# --- #38: bought cards are drawable in a later encounter ------------------
	# Scrub Logs left noise at 50, where some cards show their weak-side name.
	# The shop lists every card by its strong-side name, so draw at zero noise
	# to find them by the same names.
	_noise_meter.add_noise(-_noise_meter.noise)
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
	var hand: PackedStringArray = draw.hand_names()
	var checked := {}
	for card_name in bought:
		if checked.has(card_name):
			continue
		checked[card_name] = true
		var expected: int = int(copies_before[card_name]) + bought.count(card_name)
		var drawn := _count_in(hand, card_name)
		_check(
			drawn == expected,
			(
				"every copy of the bought card '%s' can be drawn in a later encounter (%d of %d)"
				% [card_name, drawn, expected]
			)
		)

	_finish()


# S3: Scrub Logs lowers noise by 20 for its price, once per visit, and is
# refused with nothing changed when the player cannot afford it.
func _play_scrub_logs(shop: Node, shop_node: Node) -> void:
	var scrub_button: Button = shop.get_node("%ScrubLogsButton")
	var scrub_price: Label = shop.get_node("%ScrubLogsPriceLabel")
	var scrub_details: Label = shop.get_node("%ScrubLogsDetails")

	# One credit short first.
	_set_credits_and_noise(shop, SCRUB_LOGS_PRICE - 1, 70)
	_check(scrub_details.text.contains("SCRUB LOGS"), "the shop offers Scrub Logs")
	_check(scrub_details.text.contains("now 70/100"), "Scrub Logs shows the noise it would scrub")
	_check(scrub_price.text == "%d cr" % SCRUB_LOGS_PRICE, "Scrub Logs shows its price")
	_check(scrub_button.disabled, "Scrub Logs cannot be bought one credit short")
	_check(
		scrub_price.get_theme_color("font_color") == UNAFFORDABLE_COLOUR,
		"an unaffordable Scrub Logs has its price in red"
	)
	# Force the attempt past the disabled button, as a stale click would.
	shop._on_scrub_logs_pressed()
	_check(_noise_meter.noise == 70, "a refused Scrub Logs leaves noise alone")
	_check(_player.money() == SCRUB_LOGS_PRICE - 1, "a refused Scrub Logs leaves credits alone")
	_check(not shop_node.scrub_logs_offer().sold, "a refused Scrub Logs is still for sale")

	# S3 scenario 1: noise 70 with enough credits.
	var credits_before := SCRUB_LOGS_PRICE + 40
	_set_credits_and_noise(shop, credits_before, 70)
	if not _press_service(scrub_button, "Scrub Logs"):
		return
	_check(
		_noise_meter.noise == 70 - SCRUB_LOGS_NOISE_REDUCTION,
		"Scrub Logs drops noise by 20 to 50 (now %d)" % _noise_meter.noise
	)
	_check(
		_player.money() == credits_before - SCRUB_LOGS_PRICE,
		"Scrub Logs takes exactly its price (%d -> %d)" % [credits_before, _player.money()]
	)
	_check(
		shop.get_node("%CreditsLabel").text.contains(str(_player.money())),
		"the credits shown drop with Scrub Logs"
	)
	_check(scrub_details.text.contains("now 50/100"), "Scrub Logs shows the lowered noise")

	# S3 scenario 2: sold out for the rest of the visit.
	_check(scrub_price.text == "SOLD OUT", "Scrub Logs is shown as sold out")
	_check(scrub_button.disabled, "a sold-out Scrub Logs cannot be pressed")
	var credits_after: int = _player.money()
	var again: Dictionary = shop_node.scrub_logs()
	_check(again.get("error", "") == "already_sold", "the shop refuses a second Scrub Logs")
	_check(_noise_meter.noise == 50, "a second Scrub Logs does not lower noise again")
	_check(_player.money() == credits_after, "a second Scrub Logs charges nothing")


# S4: a reroll replaces every unsold card, leaves sold ones sold, takes its
# price and costs more next time. Ends by buying one of the rerolled cards.
func _play_reroll(
	shop: Node,
	shop_node: Node,
	run_deck: Node,
	copies_before: Dictionary,
	bought: Array[String],
) -> void:
	var reroll_button: Button = shop.get_node("%RerollButton")
	var reroll_price: Label = shop.get_node("%RerollPriceLabel")

	# S4 scenario 1: 50 credits and a 25 credit reroll.
	_set_credits_and_noise(shop, 50, _noise_meter.noise)
	_check(reroll_price.text == "%d cr" % REROLL_BASE_PRICE, "the first reroll costs 25")
	var before: Array = shop_node.stock()
	var before_ids := _stock_ids(before)
	var sold_before := 0
	for card: Dictionary in before:
		if card.get("sold", false):
			sold_before += 1
	_check(sold_before > 0, "some cards were sold before the reroll")
	if not _press_service(reroll_button, "reroll"):
		return

	var after: Array = shop_node.stock()
	_check(_player.money() == 25, "a reroll from 50 credits leaves 25 (got %d)" % _player.money())
	_check(after.size() == before.size(), "a reroll keeps every slot")
	var distinct := {}
	for index in after.size():
		var was: Dictionary = before[index]
		var now: Dictionary = after[index]
		distinct[now.id] = true
		if was.get("sold", false):
			_check(
				now.get("sold", false) and now.id == was.id,
				"sold slot %d stays sold with the same card" % index
			)
			continue
		_check(
			not now.get("sold", false) and not before_ids.has(String(now.id)),
			"unsold slot %d now offers a new card (%s -> %s)" % [index, was.id, now.id]
		)
		_check(
			shop._rows[index].get_child(0).text.contains(String(now.name).to_upper()),
			"slot %d shows its new card" % index
		)
		_check(
			int(now.price) == PRICE_BY_RARITY.get(now.rarity, -1),
			"slot %d's new card is priced by its rarity" % index
		)
	_check(distinct.size() == after.size(), "a reroll never stocks the same card twice")

	# S4 scenario 2: the next reroll costs more than the first, and is out of
	# reach on the 25 credits left.
	var next_price := int(shop_node.reroll_offer().price)
	_check(next_price > REROLL_BASE_PRICE, "the next reroll costs more (%d)" % next_price)
	_check(reroll_price.text == "%d cr" % next_price, "the screen shows the higher reroll price")
	_check(reroll_button.disabled, "an unaffordable reroll's button is disabled")
	_check(
		reroll_price.get_theme_color("font_color") == UNAFFORDABLE_COLOUR,
		"an unaffordable reroll has its price in red"
	)
	shop._on_reroll_pressed()
	_check(_stock_ids(shop_node.stock()) == _stock_ids(after), "a refused reroll keeps the stock")
	_check(_player.money() == 25, "a refused reroll leaves the credits alone")

	# With the money for it, the second reroll takes the higher price.
	_set_credits_and_noise(shop, 200, _noise_meter.noise)
	if not _press_service(reroll_button, "second reroll"):
		return
	_check(
		_player.money() == 200 - next_price,
		"the second reroll takes its higher price (%d left)" % _player.money()
	)

	# A rerolled card is for sale like any other.
	var stock: Array = shop_node.stock()
	var slot := _cheapest_affordable_slot(stock)
	_check(slot >= 0, "a rerolled card is affordable with %d credits" % _player.money())
	if slot < 0:
		return
	var card: Dictionary = stock[slot]
	var card_name := String(card.name)
	if not copies_before.has(card_name):
		copies_before[card_name] = int(run_deck.count_of(String(card.id)))
	var deck_before: int = _flow.run_deck_size()
	if _press_buy(shop, slot, card_name):
		bought.append(card_name)
		_check(_flow.run_deck_size() == deck_before + 1, "a rerolled card can be bought")

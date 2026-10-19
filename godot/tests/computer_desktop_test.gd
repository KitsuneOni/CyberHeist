# Real scene/extension coverage for story #25: a computer-style interface
# between contracts, from which the player reaches their tools and the dark
# web.
#
#   Scenario: A new run opens on the desktop
#     Given I am on the main menu
#     When I choose New contract
#     Then I see my computer desktop rather than the contract map
#     And it offers contracts.exe, darkweb.onion and toolkit/
#
#   Scenario: Look through my tools
#     Given I am on the desktop
#     When I open toolkit/
#     Then my deck opens over the desktop, and closing it leaves me there
#
#   Scenario: Buy upgrades on the dark web
#     Given I am on the desktop with a point to spend
#     When I open darkweb.onion and buy an upgrade
#     Then going back shows the upgrade installed on the desktop
#
#   Scenario: Take a contract
#     Given I am on the desktop
#     When I open contracts.exe
#     Then the offer picker opens with a way back to the desktop
#     And once I accept a contract the desktop and dark web are closed to me
#
#   Scenario: Continue a run saved between contracts
#     Given I saved and quit on the desktop
#     When I continue the run
#     Then I am back on the desktop
#
# Run: godot --headless --path godot -s res://tests/computer_desktop_test.gd
extends SceneTree

const SAVE_PATH := "user://computer_desktop_test.json"

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


func _settle() -> void:
	await process_frame
	await process_frame


func _finish() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))
	if _failures.is_empty():
		print("computer desktop test: PASSED")
		quit(0)
	else:
		print("computer desktop test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


# The desktop on screen, or null with a failure recorded.
func _desktop(context: String) -> Node:
	var screen := _screen()
	_check(screen != null and screen.name == "ComputerDesktop", "%s shows the desktop" % context)
	if screen == null or screen.name != "ComputerDesktop":
		return null
	return screen


func _run() -> void:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_check(_flow != null, "FlowCoordinator autoload is present")
	if _flow == null:
		_finish()
		return

	DirAccess.remove_absolute(ProjectSettings.globalize_path(SAVE_PATH))
	_flow.use_save_file(SAVE_PATH)
	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await _settle()

	print("A new run opens on the desktop")
	var menu := _screen()
	if menu == null or menu.name != "MainMenu":
		_check(false, "the game opens on the main menu")
		_finish()
		return
	menu.new_contract_button.pressed.emit()
	await _settle()
	var desktop := _desktop("New contract")
	if desktop == null:
		_finish()
		return
	_check(_flow.is_between_contracts(), "the run is between contracts")
	_check(desktop.contracts_app.has_focus(), "contracts.exe is selected")
	var offers: int = _flow.contract_offers().size()
	_check(
		desktop.contracts_app.text.contains("[%d offers]" % offers),
		"contracts.exe shows how many offers wait: %s" % desktop.contracts_app.text
	)
	_check(
		desktop.dark_web_app.text.contains("[0 points]"),
		"darkweb.onion shows the points to spend: %s" % desktop.dark_web_app.text
	)
	_check(
		desktop.toolkit_app.text.contains("[%d cards]" % _flow.run_deck_size()),
		"toolkit/ shows the deck size: %s" % desktop.toolkit_app.text
	)
	_check(
		desktop.terminal_log.text.contains("%d contracts waiting" % offers),
		"the terminal says how many contracts wait"
	)
	_check(
		desktop.installed_label.text.begins_with("nothing installed"),
		"the installed window says nothing is installed yet"
	)
	_check(
		not desktop.app_detail_label.text.is_empty(),
		"the selected app says what it does"
	)

	print("Toolkit")
	desktop.toolkit_app.pressed.emit()
	await process_frame
	_check(desktop.is_toolkit_open(), "toolkit/ opens the deck over the desktop")
	_check(
		desktop.deck_viewer.shown_cards.size() > 0,
		"the deck viewer lists the run's cards"
	)
	desktop.deck_viewer.close()
	await process_frame
	_check(not desktop.is_toolkit_open(), "closing the deck leaves the desktop")
	_check(desktop.toolkit_app.has_focus(), "focus goes back to toolkit/")

	print("Dark web")
	desktop.dark_web_app.pressed.emit()
	await _settle()
	var dark_web := _screen()
	_check(
		dark_web != null and dark_web.name == "DarkWebShopScreen",
		"darkweb.onion opens the dark web shop"
	)
	if dark_web != null and dark_web.name == "DarkWebShopScreen":
		dark_web.back_button.pressed.emit()
		await _settle()
	desktop = _desktop("Back from the dark web")
	if desktop == null:
		_finish()
		return

	# A won contract is what earns points; award one directly rather than
	# playing a whole contract, which contract_victory_test already covers.
	_flow.get_node("Progression").award_run_completion(0)
	_check(_flow.return_to_desktop().get("ok", false), "the desktop can be reopened")
	await _settle()
	desktop = _desktop("Reopening")
	if desktop == null:
		_finish()
		return
	_check(
		desktop.dark_web_app.text.contains("[1 point]"),
		"darkweb.onion shows the point earned: %s" % desktop.dark_web_app.text
	)
	_check(
		desktop.terminal_log.text.contains("1 point to spend"),
		"the terminal mentions the point to spend"
	)

	var relic_name := ""
	for relic: Dictionary in _flow.relic_listing():
		if relic.get("affordable", false):
			var bought: Dictionary = _flow.buy_relic(String(relic.get("id", "")))
			if bought.get("ok", false):
				relic_name = String(relic.get("name", ""))
			break
	_check(not relic_name.is_empty(), "an upgrade can be bought with the point")
	_check(_flow.return_to_desktop().get("ok", false), "back to the desktop after buying")
	await _settle()
	desktop = _desktop("After buying an upgrade")
	if desktop == null:
		_finish()
		return
	_check(
		not relic_name.is_empty() and desktop.installed_label.text.contains(relic_name.to_upper()),
		"the installed window lists the upgrade bought: %s" % desktop.installed_label.text
	)

	print("A shop entered from the dark web leaves to the desktop")
	_flow.get_node("Progression").award_run_completion(0)
	_check(
		_flow.enter_shop_from_dark_web().get("ok", false),
		"the shop can be entered from the dark web with a point"
	)
	await _settle()
	var shop := _screen()
	_check(shop != null and shop.name == "ShopScreen", "the shop is on screen")
	if shop != null and shop.name == "ShopScreen":
		shop.leave_button.pressed.emit()
		await _settle()
	desktop = _desktop("Leaving the dark web's shop")
	if desktop == null:
		_finish()
		return

	print("Continue a run saved on the desktop")
	var quit: Dictionary = _flow.save_and_quit_to_menu()
	await _settle()
	_check(quit.get("ok", false) and quit.get("saved_now", false), "the desktop is a safe point")
	_check(_flow.has_saved_run(), "the main menu can continue the run")
	_check(_flow.continue_run().get("ok", false), "Continue run works")
	await _settle()
	desktop = _desktop("Continue on a run saved between contracts")
	if desktop == null:
		_finish()
		return

	print("Contracts")
	desktop.contracts_app.pressed.emit()
	await _settle()
	var hub := _screen()
	_check(hub != null and hub.name == "ContractHub", "contracts.exe opens the contract hub")
	if hub == null or hub.name != "ContractHub":
		_finish()
		return
	_check(hub.is_choosing_contract(), "the hub asks for a contract")
	_check(hub.desktop_button.visible, "the picker offers Back to desktop")
	_check(
		hub.get_node_or_null("Page/VBox/DarkWebShopButton") == null,
		"the dark web is no longer reached from the hub"
	)
	hub.desktop_button.pressed.emit()
	await _settle()
	desktop = _desktop("Back to desktop")
	if desktop == null:
		_finish()
		return

	desktop.contracts_app.pressed.emit()
	await _settle()
	hub = _screen()
	hub.offer_picker.accept_button.pressed.emit()
	await _settle()
	_check(not hub.is_choosing_contract(), "accepting a contract shows the map")
	_check(not hub.desktop_button.visible, "the map does not offer Back to desktop")
	_check(not _flow.is_between_contracts(), "the run is no longer between contracts")
	_check(
		not _flow.return_to_desktop().get("ok", true),
		"the desktop is refused once a contract is accepted"
	)
	_check(
		not _flow.open_dark_web_shop().get("ok", true),
		"the dark web is refused partway through a contract"
	)
	_check(
		not _flow.open_contracts().get("ok", true),
		"contracts.exe is refused partway through a contract"
	)
	await _settle()
	_check(_screen() == hub, "refused requests leave the map on screen")

	_finish()

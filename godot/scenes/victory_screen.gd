# The end screen for a won contract (Trello #89 and #23).
#
# FlowCoordinator shows it once the final zone's boss goes down, after it has
# already paid the contract's reward into PlayerStateGlobal. This screen only
# reports the run and counts the balance up from what it was before the reward
# to the new total, so the payout reads as money arriving rather than a number
# that silently changed.
#
# Navigation goes through FlowCoordinator like every other screen.
extends Control

# Emitted each time the counting balance shows a new whole number of credits.
signal displayed_balance_changed(balance: int)
# Emitted once the balance has finished counting up to its new total.
signal balance_count_finished

# How long the balance takes to count up to its new total, in seconds.
const BALANCE_COUNT_SECONDS := 1.5

@onready var contract_label: Label = $Page/VictoryPanel/Margin/Content/ContractLabel
@onready var summary_label: Label = $Page/VictoryPanel/Margin/Content/SummaryLabel
@onready var reward_label: Label = $Page/VictoryPanel/Margin/Content/RewardLabel
@onready var balance_label: Label = $Page/VictoryPanel/Margin/Content/BalanceLabel
@onready var new_contract_button: Button = %NewContractButton
@onready var status_label: Label = $Page/VictoryPanel/Margin/Content/StatusLabel

# The balance currently on screen. It trails the real balance while counting.
var displayed_balance := 0
# The balance the count is heading for: the player's balance with the reward.
var target_balance := 0
# Guards against a double click starting two new runs.
var _leaving := false


func _ready() -> void:
	_show_contract(FlowCoordinator.accepted_contract_offer())
	_show_summary(FlowCoordinator.run_summary())

	var payout: Dictionary = FlowCoordinator.contract_payout()
	_show_reward(payout)
	_count_balance(payout)

	new_contract_button.pressed.connect(_on_new_contract_pressed)
	new_contract_button.grab_focus()


# Whether the balance is still counting up to its new total.
func is_counting() -> bool:
	return displayed_balance != target_balance


func _show_contract(offer: Dictionary) -> void:
	if offer.get("ok", false):
		contract_label.text = "%s  ·  target: %s" % [offer.get("name", "?"), offer.get("target", "?")]
	else:
		contract_label.text = "Default contract"


func _show_summary(summary: Dictionary) -> void:
	summary_label.text = (
		"Zones cleared: %d of %d  ·  Fights won: %d  ·  Cards added: %d\nCredits earned this run: %d"
		% [
			int(summary.get("zones_cleared", 0)),
			int(summary.get("total_zones", 0)),
			int(summary.get("combats_won", 0)),
			int(summary.get("cards_added", 0)),
			int(summary.get("credits_earned", 0)),
		]
	)


func _show_reward(payout: Dictionary) -> void:
	var credits := int(payout.get("credits", 0))
	if credits > 0:
		reward_label.text = "Contract reward: +%d credits" % credits
	else:
		reward_label.text = "This contract carried no reward."

	# The final boss pays its own encounter credits on top, the same as any
	# boss. Those were already in the balance before the count starts.
	var boss_credits: int = FlowCoordinator.pending_reward_credits()
	if boss_credits > 0:
		reward_label.text += "\nFinal boss payout: +%d credits" % boss_credits


# Counts the balance up from before the reward to the new total. Without a
# payout to show, for example if this screen is opened on its own, it simply
# shows the current balance.
func _count_balance(payout: Dictionary) -> void:
	var current_balance: int = PlayerStateGlobal.money()
	var start_balance := int(payout.get("balance_before", current_balance))
	target_balance = int(payout.get("balance_after", current_balance))
	_render_balance(start_balance)

	if start_balance == target_balance:
		balance_count_finished.emit.call_deferred()
		return

	var tween := create_tween()
	tween.tween_method(
		_set_displayed_balance, float(start_balance), float(target_balance), BALANCE_COUNT_SECONDS
	)
	tween.finished.connect(_on_balance_count_finished)


# Called by the tween on every frame of the count. Takes a float because the
# tween interpolates one; the screen only ever shows whole credits, so frames
# that land on the same whole number change nothing.
func _set_displayed_balance(balance: float) -> void:
	var whole_balance := roundi(balance)
	if whole_balance == displayed_balance:
		return
	_render_balance(whole_balance)


func _render_balance(balance: int) -> void:
	displayed_balance = balance
	balance_label.text = "Balance: %d credits" % displayed_balance
	displayed_balance_changed.emit(displayed_balance)


func _on_balance_count_finished() -> void:
	# The tween ends exactly on its final value, but set it explicitly so the
	# screen can never stop a credit short through rounding.
	_set_displayed_balance(float(target_balance))
	balance_count_finished.emit()


func _on_new_contract_pressed() -> void:
	if _leaving:
		return
	_leaving = true

	var result: Dictionary = FlowCoordinator.start_new_run()
	if not result.get("ok", false):
		# Let the player try again rather than stranding them on this screen.
		_leaving = false
		status_label.text = "Could not start a new contract: %s" % result.get("error", "unknown error")

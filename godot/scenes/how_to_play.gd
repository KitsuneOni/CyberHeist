# The How to play screen (Trello #113), reached from the main menu: a short
# explanation of energy, noise, detection, intents and the map, so a new
# player understands them before their first run. It stands in for a full
# tutorial, which is out of scope.
#
# The topics are plain data below and laid out in two columns in code, so
# changing the wording never means touching the scene. Back, or Escape,
# returns to the main menu. Navigation goes through FlowCoordinator like every
# other screen.
extends Control

const SELECTED_PREFIX := "> "
const IDLE_PREFIX := "  "
const BACK_LABEL := "Back"

const HEADING_COLOUR := Color(0.2, 1, 0.4, 1)
const BODY_COLOUR := Color(0.65, 0.85, 0.72, 1)

# Each topic's heading and what it says, in reading order: left to right, then
# down.
const TOPICS: Array[Dictionary] = [
	{
		"heading": "ENERGY",
		"body": "Each turn you draw 3 cards and get 3 energy. A card's cost is the energy it takes to play. Energy you don't spend is lost when you end your turn, and it refills at the start of the next one.",
	},
	{
		"heading": "NOISE",
		"body": "Almost every card makes noise when you play it. Noise carries from one fight to the next, so it builds up over a contract. Shield soaks up noise before it lands, and recovery cards like VPN bring it back down. Some cards flip to a weaker side once noise is high.",
	},
	{
		"heading": "DETECTION",
		"body": "The detection meter shows your noise out of 100 and changes colour as it fills. Security goes on alert at 60 and gets louder. At 100 you are detected: you are fined, lose this contract's upgrades, and go back to the map to try that node again.",
	},
	{
		"heading": "INTENTS",
		"body": "During your turn the security system shows its next move: how much noise it will make and anything else it will do, such as raising a firewall or taking energy. It acts when you end your turn. Bring its integrity to 0 to win the fight. A firewall blocks damage, but penetrating damage goes straight through.",
	},
	{
		"heading": "THE MAP",
		"body": "A contract is a route of nodes across one or more zones. Pick a highlighted node to breach it; the branches you don't take are locked. [C] combat, [!] elite, [E] event, [$] shop, [#] boss.",
	},
	{
		"heading": "WINNING",
		"body": "Each zone ends in a boss. Beat it to open the next zone. Beat the last zone's boss to win the contract and its payout, then spend points on the dark web between contracts.",
	},
]

@onready var topics_grid: GridContainer = %Topics
@onready var back_button: Button = %BackButton
@onready var status_label: Label = %StatusLabel


func _ready() -> void:
	for topic: Dictionary in TOPICS:
		topics_grid.add_child(_topic_block(topic))

	back_button.focus_entered.connect(_render_back_label)
	back_button.focus_exited.connect(_render_back_label)
	back_button.mouse_entered.connect(back_button.grab_focus)
	back_button.pressed.connect(_on_back_pressed)
	_render_back_label()
	back_button.grab_focus()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		get_viewport().set_input_as_handled()
		_on_back_pressed()


# The headings shown, in order, for tests and anything else that needs to know
# what the screen covers without reading every label.
func shown_headings() -> Array[String]:
	var headings: Array[String] = []
	for block in topics_grid.get_children():
		headings.append(block.get_node("Heading").text)
	return headings


func _topic_block(topic: Dictionary) -> VBoxContainer:
	var block := VBoxContainer.new()
	block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	block.add_theme_constant_override("separation", 4)

	var heading := Label.new()
	heading.name = "Heading"
	heading.text = topic["heading"]
	heading.add_theme_color_override("font_color", HEADING_COLOUR)
	heading.add_theme_font_size_override("font_size", 18)
	block.add_child(heading)

	var body := Label.new()
	body.name = "Body"
	body.text = topic["body"]
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	# Without a minimum width a wrapping label in a grid collapses to nothing.
	body.custom_minimum_size = Vector2(500, 0)
	body.add_theme_color_override("font_color", BODY_COLOUR)
	body.add_theme_font_size_override("font_size", 15)
	block.add_child(body)
	return block


func _render_back_label() -> void:
	var prefix := SELECTED_PREFIX if back_button.has_focus() else IDLE_PREFIX
	back_button.text = prefix + BACK_LABEL


func _on_back_pressed() -> void:
	var result: Dictionary = FlowCoordinator.close_how_to_play()
	if not result.get("ok", false):
		status_label.text = "Could not leave How to play: %s" % result.get("error", "unknown error")

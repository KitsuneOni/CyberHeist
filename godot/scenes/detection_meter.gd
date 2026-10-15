# The combat detection meter, drawn as a terminal readout: a header line
# naming the current band, and the noise bar as a row of lit segments on a
# dark panel. Danger reads at a glance instead of by doing arithmetic on
# "Noise: x / y".
#
# The meter is split into thirds, per the acceptance tests on Trello card #21:
# green while detection is in the lowest third, orange in the middle third and
# red in the highest. The colour eases between bands rather than snapping, and
# the segments pulse in the red band. The band is also named in the header, so
# the state never relies on colour alone.
#
# This is display only. The combat screen still owns the bar's value and the
# NoiseLabel text; it calls show_level() whenever it refreshes them. The
# ProgressBar keeps its value for the rest of the game to read, but its own
# drawing is switched off and replaced by the segments drawn here.
extends Control

enum Band { CALM, ALERT, LOCKDOWN }

# Where each band starts, as a fraction of the meter, so the bands stay thirds
# whatever the cap is.
const ALERT_FRACTION := 1.0 / 3.0
const LOCKDOWN_FRACTION := 2.0 / 3.0

const BAND_COLOURS := {
	Band.CALM: Color("#33FF66"),
	Band.ALERT: Color("#FF8C1A"),
	Band.LOCKDOWN: Color("#FF3355"),
}
const BAND_NAMES := {
	Band.CALM: "CALM",
	Band.ALERT: "ALERT",
	Band.LOCKDOWN: "LOCKDOWN",
}

const FONT := preload("res://assets/fonts/share-tech-mono/ShareTechMono-Regular.ttf")
const PANEL_COLOUR := Color(0.0392157, 0.0588235, 0.0509804, 0.92)
const TEXT_COLOUR := Color(0.85, 0.95, 0.88)

# Layout inside the meter's own rect: a header line, then the segment row.
const HEADER_HEIGHT := 18.0
const BAR_GAP := 4.0
# Dark panel drawn this far outside the meter on every side.
const PANEL_PADDING := 6.0

const SEGMENTS := 25
const SEGMENT_GAP := 2.0
# Unlit segments show the track faintly in the current band colour.
const UNLIT_ALPHA := 0.12
# Lit segments get a soft halo this many pixels wide.
const GLOW_SIZE := 2.0
const GLOW_ALPHA := 0.25

# How quickly the colour eases to a new band's; higher is faster.
const COLOUR_EASE_SPEED := 8.0
# Lockdown pulse: cycles per second, and how far the segments dim at their lowest.
const PULSE_SPEED := 2.5
const PULSE_DEPTH := 0.35

@onready var _bar: ProgressBar = $NoiseBar
@onready var _value_label: Label = $NoiseLabel

var _band: Band = Band.CALM
var _shown_colour: Color = BAND_COLOURS[Band.CALM]
var _drawn_colour: Color = BAND_COLOURS[Band.CALM]
var _pulse_time := 0.0
var _header_label := Label.new()


func _ready() -> void:
	# The bar keeps its value but draws nothing itself; the segments are
	# drawn onto it through its draw signal instead.
	var empty := StyleBoxEmpty.new()
	_bar.add_theme_stylebox_override("background", empty)
	_bar.add_theme_stylebox_override("fill", empty)
	_bar.anchor_top = 0.0
	_bar.anchor_bottom = 1.0
	_bar.offset_top = HEADER_HEIGHT + BAR_GAP
	_bar.offset_bottom = 0.0
	_bar.draw.connect(_draw_segments)

	_header_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_style_header_line(_header_label, HORIZONTAL_ALIGNMENT_LEFT)
	add_child(_header_label)

	# The value stays in NoiseLabel, which the rest of combat writes to,
	# moved up to the right-hand end of the header line.
	_style_header_line(_value_label, HORIZONTAL_ALIGNMENT_RIGHT)
	_value_label.add_theme_color_override("font_color", TEXT_COLOUR)

	_apply_band(_band)


# Which band a level falls in. A meter with no capacity has nothing to fill,
# so it is treated as calm rather than dividing by zero.
static func band_for(current: int, maximum: int) -> Band:
	if maximum <= 0:
		return Band.CALM
	var fraction := float(current) / float(maximum)
	if fraction >= LOCKDOWN_FRACTION:
		return Band.LOCKDOWN
	if fraction >= ALERT_FRACTION:
		return Band.ALERT
	return Band.CALM


func show_level(current: int, maximum: int) -> void:
	var band := band_for(current, maximum)
	if band != _band:
		_apply_band(band)
	_bar.queue_redraw()


func current_band_name() -> String:
	return BAND_NAMES[_band]


func _process(delta: float) -> void:
	var target: Color = BAND_COLOURS[_band]
	_shown_colour = _shown_colour.lerp(target, minf(1.0, delta * COLOUR_EASE_SPEED))

	var colour := _shown_colour
	if _band == Band.LOCKDOWN:
		_pulse_time += delta
		var dip := (sin(_pulse_time * TAU * PULSE_SPEED) + 1.0) * 0.5 * PULSE_DEPTH
		colour = colour.darkened(dip)

	# Only redraw while the colour is actually moving: easing or pulsing.
	if not colour.is_equal_approx(_drawn_colour):
		_drawn_colour = colour
		_header_label.add_theme_color_override("font_color", _shown_colour)
		_bar.queue_redraw()


func _draw() -> void:
	var panel := Rect2(Vector2.ZERO, size).grow(PANEL_PADDING)
	draw_rect(panel, PANEL_COLOUR)
	draw_rect(panel, Color(_drawn_colour, 0.45), false, 1.0)


func _draw_segments() -> void:
	var width := _bar.size.x
	var height := _bar.size.y
	var cell := (width - SEGMENT_GAP * (SEGMENTS - 1)) / SEGMENTS
	# Any noise at all lights the first segment, so the meter never looks
	# empty while there is something on it.
	var lit := ceili(_bar.ratio * SEGMENTS)

	for index in SEGMENTS:
		var rect := Rect2(index * (cell + SEGMENT_GAP), 0.0, cell, height)
		if index < lit:
			_bar.draw_rect(rect.grow(GLOW_SIZE), Color(_drawn_colour, GLOW_ALPHA))
			_bar.draw_rect(rect, _drawn_colour)
		else:
			_bar.draw_rect(rect, Color(_drawn_colour, UNLIT_ALPHA))

	# The panel border follows the band colour too.
	queue_redraw()


func _apply_band(band: Band) -> void:
	_band = band
	_pulse_time = 0.0
	_header_label.text = "DETECTION // %s" % BAND_NAMES[band]
	_header_label.add_theme_color_override("font_color", _shown_colour)


func _style_header_line(label: Label, alignment: HorizontalAlignment) -> void:
	label.anchor_left = 0.0
	label.anchor_right = 1.0
	label.anchor_top = 0.0
	label.anchor_bottom = 0.0
	label.offset_left = 0.0
	label.offset_right = 0.0
	label.offset_top = 0.0
	label.offset_bottom = HEADER_HEIGHT
	label.horizontal_alignment = alignment
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.add_theme_font_override("font", FONT)
	label.add_theme_font_size_override("font_size", 15)

# Owns everything the player hears (story #111): the looping background music,
# the sound each card type makes when it is played, and the volume settings.
#
# Sound is routed through three buses, set up in res://default_bus_layout.tres:
#
#   Master  - everything, the overall volume
#   Music   - the background loop, sent to Master
#   SFX     - card sounds, sent to Master
#
# Each bus has a volume setting from 0 to 100. A setting of 0 mutes its bus
# outright, so the music (or everything, for Master) is completely silent
# rather than merely quiet. AudioRules, in the Rust crate, holds the rules for
# turning a setting into a bus level and for which card type makes which sound.
#
# Each sound cue has its own effects player, made when the cues are loaded,
# whose stream never changes. Giving an AudioStreamPlayer a new stream stops
# whatever it was playing, so sharing one player would cut a card's sound off
# as soon as the next card was played. Each player also allows a few
# overlapping plays (EFFECT_POLYPHONY), so the same card twice in quick
# succession layers rather than restarting.
#
# Without an audio device (the Dummy driver, which is what --headless uses)
# nothing can be heard, so the players are set up exactly as usual but never
# started. Godot 4.7.1 frees a stopped playback on the audio server's next
# update, which never comes once the engine is quitting, so a playback started
# headless is reported at exit as leaked objects and "resources still in use",
# even when the player is stopped in _exit_tree. That ERROR line would fail the
# headless suites' error checks, since the music starts with every one of
# them. They check the routing instead: which stream sits on which player and
# bus.
#
# The settings are kept in user://settings.cfg and applied as soon as this
# autoload is ready, so they carry over between sessions. The settings screen
# changes them through set_volume() and keeps them with save_settings().
extends Node

const SETTINGS_PATH := "user://settings.cfg"
const AUDIO_SECTION := "audio"

# Each volume setting's key in the settings file, and the bus it controls.
const VOLUME_BUSES := {
	"master": &"Master",
	"music": &"Music",
	"sfx": &"SFX",
}

# Where a fresh install starts. The music sits a little under the effects so a
# card being played is never lost under it.
const DEFAULT_VOLUMES := {
	"master": 80,
	"music": 60,
	"sfx": 80,
}

const MUSIC_STREAM_PATH := "res://assets/audio/music/heist_loop.wav"
# Each sound cue is a file named after it in here, e.g. attack.wav.
const SFX_DIRECTORY := "res://assets/audio/sfx/"
const SFX_BUS := &"SFX"
# How many plays of one cue can overlap before the oldest is cut off.
const EFFECT_POLYPHONY := 4

@onready var music_player: AudioStreamPlayer = $MusicPlayer

# Where settings are read from and written to. Only a test points this
# anywhere else, so it never overwrites the player's real settings.
var settings_path := SETTINGS_PATH
# The cue last asked for by play_card_sound(), for the screens and tests that
# need to know what was played without an audio device to listen to.
var last_sound_cue := ""

var _has_audio_output := AudioServer.get_driver_name() != "Dummy"
var _music_on := false

var _volumes := {}
# One player per sound cue, keyed by cue, each holding that cue's stream.
var _sfx_players: Dictionary[String, AudioStreamPlayer] = {}


func _ready() -> void:
	_load_sfx_streams()
	load_settings()
	play_music()


# The current setting for "master", "music" or "sfx", from 0 to 100.
func volume(setting: String) -> int:
	return int(_volumes.get(setting, 0))


# Changes a volume setting and applies it to its bus straight away. The value
# is clamped into 0..100. This does not save; call save_settings() to keep it.
# Returns false, changing nothing, for a setting that does not exist.
func set_volume(setting: String, percent: int) -> bool:
	if not VOLUME_BUSES.has(setting):
		push_error("Unknown volume setting '%s'." % setting)
		return false

	_volumes[setting] = AudioRules.clamp_volume(percent)
	_apply_volume(setting)
	return true


# Writes the current volumes to the settings file. Other sections already in
# the file, such as display settings added later, are kept as they are.
#
# A settings file that exists but cannot be read (a syntax error from a hand
# edit, say) would otherwise be replaced by one holding only the volumes, so it
# is first copied aside to settings.cfg.broken, where nothing is lost, and a
# fresh file is written. Only a missing file means nothing was saved yet.
func save_settings() -> Error:
	var config := ConfigFile.new()
	if FileAccess.file_exists(settings_path):
		var loaded := config.load(settings_path)
		if loaded != OK:
			var kept := _keep_unreadable_settings()
			if kept != OK:
				var reason := "could not be read (error %d) or copied aside (error %d)" % [
					loaded,
					kept,
				]
				push_error("Settings at %s %s, so they were not overwritten." % [settings_path, reason])
				return kept
			config = ConfigFile.new()

	for setting: String in VOLUME_BUSES:
		config.set_value(AUDIO_SECTION, setting, volume(setting))

	var result := config.save(settings_path)
	if result != OK:
		push_error("Could not save settings to %s (error %d)." % [settings_path, result])
	return result


# Copies an unreadable settings file to the same path with ".broken" added,
# replacing any earlier copy, so save_settings() can write a fresh one.
func _keep_unreadable_settings() -> Error:
	var broken_path := settings_path + ".broken"
	var result := DirAccess.copy_absolute(
		ProjectSettings.globalize_path(settings_path),
		ProjectSettings.globalize_path(broken_path)
	)
	if result == OK:
		push_warning(
			"Settings at %s could not be read; kept a copy at %s." % [settings_path, broken_path]
		)
	return result


# Reads the volumes from the settings file and applies them. Anything missing
# or unreadable falls back to its default, so a first launch or a damaged file
# still starts with sound at sensible levels.
func load_settings() -> void:
	var config := ConfigFile.new()
	var loaded := config.load(settings_path) == OK

	for setting: String in VOLUME_BUSES:
		var stored = DEFAULT_VOLUMES[setting]
		if loaded:
			stored = config.get_value(AUDIO_SECTION, setting, stored)
		if typeof(stored) != TYPE_INT and typeof(stored) != TYPE_FLOAT:
			stored = DEFAULT_VOLUMES[setting]
		set_volume(setting, int(stored))


# The bus a volume setting controls, or an empty name for an unknown one.
func bus_for(setting: String) -> StringName:
	return VOLUME_BUSES.get(setting, &"")


# Whether sound actually reaches a device. False in headless runs.
func has_audio_output() -> bool:
	return _has_audio_output


# Whether the background loop has been started. With an audio device that
# means it is playing; without one it is set up but silent.
func is_music_on() -> bool:
	return _music_on


# Starts the background loop if it is not already going. It loops by itself
# (its import settings loop the whole file), so this only needs calling once.
# If the music file cannot be loaded the music stays off, so is_music_on()
# never reports music that cannot play.
func play_music() -> void:
	if music_player.stream == null:
		music_player.stream = load(MUSIC_STREAM_PATH)
	if music_player.stream == null:
		push_error("Could not load the background music at %s." % MUSIC_STREAM_PATH)
		_music_on = false
		return

	_music_on = true
	if _has_audio_output and not music_player.playing:
		music_player.play()


# Plays the sound for a cue reported by DrawPhase.play_card(), one per card
# type, on that cue's own player, so it never cuts off another card's sound.
# Returns false, playing nothing, for a cue with no sound.
func play_card_sound(cue: String) -> bool:
	var player := sfx_player_for(cue)
	if player == null:
		push_warning("No sound effect for cue '%s'." % cue)
		return false

	last_sound_cue = cue
	if _has_audio_output:
		player.play()
	return true


# The effects player for a cue, or null when it has none.
func sfx_player_for(cue: String) -> AudioStreamPlayer:
	return _sfx_players.get(cue)


# The stream a cue plays, or null when it has none.
func stream_for_cue(cue: String) -> AudioStream:
	var player := sfx_player_for(cue)
	return player.stream if player != null else null


func _load_sfx_streams() -> void:
	for cue: String in AudioRules.sound_cues():
		var path := "%s%s.wav" % [SFX_DIRECTORY, cue]
		var stream: AudioStream = load(path)
		if stream == null:
			push_error("Missing sound effect for cue '%s' at %s." % [cue, path])
			continue

		var player := AudioStreamPlayer.new()
		player.name = "Sfx_%s" % cue
		player.bus = SFX_BUS
		player.max_polyphony = EFFECT_POLYPHONY
		player.stream = stream
		add_child(player)
		_sfx_players[cue] = player


func _apply_volume(setting: String) -> void:
	var bus_index := AudioServer.get_bus_index(VOLUME_BUSES[setting])
	if bus_index < 0:
		push_error("Audio bus '%s' is missing from the bus layout." % VOLUME_BUSES[setting])
		return

	var percent := volume(setting)
	AudioServer.set_bus_volume_db(bus_index, AudioRules.volume_db(percent))
	AudioServer.set_bus_mute(bus_index, AudioRules.is_silent(percent))

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
# Without an audio device (the Dummy driver, which is what --headless uses)
# nothing can be heard, so the players are set up exactly as usual but never
# started. A playback still running when the engine quits is reported as a
# leak at exit, which would otherwise fail the headless tests. They check the
# routing instead: which stream sits on which player and bus.
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

@onready var music_player: AudioStreamPlayer = $MusicPlayer
@onready var sfx_player: AudioStreamPlayer = $SfxPlayer

# Where settings are read from and written to. Only a test points this
# anywhere else, so it never overwrites the player's real settings.
var settings_path := SETTINGS_PATH
# The cue last asked for by play_card_sound(), for the screens and tests that
# need to know what was played without an audio device to listen to.
var last_sound_cue := ""

var _has_audio_output := AudioServer.get_driver_name() != "Dummy"
var _music_on := false

var _volumes := {}
var _sfx_streams := {}


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
func save_settings() -> Error:
	var config := ConfigFile.new()
	# A missing file just means nothing has been saved yet.
	config.load(settings_path)
	for setting: String in VOLUME_BUSES:
		config.set_value(AUDIO_SECTION, setting, volume(setting))

	var result := config.save(settings_path)
	if result != OK:
		push_error("Could not save settings to %s (error %d)." % [settings_path, result])
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
func play_music() -> void:
	if music_player.stream == null:
		music_player.stream = load(MUSIC_STREAM_PATH)
	_music_on = true
	if _has_audio_output and not music_player.playing:
		music_player.play()


# Plays the sound for a cue reported by DrawPhase.play_card(), one per card
# type. Returns false, playing nothing, for a cue with no sound.
func play_card_sound(cue: String) -> bool:
	if not _sfx_streams.has(cue):
		push_warning("No sound effect for cue '%s'." % cue)
		return false

	last_sound_cue = cue
	sfx_player.stream = _sfx_streams[cue]
	if _has_audio_output:
		sfx_player.play()
	return true


# The stream a cue plays, or null when it has none.
func stream_for_cue(cue: String) -> AudioStream:
	return _sfx_streams.get(cue)


func _load_sfx_streams() -> void:
	for cue: String in AudioRules.sound_cues():
		var path := "%s%s.wav" % [SFX_DIRECTORY, cue]
		var stream: AudioStream = load(path)
		if stream == null:
			push_error("Missing sound effect for cue '%s' at %s." % [cue, path])
			continue
		_sfx_streams[cue] = stream


func _apply_volume(setting: String) -> void:
	var bus_index := AudioServer.get_bus_index(VOLUME_BUSES[setting])
	if bus_index < 0:
		push_error("Audio bus '%s' is missing from the bus layout." % VOLUME_BUSES[setting])
		return

	var percent := volume(setting)
	AudioServer.set_bus_volume_db(bus_index, AudioRules.volume_db(percent))
	AudioServer.set_bus_mute(bus_index, AudioRules.is_silent(percent))

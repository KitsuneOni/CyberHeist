# Real scene/extension coverage for story #111: sound effects and background
# music, routed through audio buses that follow the volume settings.
#
#   Scenario: Card play sound
#     Given I am in combat
#     When I play a card
#     Then a sound effect plays for that card type
#
#   Scenario: Music follows volume setting
#     Given music is playing
#     When I set music volume to 0 in settings
#     Then the music is silent
#
# There is no audio device in a headless run, and AudioDirector does not start
# its players without one, so this checks the routing rather than listening:
# which stream each card type puts on the SFX player, which bus each player
# feeds, that the music loops, and that a volume of 0 mutes its bus. It also
# checks the settings persist, using a scratch settings file so a developer's
# real settings are never touched.
#
# Run: godot --headless --path godot -s res://tests/audio_test.gd
extends SceneTree

const TEST_SETTINGS_PATH := "user://audio_test_settings.cfg"

# Every card type the combat screen can show, and the sound cue it should make.
const CUES_BY_CARD_TYPE := {
	"Attack": "attack",
	"Defence": "defence",
	"Skill": "skill",
	"Recon": "recon",
	"Social Engineering": "social_engineering",
}

# Cards added to the starter deck so the hand holds one of every type. The
# starter deck already has Attack, Defence and Skill cards.
const EXTRA_CARDS := ["background_check", "nigerian_king"]

var _flow: Node
var _audio: Node
var _failures: Array[String] = []
var _original_volumes := {}


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


func _bus_index(setting: String) -> int:
	return AudioServer.get_bus_index(_audio.bus_for(setting))


func _bus_is_silent(setting: String) -> bool:
	var index := _bus_index(setting)
	return AudioServer.is_bus_mute(index) and AudioServer.get_bus_volume_db(index) <= -80.0


func _finish() -> void:
	# Leave the autoload as this test found it and remove the scratch file.
	if _audio != null:
		for setting: String in _original_volumes:
			_audio.set_volume(setting, _original_volumes[setting])
		_audio.settings_path = _audio.SETTINGS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TEST_SETTINGS_PATH))

	if _failures.is_empty():
		print("audio test: PASSED")
		quit(0)
	else:
		print("audio test: FAILED (%d)" % _failures.size())
		for failure in _failures:
			print("  - %s" % failure)
		quit(1)


func _run() -> void:
	_flow = root.get_node_or_null("/root/FlowCoordinator")
	_audio = root.get_node_or_null("/root/AudioDirector")
	_check(_flow != null, "FlowCoordinator autoload is present")
	_check(_audio != null, "AudioDirector autoload is present")
	if _flow == null or _audio == null:
		_finish()
		return

	for setting: String in _audio.VOLUME_BUSES:
		_original_volumes[setting] = _audio.volume(setting)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TEST_SETTINGS_PATH))
	_audio.settings_path = TEST_SETTINGS_PATH

	var main: Node = load("res://scenes/main.tscn").instantiate()
	root.add_child(main)
	await process_frame
	await process_frame

	_check_bus_layout()
	_check_music()
	await _check_settings_screen()
	_check_persistence()
	await _check_card_sounds()

	_finish()


func _check_bus_layout() -> void:
	for setting: String in _audio.VOLUME_BUSES:
		_check(_bus_index(setting) >= 0, "the %s bus exists" % _audio.bus_for(setting))

	_check(
		AudioServer.get_bus_send(_bus_index("music")) == &"Master",
		"the Music bus feeds Master"
	)
	_check(
		AudioServer.get_bus_send(_bus_index("sfx")) == &"Master",
		"the SFX bus feeds Master"
	)


func _check_music() -> void:
	var music: AudioStreamPlayer = _audio.music_player
	_check(music.bus == &"Music", "the background music plays on the Music bus")
	_check(
		music.stream != null and music.stream.resource_path == _audio.MUSIC_STREAM_PATH,
		"the background music is the heist loop"
	)
	_check(_audio.is_music_on(), "the background music is on")
	_check(
		music.playing == _audio.has_audio_output(),
		"the music player is only started when there is a device to hear it"
	)

	var wav := music.stream as AudioStreamWAV
	_check(
		wav != null and wav.loop_mode == AudioStreamWAV.LOOP_FORWARD,
		"the background music loops"
	)


func _check_settings_screen() -> void:
	var menu := _screen()
	_check(menu != null and menu.name == "MainMenu", "the game opens on the main menu")
	if menu == null or menu.name != "MainMenu":
		return

	var settings_button: Button = menu.get_node_or_null("%SettingsButton")
	_check(settings_button != null, "the main menu offers Settings")
	if settings_button == null:
		return

	settings_button.pressed.emit()
	await process_frame

	var settings := _screen()
	_check(
		settings != null and settings.name == "SettingsMenu",
		"Settings opens the settings screen"
	)
	if settings == null or settings.name != "SettingsMenu":
		return

	var master_slider: HSlider = settings.get_node("%MasterSlider")
	var music_slider: HSlider = settings.get_node("%MusicSlider")
	var sfx_slider: HSlider = settings.get_node("%SfxSlider")
	_check(
		int(music_slider.value) == _audio.volume("music"),
		"the music slider starts at the current music volume"
	)

	# --- music follows the volume setting ---
	music_slider.value = 0
	_check(_audio.is_music_on(), "the music stays on while it is turned down")
	_check(_bus_is_silent("music"), "music volume 0 mutes the Music bus outright")
	_check(not AudioServer.is_bus_mute(_bus_index("sfx")), "muting the music leaves effects alone")
	_check(
		not AudioServer.is_bus_mute(_bus_index("master")),
		"muting the music leaves Master alone"
	)
	_check(settings.get_node("%MusicValue").text == "muted", "the music row says it is muted")

	music_slider.value = 40
	_check(not AudioServer.is_bus_mute(_bus_index("music")), "raising the music unmutes it")
	_check(
		is_equal_approx(
			AudioServer.get_bus_volume_db(_bus_index("music")), AudioRules.volume_db(40)
		),
		"the Music bus level follows the slider straight away"
	)
	_check(settings.get_node("%MusicValue").text == "40%", "the music row shows its percentage")

	sfx_slider.value = 70
	master_slider.value = 0
	_check(_bus_is_silent("master"), "master volume 0 silences everything")
	master_slider.value = 90
	_check(not AudioServer.is_bus_mute(_bus_index("master")), "raising master unmutes it")

	_check(
		FileAccess.file_exists(TEST_SETTINGS_PATH),
		"moving a slider saves the settings"
	)

	settings.get_node("%BackButton").pressed.emit()
	await process_frame
	var back := _screen()
	_check(back != null and back.name == "MainMenu", "Back returns to the main menu")


func _check_persistence() -> void:
	var saved := ConfigFile.new()
	_check(saved.load(TEST_SETTINGS_PATH) == OK, "the saved settings can be read back")
	_check(int(saved.get_value("audio", "master", -1)) == 90, "master volume is saved")
	_check(int(saved.get_value("audio", "music", -1)) == 40, "music volume is saved")
	_check(int(saved.get_value("audio", "sfx", -1)) == 70, "effects volume is saved")

	# Change the live volumes without saving, then load: what comes back is
	# what was saved, the same as the next launch would see.
	_audio.set_volume("music", 5)
	_audio.load_settings()
	_check(_audio.volume("music") == 40, "loading restores the saved music volume")

	_audio.set_volume("music", 250)
	_check(_audio.volume("music") == 100, "a volume above 100 is clamped to 100")
	_audio.set_volume("music", 40)


func _check_card_sounds() -> void:
	for card_id: String in EXTRA_CARDS:
		_check(_flow.add_card_to_deck(card_id), "%s joins the run's deck" % card_id)

	var selected: Dictionary = preload("res://tests/combat_fixture.gd").enter(_flow)
	_check(selected.get("ok", false), "combat loads through the real flow lifecycle")
	if not selected.get("ok", false):
		return
	await process_frame

	var combat := _screen()
	var draw: Node = combat.draw_phase
	# Fixture only: the whole deck in hand and energy to play all of it, so
	# every card type can be played without depending on the shuffle.
	draw.max_energy = 99
	draw.hand_size = (
		draw.hand_names().size() + draw.draw_pile_count() + draw.discard_pile_count()
	)
	combat.selected_index = -1
	combat._draw_hand()

	var sfx: AudioStreamPlayer = _audio.sfx_player
	var streams_heard := {}
	for card_type: String in CUES_BY_CARD_TYPE:
		var expected_cue: String = CUES_BY_CARD_TYPE[card_type]
		var index := _hand_index_of_type(draw, card_type)
		_check(index >= 0, "the hand holds a %s card" % card_type)
		if index < 0:
			continue

		_audio.last_sound_cue = ""
		combat.card_buttons[index].pressed.emit()
		combat.get_node("VBoxContainer/Buttons/PlayButton").pressed.emit()

		_check(
			_audio.last_sound_cue == expected_cue,
			"playing a %s card plays the %s sound" % [card_type, expected_cue]
		)
		_check(
			sfx.stream != null and sfx.stream == _audio.stream_for_cue(expected_cue),
			"the %s sound is the stream on the effects player" % expected_cue
		)
		_check(sfx.bus == &"SFX", "the %s sound plays on the SFX bus" % expected_cue)
		if sfx.stream != null:
			streams_heard[sfx.stream.resource_path] = true

	_check(
		streams_heard.size() == CUES_BY_CARD_TYPE.size(),
		"each card type plays a different sound"
	)

	# Effects at 0 are silent too: the card still routes its sound, to a bus
	# that is muted.
	_audio.set_volume("sfx", 0)
	_check(_bus_is_silent("sfx"), "effects volume 0 mutes the SFX bus outright")
	_check(_audio.is_music_on(), "the music is still on during combat")


func _hand_index_of_type(draw: Node, card_type: String) -> int:
	for index in draw.hand_names().size():
		var detail: Dictionary = draw.card_detail(index)
		if detail.get("type", "") == card_type:
			return index
	return -1

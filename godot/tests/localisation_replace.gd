# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const EXPECTED_TITLE := "替换议案\n“北方”"
const EXPECTED_DESCRIPTION := "替换正文第一行\n替换正文第二行"
const EXPECTED_SUPPORT := "替换赞成\n立即实施"
const EXPECTED_REJECT := "localisation_replace.option_reject"

var failures := 0
var fixture: String = ProjectSettings.globalize_path("res://../examples/world")
var mod_fixture: String = ProjectSettings.globalize_path("res://../examples/localisation-replace")
var external_state_fixture: String = ProjectSettings.globalize_path("res://../examples/localisation-replace/external_state.json")
var capture_prefix := ""

func _initialize() -> void:
	run.call_deferred()

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func run() -> void:
	capture_prefix = read_capture_prefix(OS.get_cmdline_user_args())
	if not capture_prefix.is_empty() and DisplayServer.get_name() == "headless":
		check(false, "--capture requires a graphical display server")
		capture_prefix = ""
	root.content_scale_size = Vector2i.ZERO
	root.size = Vector2i(1100, 720)

	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = fixture
	ui.mod_directories.append(mod_fixture)
	ui.external_state.text = external_state_fixture
	ui.player_countries_input.text = "AAA"
	ui.game_path.grab_focus()
	var loaded: bool = ui.load_world()
	check(loaded, "load the localisation-replace Mod: %s" % ui.status.text)
	await process_frame
	await process_frame
	if not loaded or ui.overview.is_empty():
		await cleanup(ui)
		finish()
		return

	var packet: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(packet.get("ok", false), "query pending event choices")
	var pending: Array = packet.get("value", {}).get("pending_events", [])
	check(pending.size() == 1, "startup queues one AAA event")
	if pending.size() != 1:
		await cleanup(ui)
		finish()
		return

	var choice: Dictionary = pending[0]
	assert_event_text(choice, "initial query")
	var dialog = ui.event_dialog
	check(ui.event_overlay.visible, "pending event opens the choice dialog")
	check(dialog.title_label.text == EXPECTED_TITLE, "dialog displays replacement title with its newline")
	check(
		dialog.body_label.text.begins_with(EXPECTED_DESCRIPTION + "\n"),
		"dialog displays replacement description with its newline"
	)
	check(dialog.option_buttons.size() == 2, "dialog renders both event options")
	if dialog.option_buttons.size() == 2:
		check(dialog.option_buttons[0].text == EXPECTED_SUPPORT, "first option displays replacement text")
		check(dialog.option_buttons[1].text == EXPECTED_REJECT, "conflicting replacement falls back to the original key")
	if not capture_prefix.is_empty():
		await capture_dialog()

	var snapshot: String = save_snapshot_opaque(ui)
	check(not snapshot.is_empty(), "save the pending event as opaque snapshot data")
	var queried_again: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(queried_again.get("ok", false), "repeat the presentation query")
	check(save_snapshot_opaque(ui) == snapshot, "event text query leaves the snapshot unchanged")

	var restored_packet: Dictionary = JSON.parse_string(ui.session.restore_snapshot(snapshot))
	check(restored_packet.get("ok", false), "restore the pending event snapshot")
	ui.refresh_pending_events()
	await process_frame
	await process_frame
	var restored_query: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	var restored_pending: Array = restored_query.get("value", {}).get("pending_events", [])
	check(ui.event_overlay.visible and restored_pending.size() == 1, "restore reopens the pending event dialog")
	if restored_pending.size() == 1:
		var restored_choice: Dictionary = restored_pending[0]
		assert_event_text(restored_choice, "restored query")
		check(dialog.title_label.text == EXPECTED_TITLE, "restored dialog resolves the replacement title")
		check(
			dialog.body_label.text.begins_with(EXPECTED_DESCRIPTION + "\n"),
			"restored dialog resolves the replacement description"
		)
		if dialog.option_buttons.size() == 2:
			check(dialog.option_buttons[0].text == EXPECTED_SUPPORT, "restored dialog resolves the first option")
			check(dialog.option_buttons[1].text == EXPECTED_REJECT, "restored dialog retains the key fallback")
	check(save_snapshot_opaque(ui) == snapshot, "restored presentation leaves snapshot bytes identical")

	var before_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(before_country.get("ok", false), "query treasury before choosing")
	if before_country.get("ok", false):
		check(
			int(before_country.get("value", {}).get("resources", {}).get("treasury_milli", -1)) == 1500,
			"external state starts AAA at 1500 milli"
		)
	if dialog.option_buttons.size() == 2:
		dialog.option_buttons[1].pressed.emit()
		await process_frame
		await process_frame
	var after_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(after_country.get("ok", false), "query treasury after choosing the second option")
	if after_country.get("ok", false):
		check(
			int(after_country.get("value", {}).get("resources", {}).get("treasury_milli", -1)) == 5500,
			"the second option adds 4000 milli"
		)
	var remaining: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(
		not ui.event_overlay.visible and remaining.get("value", {}).get("pending_events", []).is_empty(),
		"choosing the second option closes and clears the pending event"
	)

	await cleanup(ui)
	finish()

func assert_event_text(choice: Dictionary, phase: String) -> void:
	check(choice.get("event") == "localisation_replace.1", "%s returns the fixture event" % phase)
	check(choice.get("title") == EXPECTED_TITLE, "%s returns replacement title text" % phase)
	check(choice.get("description") == EXPECTED_DESCRIPTION, "%s returns replacement description text" % phase)
	var labels: Dictionary = choice.get("option_labels", {})
	check(labels.get("0") == EXPECTED_SUPPORT, "%s returns replacement first-option text" % phase)
	check(labels.get("1") == EXPECTED_REJECT, "%s returns the conflicted key fallback" % phase)

func save_snapshot_opaque(ui) -> String:
	var packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	if not packet.get("ok", false):
		check(false, "save world snapshot")
		return ""
	return str(packet.value)

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			return args[index + 1]
	return ""

func capture_dialog() -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the replacement-localisation dialog")
	if image.is_empty():
		return
	var path := capture_prefix + "-event.png"
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	check(image.save_png(path) == OK, "save replacement-localisation dialog screenshot")

func cleanup(ui) -> void:
	ui.queue_free()
	await process_frame

func finish() -> void:
	print("Augustus localisation replacement integration: %d failures" % failures)
	quit(1 if failures else 0)

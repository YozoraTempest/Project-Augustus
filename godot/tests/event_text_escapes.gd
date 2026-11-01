# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const EXPECTED_TITLE := "议案：\"北方议事会\""
const EXPECTED_DESCRIPTION := "第一段\n第二段\t缩进；字面序列 \\n 保留"
const EXPECTED_FIRST_OPTION := "Café says \"oui\";\tpath C:\\archive\nfollow-up"

var failures := 0
var fixture: String = ProjectSettings.globalize_path("res://../examples/world")
var mod_fixture: String = ProjectSettings.globalize_path("res://../examples/event-text-escapes")
var external_state_fixture: String = ProjectSettings.globalize_path("res://../examples/event-text-escapes/external_state.json")
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
	check(ui.load_world(), "load the event-text-escapes Mod: %s" % ui.status.text)
	await process_frame
	await process_frame
	if ui.overview.is_empty():
		await cleanup(ui)
		finish()
		return

	var pending_packet: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(pending_packet.get("ok", false), "query pending event choices")
	var pending: Array = pending_packet.get("value", {}).get("pending_events", [])
	check(pending.size() == 1, "startup queues one AAA event")
	if pending.size() != 1:
		await cleanup(ui)
		finish()
		return

	var choice: Dictionary = pending[0]
	assert_event_text(choice, "initial query")
	var dialog = ui.event_dialog
	check(ui.event_overlay.visible, "pending event opens the choice dialog")
	check(dialog.title_label.text == EXPECTED_TITLE, "dialog title displays escaped quotes")
	check(dialog.body_label.text.begins_with(EXPECTED_DESCRIPTION + "\n"), "dialog body displays actual newline, tab, and one-pass literal slash-n")
	var before_tab: Rect2 = dialog.body_label.get_character_bounds(6)
	var after_tab: Rect2 = dialog.body_label.get_character_bounds(8)
	check(after_tab.position.x > before_tab.end.x + 2.0, "decoded body tab creates visible spacing between adjacent words")
	check(dialog.option_buttons.size() == 2, "dialog renders both event options")
	if dialog.option_buttons.size() == 2:
		check(dialog.option_buttons[0].text == EXPECTED_FIRST_OPTION.replace("\t", "    "), "first button displays CP1252 text with newline and tab expanded to four spaces")
		check(dialog.option_buttons[1].text == "event_text_escapes.option_missing", "second button displays its missing-key fallback")
	if not capture_prefix.is_empty():
		await capture_dialog("resolved")

	var pending_snapshot: String = save_snapshot_opaque(ui)
	check(not pending_snapshot.is_empty(), "save pending event as an opaque snapshot")
	var readonly_packet: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(readonly_packet.get("ok", false), "repeat pending-event presentation query")
	check(save_snapshot_opaque(ui) == pending_snapshot, "presentation query does not mutate the snapshot")

	var restore_packet: Dictionary = JSON.parse_string(ui.session.restore_snapshot(pending_snapshot))
	check(restore_packet.get("ok", false), "restore the pending-event snapshot")
	ui.refresh_pending_events()
	await process_frame
	await process_frame
	var restored_packet: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	var restored_pending: Array = restored_packet.get("value", {}).get("pending_events", [])
	check(ui.event_overlay.visible and restored_pending.size() == 1, "restored pending event remains visible in the dialog")
	if restored_pending.size() == 1:
		var restored_choice: Dictionary = restored_pending[0]
		assert_event_text(restored_choice, "restored query")
		check(dialog.title_label.text == EXPECTED_TITLE, "restored dialog title is re-resolved")
		check(dialog.body_label.text.begins_with(EXPECTED_DESCRIPTION + "\n"), "restored dialog body is re-resolved")
		if dialog.option_buttons.size() == 2:
			check(dialog.option_buttons[0].text == EXPECTED_FIRST_OPTION.replace("\t", "    "), "restored dialog keeps CP1252 option text and visible tab spacing")
			check(dialog.option_buttons[1].text == "event_text_escapes.option_missing", "restored dialog keeps missing-key fallback")
	check(save_snapshot_opaque(ui) == pending_snapshot, "restoring presentation text leaves snapshot bytes identical")

	var before_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(before_country.get("ok", false), "query external-state treasury before choosing")
	if before_country.get("ok", false):
		check(int(before_country.get("value", {}).get("resources", {}).get("treasury_milli", -1)) == 1200, "external state starts AAA at 1200 milli")
	if dialog.option_buttons.size() == 2:
		dialog.option_buttons[1].pressed.emit()
		await process_frame
		await process_frame
	var after_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(after_country.get("ok", false), "query treasury after choosing the second option")
	if after_country.get("ok", false):
		check(int(after_country.get("value", {}).get("resources", {}).get("treasury_milli", -1)) == 4200, "second option adds 3000 milli")
	var remaining: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(not ui.event_overlay.visible and remaining.get("value", {}).get("pending_events", []).is_empty(), "choosing the second option closes and clears the event")

	await cleanup(ui)
	finish()

func assert_event_text(choice: Dictionary, phase: String) -> void:
	check(choice.get("event") == "event_text_escapes.1", "%s returns the fixture event" % phase)
	check(choice.get("title") == EXPECTED_TITLE, "%s returns the decoded title" % phase)
	check(choice.get("description") == EXPECTED_DESCRIPTION, "%s returns decoded body escapes exactly once" % phase)
	var labels: Dictionary = choice.get("option_labels", {})
	check(labels.get("0") == EXPECTED_FIRST_OPTION, "%s returns decoded CP1252 option text" % phase)
	check(labels.get("1") == "event_text_escapes.option_missing", "%s returns the missing localization key" % phase)

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

func capture_dialog(suffix: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the resolved event dialog")
	if image.is_empty():
		return
	var path := capture_prefix + "-" + suffix + ".png"
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	check(image.save_png(path) == OK, "save resolved event dialog screenshot")

func cleanup(ui) -> void:
	ui.queue_free()
	await process_frame

func finish() -> void:
	print("Augustus event-text escape integration: %d failures" % failures)
	quit(1 if failures else 0)

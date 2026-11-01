# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")
var dialog_fixture := ProjectSettings.globalize_path("res://../examples/event-dialog")
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
	ui.mod_directories.append(dialog_fixture)
	ui.player_countries_input.text = "AAA"
	ui.game_path.grab_focus()
	var external_state_path := write_external_state()
	check(not external_state_path.is_empty(), "create a self-authored event-dialog treasury")
	if external_state_path.is_empty():
		ui.queue_free()
		finish()
		return
	ui.external_state.text = external_state_path
	check(ui.load_world(), "load the self-authored event-dialog Mod: %s" % ui.status.text)
	await process_frame
	await process_frame
	if ui.overview.is_empty():
		await cleanup(ui, external_state_path)
		finish()
		return

	var packet: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(packet.get("ok", false), "query pending event choices")
	var pending: Array = packet.get("value", {}).get("pending_events", [])
	check(pending.size() == 1, "startup queues one event for AAA")
	if pending.size() != 1:
		await cleanup(ui, external_state_path)
		finish()
		return
	var choice: Dictionary = pending[0]
	check(choice.event == "augustus_dialog.1", "query the event-dialog event ID")
	check(str(choice.title).contains("北方海峡") and str(choice.title).length() >= 60, "localized title preserves long UTF-8 Chinese text")
	var description := str(choice.description)
	check(description.contains("冬季贸易协定"), "localized description preserves UTF-8 Chinese text")
	check(description.split("\u2028").size() == 12, "localized description preserves twelve UTF-8 paragraphs")

	var dialog = ui.event_dialog
	check(ui.event_overlay.visible, "pending event opens the dialog")
	check(dialog.body_label.text.begins_with(description + "\n"), "dialog body displays the localized description")
	check(dialog.body_label.text.count("\u2028") == 11, "dialog body retains Unicode line separators")
	check(dialog.title_label.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "dialog title wraps long text")
	check(dialog.body_label.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "dialog body wraps long text")
	check(dialog.scroll.has_focus(), "dialog starts keyboard focus on its scroll area")
	check(dialog.option_buttons.size() == 8, "dialog renders all eight available options")
	if dialog.option_buttons.size() != 8:
		await cleanup(ui, external_state_path)
		finish()
		return
	for index in dialog.option_buttons.size():
		var option_id := str(index)
		check(dialog.option_buttons[index].text == choice.option_labels[option_id], "option %s uses its localized label" % option_id)
		check(dialog.option_buttons[index].autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "option %s wraps long text" % option_id)
	check(dialog.option_buttons.back().text.contains("最终批准"), "last option label is localized")
	check(dialog.option_buttons[0].size.y > 50.0, "long option button grows to fit wrapped text")
	check(dialog.scroll.focus_next == dialog.scroll.get_path_to(dialog.option_buttons[0]), "keyboard focus moves from the scroll area to the first option")
	check(dialog.option_buttons.back().focus_next == dialog.option_buttons.back().get_path_to(dialog.scroll), "keyboard focus loops from the last option to the scroll area")

	await check_viewport(dialog, Vector2i(1100, 720), "wide")
	check(dialog.title_label.size.y > 30.0, "long title occupies multiple lines")
	if not capture_prefix.is_empty():
		await capture_dialog("top")

	var vertical_bar: VScrollBar = dialog.scroll.get_v_scroll_bar()
	check(vertical_bar.max_value > 0.0, "long event content requires vertical scrolling")
	dialog.scroll.scroll_vertical = int(vertical_bar.max_value)
	await process_frame
	await process_frame
	check_last_option_visible(dialog, "wide viewport scroll reaches the last option")
	var last_button: Button = dialog.option_buttons.back()
	var scroll_before_refresh: int = dialog.scroll.scroll_vertical
	ui.refresh_pending_events()
	await process_frame
	await process_frame
	check(dialog.option_buttons.back() == last_button, "refreshing an unchanged event preserves its option controls")
	check(absf(dialog.scroll.scroll_vertical - scroll_before_refresh) <= 1.0, "refreshing an unchanged event preserves scroll position")
	if not capture_prefix.is_empty():
		await capture_dialog("bottom")
	dialog.scroll.scroll_vertical = 0
	await process_frame
	await process_frame
	last_button.grab_focus()
	await process_frame
	await process_frame
	check(last_button.has_focus(), "last option can receive keyboard focus")
	check_last_option_visible(dialog, "following focus scrolls the last option into view")
	var focused_scroll_position: int = dialog.scroll.scroll_vertical
	var core_before_synthetic_present := save_snapshot_opaque(ui)
	var previous_last_button: Button = last_button
	dialog.present(choice, 2)
	await process_frame
	await process_frame
	check(dialog.option_buttons.back() != previous_last_button, "updated pending count refreshes event presentation")
	check(dialog.option_buttons.back().has_focus(), "same-event presentation update restores focus to the same option")
	check(dialog.scroll.scroll_vertical >= focused_scroll_position, "same-event presentation update preserves reading position while revealing the focused option")
	check_last_option_visible(dialog, "same-event presentation update keeps the focused option visible")
	ui.refresh_pending_events()
	await process_frame
	await process_frame
	check(dialog.option_buttons.back().has_focus(), "query refresh restores focus to the same option")
	check(dialog.scroll.scroll_vertical > 0, "query refresh preserves reading position")
	check(save_snapshot_opaque(ui) == core_before_synthetic_present, "synthetic presentation changes do not mutate core state")
	var core_pending_after_synthetic: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(core_pending_after_synthetic.get("value", {}).get("pending_events", []).size() == 1, "synthetic presentation does not add a core pending event")
	last_button = dialog.option_buttons.back()

	await check_viewport(dialog, Vector2i(800, 600), "narrow")
	dialog.scroll.grab_focus()
	dialog.scroll.scroll_vertical = 0
	await process_frame
	await process_frame
	if not capture_prefix.is_empty():
		await capture_dialog("narrow")
	vertical_bar = dialog.scroll.get_v_scroll_bar()
	dialog.scroll.scroll_vertical = int(vertical_bar.max_value)
	await process_frame
	await process_frame
	check_last_option_visible(dialog, "narrow viewport scroll reaches the last option")

	var pending_snapshot := save_snapshot_opaque(ui)
	check(not pending_snapshot.is_empty(), "save pending event snapshot as opaque data")
	check(not ui.advance_world(1), "pending event blocks calendar advancement")
	check(save_snapshot_opaque(ui) == pending_snapshot, "blocked advancement leaves the snapshot unchanged")
	var starting_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(starting_country.get("ok", false), "query starting treasury")
	var starting_treasury := int(starting_country.value.resources.treasury_milli)
	last_button = dialog.option_buttons.back()
	last_button.pressed.emit()
	await process_frame
	await process_frame
	var first_result: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	check(first_result.get("ok", false), "query treasury after selecting the final option")
	if first_result.get("ok", false):
		check(int(first_result.value.resources.treasury_milli) == starting_treasury + 17000, "final option adds 17 treasury units")
	var cleared_events: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(not ui.event_overlay.visible and cleared_events.get("value", {}).get("pending_events", []).is_empty(), "selecting the final option closes and clears the dialog")

	if not pending_snapshot.is_empty():
		var restore_packet: Dictionary = JSON.parse_string(ui.session.restore_snapshot(pending_snapshot))
		check(restore_packet.get("ok", false), "restore pending event from opaque snapshot data")
		ui.refresh_pending_events()
		await process_frame
		await process_frame
		var restored_events: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
		var restored_pending: Array = restored_events.get("value", {}).get("pending_events", [])
		check(ui.event_overlay.visible and restored_pending.size() == 1, "restoring after selection reopens the pending event dialog")
		check(restored_pending.size() == 1 and restored_pending[0].event == "augustus_dialog.1", "snapshot roundtrip preserves the pending event identity")
		check(dialog.option_buttons.size() == 8, "restored dialog exposes all eight options")
		check(save_snapshot_opaque(ui) == pending_snapshot, "restored snapshot remains byte-for-byte identical")
		if dialog.option_buttons.size() == 8:
			vertical_bar = dialog.scroll.get_v_scroll_bar()
			dialog.scroll.scroll_vertical = int(vertical_bar.max_value)
			await process_frame
			await process_frame
			check_last_option_visible(dialog, "restored dialog can scroll to its final option")
			dialog.option_buttons.back().pressed.emit()
			await process_frame
			await process_frame
			var restored_result: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
			check(restored_result.get("ok", false), "query treasury after selecting the restored final option")
			if restored_result.get("ok", false):
				check(int(restored_result.value.resources.treasury_milli) == starting_treasury + 17000, "restored final option still applies its treasury effect")
			var remaining: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
			check(not ui.event_overlay.visible and remaining.get("value", {}).get("pending_events", []).is_empty(), "selecting the restored final option clears the dialog")
			check(ui.game_path.has_focus(), "closing the restored dialog restores focus to the game path")

	await cleanup(ui, external_state_path)
	finish()

func check_viewport(dialog, viewport_size: Vector2i, label: String) -> void:
	root.size = viewport_size
	await process_frame
	await process_frame
	check(root.size == viewport_size, "%s test uses the requested logical viewport" % label)
	var viewport_rect := Rect2(Vector2.ZERO, Vector2(viewport_size))
	var panel_rect: Rect2 = dialog.panel.get_global_rect()
	check(panel_rect.position.x >= -1.0 and panel_rect.position.y >= -1.0 and panel_rect.end.x <= viewport_rect.end.x + 1.0 and panel_rect.end.y <= viewport_rect.end.y + 1.0, "%s dialog panel stays inside the viewport" % label)
	var scroll_rect: Rect2 = dialog.scroll.get_global_rect()
	check(dialog.scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "%s dialog has no horizontal scrolling" % label)
	var title_rect: Rect2 = dialog.title_label.get_global_rect()
	var body_rect: Rect2 = dialog.body_label.get_global_rect()
	check(title_rect.position.x >= scroll_rect.position.x - 1.0 and title_rect.end.x <= scroll_rect.end.x + 1.0, "%s title stays within the scroll width" % label)
	check(body_rect.position.x >= scroll_rect.position.x - 1.0 and body_rect.end.x <= scroll_rect.end.x + 1.0, "%s body stays within the scroll width" % label)
	for option in dialog.option_buttons:
		var option_rect: Rect2 = option.get_global_rect()
		check(option_rect.position.x >= scroll_rect.position.x - 1.0 and option_rect.end.x <= scroll_rect.end.x + 1.0, "%s option text stays within the scroll width" % label)

func check_last_option_visible(dialog, message: String) -> void:
	if dialog.option_buttons.is_empty():
		check(false, message)
		return
	var scroll_rect: Rect2 = dialog.scroll.get_global_rect()
	var option_rect: Rect2 = dialog.option_buttons.back().get_global_rect()
	check(option_rect.position.y >= scroll_rect.position.y - 1.0 and option_rect.end.y <= scroll_rect.end.y + 1.0, message)

func save_snapshot_opaque(ui) -> String:
	var packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	if not packet.get("ok", false):
		check(false, "save world snapshot")
		return ""
	return str(packet.value)

func write_external_state() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-event-dialog-state-%d.json" % OS.get_process_id())
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	file.store_string(JSON.stringify({"country_resources": {"AAA": {"treasury_milli": 12345}}}))
	var error := file.get_error()
	file.close()
	if error != OK:
		DirAccess.remove_absolute(path)
		return ""
	return path

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			return args[index + 1]
	return ""

func capture_dialog(suffix: String) -> void:
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture %s event dialog" % suffix)
	if image.is_empty():
		return
	var path := capture_prefix + "-" + suffix + ".png"
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	check(image.save_png(path) == OK, "save %s event dialog screenshot" % suffix)

func cleanup(ui, external_state_path: String) -> void:
	if not external_state_path.is_empty():
		DirAccess.remove_absolute(external_state_path)
	ui.queue_free()
	await process_frame

func finish() -> void:
	print("Augustus event dialog integration: %d failures" % failures)
	quit(1 if failures else 0)

# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var capture_prefix := ""

func _initialize() -> void:
	run.call_deferred()

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func run() -> void:
	var args := OS.get_cmdline_user_args()
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			capture_prefix = args[index + 1]
	if not capture_prefix.is_empty() and DisplayServer.get_name() == "headless":
		check(false, "--capture requires a graphical display server")
		capture_prefix = ""
	root.size = Vector2i(1100, 720)
	var ui = load("res://main.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	await process_frame
	check(ui.date_label.get_line_count() == 1, "the initial date is readable on a single line")
	check(ui.treasury_label.get_line_count() == 1, "the initial treasury is readable on a single line")
	check(ui.input_choice.size.y <= 64.0, "settings controls retain a compact row height")
	check(root.size == Vector2i(1100, 720), "wide test uses the requested viewport")
	check(root.content_scale_mode == Window.CONTENT_SCALE_MODE_DISABLED, "demo follows the native window size without canvas scaling")
	check(root.get_visible_rect().size == Vector2(root.size), "visible viewport matches the native window dimensions")
	check(not ui.columns.vertical, "1100px viewport keeps the map and province panel side by side")
	check(ui.page_scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "the page never scrolls horizontally")
	await capture_view("desktop")
	await check_reachable(ui, ui.province_buttons[1], "second province is reachable at 1100px")
	var initial_snapshot: String = opaque_snapshot(ui)
	ui.province_buttons[1].pressed.emit()
	check(ui.selected_province == 1, "province selection works in the responsive demo")
	check(opaque_snapshot(ui) == initial_snapshot, "province selection changes presentation only")

	await resize_viewport(ui, Vector2i(800, 600), true, "800px")
	await check_reachable(ui, ui.province_buttons[1], "province map is reachable at 800px")
	await capture_view("800-map")
	await check_reachable(ui, ui.building_choice, "building selector is reachable at 800px")

	await resize_viewport(ui, Vector2i(640, 480), true, "640px")
	check(ui.page_scroll.get_v_scroll_bar().max_value > 0.0, "short viewport scrolls the full demo page")
	await check_reachable(ui, ui.province_buttons[1], "second province remains reachable at 640px")
	await check_reachable(ui, ui.building_choice, "building selector is reachable at 640px")
	check(ui.building_choice.fit_to_longest_item == false and ui.building_choice.clip_text, "building names stay clipped to the selector width")
	check(ui.input_choice.fit_to_longest_item == false and ui.input_choice.clip_text, "input names stay clipped to the selector width")
	check(ui.system_choice.fit_to_longest_item == false and ui.system_choice.clip_text, "system names stay clipped to the selector width")
	check(ui.building_choice.item_count > 0, "construction choices load into the selector")
	await check_reachable(ui, ui.build_button, "capture the construction controls at 640px")
	await capture_view("640-build")
	var chosen_building: int = 1 if ui.building_choice.item_count > 1 else 0
	ui.building_choice.select(chosen_building)
	ui.refresh()
	check(ui.building_choice.selected == chosen_building, "building selection updates the active construction choice")

	var treasury_before: int = int(ui.state.treasury_milli)
	await check_reachable(ui, ui.build_button, "build action is reachable at 640px")
	ui.build_button.pressed.emit()
	check(int(ui.state.treasury_milli) < treasury_before, "building debits the selected province's construction cost")
	check(ui.state.provinces[1].queued_building == chosen_building, "selected building is queued for the selected province")
	var queued_snapshot: String = opaque_snapshot(ui)
	check(not queued_snapshot.is_empty(), "queued construction produces an opaque save snapshot")
	var queued_day: int = int(ui.state.day)
	await check_reachable(ui, ui.advance_one_button, "advance action is reachable at 640px")
	ui.advance_one_button.pressed.emit()
	check(int(ui.state.day) == queued_day + 1, "advance action progresses the simulation by one day")
	var restore_response: String = ui.session.restore_snapshot(queued_snapshot)
	var restore_packet: Dictionary = JSON.parse_string(restore_response)
	check(restore_packet.get("ok", false), "queued construction restores from opaque snapshot data")
	if restore_packet.get("ok", false):
		ui.apply_packet(restore_response)
	check(int(ui.state.day) == queued_day and ui.state.provinces[1].queued_building != null, "restore recovers the queued construction and day")
	check(opaque_snapshot(ui) == queued_snapshot, "restore preserves the queued snapshot byte-for-byte")

	var saved_day: int = int(ui.state.day)
	var saved_treasury: int = int(ui.state.treasury_milli)
	var saved_building: Variant = ui.state.provinces[1].queued_building
	for viewport_size in [Vector2i(1100, 720), Vector2i(800, 600), Vector2i(640, 480)]:
		var expects_vertical: bool = viewport_size.x < 1100
		await resize_viewport(ui, viewport_size, expects_vertical, "%dpx resize" % viewport_size.x)
		check(ui.selected_province == 1, "resize preserves the selected province at %dpx" % viewport_size.x)
		check(int(ui.state.day) == saved_day, "resize preserves the simulation day at %dpx" % viewport_size.x)
		check(int(ui.state.treasury_milli) == saved_treasury, "resize preserves the treasury at %dpx" % viewport_size.x)
		check(ui.state.provinces[1].queued_building == saved_building, "resize preserves the queue at %dpx" % viewport_size.x)
		check(opaque_snapshot(ui) == queued_snapshot, "resize preserves opaque snapshot bytes at %dpx" % viewport_size.x)
		await check_reachable(ui, ui.save_button, "save control is reachable at %dpx" % viewport_size.x)
		await check_reachable(ui, ui.restore_button, "restore control is reachable at %dpx" % viewport_size.x)
		await check_reachable(ui, ui.advance_one_button, "advance control is reachable at %dpx" % viewport_size.x)
		await check_reachable(ui, ui.build_button, "build control is reachable at %dpx" % viewport_size.x)

	ui.queue_free()
	await process_frame
	print("Augustus construction demo layout integration: %d failures" % failures)
	quit(1 if failures else 0)

func resize_viewport(ui, viewport_size: Vector2i, expects_vertical: bool, label: String) -> void:
	root.size = viewport_size
	await process_frame
	await process_frame
	check(root.size == viewport_size, "%s test uses the requested viewport" % label)
	check(root.get_visible_rect().size == Vector2(viewport_size), "%s visible viewport follows the window dimensions" % label)
	check(ui.columns.vertical == expects_vertical, "%s selects the expected responsive column layout" % label)
	check(ui.page_scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "%s keeps horizontal scrolling disabled" % label)

func check_reachable(ui, control: Control, message: String) -> void:
	await process_frame
	await process_frame
	ui.page_scroll.ensure_control_visible(control)
	await process_frame
	await process_frame
	var viewport_rect: Rect2 = ui.page_scroll.get_global_rect()
	var control_rect: Rect2 = control.get_global_rect()
	check(
		control_rect.position.x >= viewport_rect.position.x - 1.0
		and control_rect.end.x <= viewport_rect.end.x + 1.0
		and control_rect.position.y >= viewport_rect.position.y - 1.0
		and control_rect.end.y <= viewport_rect.end.y + 1.0,
		message
	)

func capture_view(suffix: String) -> void:
	if capture_prefix.is_empty():
		return
	await process_frame
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the construction demo view")
	if image.is_empty():
		return
	var path := capture_prefix + "-" + suffix + ".png"
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	check(image.save_png(path) == OK, "save construction demo screenshot")

func opaque_snapshot(ui) -> String:
	var packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	if not packet.get("ok", false):
		check(false, "save current construction demo snapshot")
		return ""
	return str(packet.value)

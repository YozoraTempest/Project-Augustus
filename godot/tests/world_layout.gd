# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const FIXTURE := "res://../examples/world"
const EVENT_FIXTURE := "res://../examples/event-dialog"
const NAVAL_SAVE := "res://../examples/naval-queues/save.eu4"
const LONG_COUNTRY_NAME := "Alpha Northern Maritime Trade and Treaty Council"
const LONG_PROVINCE_NAME := "Northern Straits Winter Trade and Joint Convoy Assembly Harbour"
const LONG_RULER_NAME := "Iris Northern Maritime Treaty Coordination Commissioner"
const LONG_STATUS := "窗口布局状态提示用于检查窄屏上的自动换行，并确保较长的诊断信息不会挤出页面或覆盖地图和操作按钮。"

var failures: int = 0
var ui: Control
var temp_name: String = ""
var temp_root: String = ""
var game_root: String = ""
var event_state_path: String = ""
var capture_prefix: String = ""

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

	var source_fixture: String = ProjectSettings.globalize_path(FIXTURE)
	var naval_save_path: String = ProjectSettings.globalize_path(NAVAL_SAVE)
	temp_name = "world-layout-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()]
	temp_root = ProjectSettings.globalize_path("user://").path_join(temp_name)
	var deep_directory: String = temp_root.path_join("深层自制开局目录-用于长路径和窄屏布局验证")
	game_root = deep_directory.path_join("game-content")
	check(copy_fixture(source_fixture, game_root), "copy the self-authored world fixture to a long temporary path")
	check(prepare_long_fixture(game_root), "prepare self-authored long country/province names and coastal ports")
	event_state_path = temp_root.path_join("event-state.json")
	check(write_text(event_state_path, JSON.stringify({"country_resources": {"AAA": {"treasury_milli": 12345}}})), "write self-authored event state")
	if failures > 0:
		await finish()
		return

	ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await settle_layout()
	ui.game_path.text = game_root
	ui.date_input.text = "1444.11.11"
	ui.player_countries_input.text = "AAA,BBB"

	for viewport_size in [Vector2i(1100, 720), Vector2i(800, 600), Vector2i(640, 480)]:
		await set_viewport(viewport_size, "initial form")
		check_page_layout(viewport_size, "initial form")
		for control in [ui.game_path, ui.date_input, ui.player_countries_input, ui.baseline_save, ui.eu4_save, ui.external_state, ui.load_button]:
			await ensure_reachable(control, true, "initial form control is reachable at %dx%d" % [viewport_size.x, viewport_size.y])
		check(ui.game_path.text == game_root, "long game path remains intact at %dx%d" % [viewport_size.x, viewport_size.y])
		check_no_horizontal_overflow("initial form at %dx%d" % [viewport_size.x, viewport_size.y])

	await ensure_reachable(ui.load_button, true, "load action is reachable")
	ui.load_button.pressed.emit()
	await settle_layout()
	check(not ui.overview.is_empty(), "load the long-path self-authored fixture: %s" % ui.status.text)
	if ui.overview.is_empty():
		await finish()
		return

	var initial_snapshot: String = save_snapshot_opaque()
	check(not initial_snapshot.is_empty(), "capture the loaded state as an opaque snapshot string")
	var aaa_index: int = country_index(ui, "AAA")
	var bbb_index: int = country_index(ui, "BBB")
	check(aaa_index >= 0, "find AAA in the selected country list")
	check(bbb_index >= 0, "find BBB in the selected country list")
	if aaa_index < 0 or bbb_index < 0:
		await finish()
		return
	var country_item: String = ui.country_choice.get_item_text(aaa_index)
	select_country_by_index(aaa_index)
	await settle_layout()
	check(country_item.contains(LONG_COUNTRY_NAME), "long country name reaches the country selector item text")
	check(ui.country_choice.clip_text and ui.country_choice.text_overrun_behavior == TextServer.OVERRUN_TRIM_ELLIPSIS, "country selector clips long names inside its control")
	check(ui.country_details.text.contains(LONG_COUNTRY_NAME), "long country name reaches the country detail text")
	check(ui.country_details.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "long country details wrap")
	check(ui.politics_details.text.contains(LONG_RULER_NAME), "long self-authored ruler name reaches country details")
	check(ui.politics_details.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "long ruler details wrap")
	if not capture_prefix.is_empty():
		await set_viewport(Vector2i(1100, 720), "desktop capture")
		await ensure_reachable(ui.map_view, false, "desktop map is reachable for capture")
		await capture_view("desktop")

	for viewport_size in [Vector2i(1100, 720), Vector2i(800, 600), Vector2i(640, 480)]:
		await set_viewport(viewport_size, "loaded world")
		check_page_layout(viewport_size, "loaded world")
		check_column_geometry(viewport_size)
		await ensure_reachable(ui.time_controls[1], true, "advance-day control is reachable at %dx%d" % [viewport_size.x, viewport_size.y])
		await ensure_reachable(ui.country_choice, true, "country selector is reachable at %dx%d" % [viewport_size.x, viewport_size.y])
		select_country_by_index(bbb_index)
		check(ui.selected_country_id == "BBB", "country selector switches to BBB at %dx%d" % [viewport_size.x, viewport_size.y])
		select_country_by_index(aaa_index)
		check(ui.selected_country_id == "AAA", "country selector switches back to AAA at %dx%d" % [viewport_size.x, viewport_size.y])
		await ensure_reachable(ui.map_view, false, "map is reachable at %dx%d" % [viewport_size.x, viewport_size.y])
		check(ui.map_view.size.x >= 100.0 and ui.map_view.size.y >= 100.0, "map keeps a usable drawing area at %dx%d" % [viewport_size.x, viewport_size.y])
		var sample: Vector2 = map_sample_point(1)
		check(sample.x >= 0.0 and sample.y >= 0.0, "find a rendered sample pixel for province 1 at %dx%d" % [viewport_size.x, viewport_size.y])
		if sample.x >= 0.0 and sample.y >= 0.0:
			var click := InputEventMouseButton.new()
			click.button_index = MOUSE_BUTTON_LEFT
			click.pressed = true
			click.position = sample
			ui.map_view.gui_input.emit(click)
			await settle_layout()
			check(ui.selected_province_id == 1, "coordinate-based map input selects province 1 at %dx%d" % [viewport_size.x, viewport_size.y])
			check(ui.province_details.text.begins_with(LONG_PROVINCE_NAME + " · 1"), "long province name reaches the details text")
			check(ui.province_details.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "long province details wrap")
			await ensure_reachable(ui.province_details, false, "province details can be reached through nested scrolling at %dx%d" % [viewport_size.x, viewport_size.y])
			var sidebar_rect: Rect2 = ui.details_scroll.get_global_rect()
			var province_rect: Rect2 = ui.province_details.get_global_rect()
			check(sidebar_rect.intersects(province_rect), "province details intersect the visible sidebar at %dx%d" % [viewport_size.x, viewport_size.y])
			var detail_bar: VScrollBar = ui.details_scroll.get_v_scroll_bar()
			check(detail_bar.max_value > 0.0, "long country and province details use vertical scrolling at %dx%d" % [viewport_size.x, viewport_size.y])
			ui.details_scroll.scroll_vertical = int(detail_bar.max_value)
			await settle_layout()
			province_rect = ui.province_details.get_global_rect()
			check(province_rect.end.y <= sidebar_rect.end.y + 1.0 and province_rect.end.y >= sidebar_rect.position.y, "the bottom of long province details is reachable by scrolling")
			ui.details_scroll.scroll_vertical = 0

		var original_status: String = ui.status.text
		ui.status.text = LONG_STATUS
		await ensure_reachable(ui.status, false, "long status text remains within page flow at %dx%d" % [viewport_size.x, viewport_size.y])
		check(ui.status.text == LONG_STATUS and ui.status.autowrap_mode == TextServer.AUTOWRAP_WORD_SMART, "long status text wraps without changing its content")
		ui.status.text = original_status
		await settle_layout()
		check_no_horizontal_overflow("loaded world at %dx%d" % [viewport_size.x, viewport_size.y])
		check(save_snapshot_opaque() == initial_snapshot, "resize, scrolling, map selection and country selection preserve the opaque snapshot")
		if not capture_prefix.is_empty() and viewport_size.x == 800:
			await ensure_reachable(ui.map_view, false, "capture map in the 800px viewport")
			await capture_view("800-map")
		if not capture_prefix.is_empty() and viewport_size.x == 640:
			await ensure_reachable(ui.province_details, false, "capture province details in the 640px viewport")
			await capture_view("640-details")

	await set_viewport(Vector2i(1100, 720), "time advancement")
	await ensure_reachable(ui.time_controls[1], true, "advance-day action is reachable before advancing time")
	var date_before: String = ui.calendar_label.text
	var before_advance_snapshot: String = save_snapshot_opaque()
	ui.time_controls[1].pressed.emit()
	await settle_layout()
	check(ui.calendar_label.text != date_before, "the visible advance-day button advances the calendar")
	check(save_snapshot_opaque() != before_advance_snapshot, "calendar advancement changes the opaque campaign state")

	ui.game_path.grab_focus()
	ui.mod_directories.clear()
	ui.mod_directories.append(ProjectSettings.globalize_path(EVENT_FIXTURE))
	ui.external_state.text = event_state_path
	ui.player_countries_input.text = "AAA"
	await ensure_reachable(ui.load_button, true, "load action remains reachable before the event fixture reload")
	ui.load_button.pressed.emit()
	await settle_layout()
	check(ui.event_overlay.visible and ui.event_dialog.option_buttons.size() == 8, "long self-authored event opens with all eight options")
	if not ui.event_overlay.visible or ui.event_dialog.option_buttons.size() != 8:
		await finish()
		return
	var event_snapshot: String = save_snapshot_opaque()
	await set_viewport(Vector2i(800, 600), "event modal resize source")
	await set_viewport(Vector2i(640, 480), "event modal")
	check_control_in_viewport(ui.event_dialog.panel, "long event panel")
	check(ui.event_dialog.scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "event modal has no horizontal scroll")
	var blocked_advance: bool = ui.advance_world(1)
	check(not blocked_advance, "event modal blocks calendar advancement")
	check(save_snapshot_opaque() == event_snapshot, "event popup and resizing preserve pending event state")
	var event_scroll: ScrollContainer = ui.event_dialog.scroll
	var event_bar: VScrollBar = event_scroll.get_v_scroll_bar()
	check(event_bar.max_value > 0.0, "long event body and options scroll vertically")
	event_scroll.scroll_vertical = int(event_bar.max_value)
	await settle_layout()
	var final_event_option: Button = ui.event_dialog.option_buttons.back()
	check(control_intersects(final_event_option, event_scroll), "the final event option is reachable at 640x480")
	final_event_option.grab_focus()
	await settle_layout()
	check(final_event_option.has_focus() and control_intersects(final_event_option, event_scroll), "focus can reach the final event option without horizontal scrolling")
	if not capture_prefix.is_empty():
		await capture_view("640-event-modal")
	final_event_option.pressed.emit()
	await settle_layout()
	check(not ui.event_overlay.visible, "choosing the last event option closes the modal")
	if not capture_prefix.is_empty():
		await capture_view("640-event-closed")

	ui.mod_directories.clear()
	ui.external_state.text = ""
	ui.eu4_save_picker.file_selected.emit(naval_save_path)
	check(not ui.naval_queues_checkbox.disabled, "selecting the self-authored naval save enables queue import")
	ui.naval_queues_checkbox.button_pressed = true
	await ensure_reachable(ui.load_button, true, "load action is reachable before naval fixture reload")
	ui.load_button.pressed.emit()
	await settle_layout()
	check(not ui.overview.is_empty(), "load the self-authored naval queue save at 640x480")
	if ui.overview.is_empty():
		await finish()
		return
	select_country_by_index(country_index(ui, "AAA"))
	var naval_panel = ui.naval_construction_panel
	naval_panel.toggle_section()
	var naval_packet: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	check(naval_packet.get("ok", false) and naval_packet.value.orders.size() > 0, "query an imported AAA naval order")
	if not naval_packet.get("ok", false) or naval_packet.value.orders.is_empty():
		await finish()
		return
	var naval_order_id: String = str(naval_packet.value.orders[0].get("id", ""))
	check(select_naval_order(naval_panel, naval_order_id), "select an imported order")
	await ensure_reachable(naval_panel.cancel_button, true, "cancellation action is reachable in the narrow page")
	var before_cancel_dialog: String = save_snapshot_opaque()
	ui.set_running(true)
	naval_panel.cancel_button.pressed.emit()
	await settle_layout()
	check(not ui.running and naval_panel.cancel_dialog.visible, "opening cancellation pauses the world and presents confirmation")
	check(naval_panel.cancel_dialog.dialog_text.contains("不会退款"), "narrow cancellation dialog explains that the order is not refunded")
	check_window_in_viewport(naval_panel.cancel_dialog, "naval cancellation confirmation")
	if not capture_prefix.is_empty():
		await capture_view("640-cancel-modal")
	naval_panel.cancel_dialog.get_cancel_button().pressed.emit()
	await settle_layout()
	check(not naval_panel.cancel_dialog.visible, "keeping the naval order closes the confirmation")
	check(not ui.running, "closing confirmation keeps simulation paused")
	check(save_snapshot_opaque() == before_cancel_dialog, "opening and closing cancellation confirmation preserve the opaque state")
	await ensure_reachable(ui.time_controls[1], true, "calendar control can be reached after closing the cancellation popup")
	var before_resume_date: String = ui.calendar_label.text
	ui.time_controls[1].pressed.emit()
	await settle_layout()
	check(ui.calendar_label.text != before_resume_date, "page controls remain usable after both popups close")
	check_no_horizontal_overflow("narrow naval page")
	await finish()

func set_viewport(viewport_size: Vector2i, context: String) -> void:
	root.size = viewport_size
	await settle_layout()
	var expected: Vector2 = Vector2(viewport_size)
	check(root.size == viewport_size, "%s uses requested native window size %dx%d" % [context, viewport_size.x, viewport_size.y])
	check(root.get_visible_rect().size.distance_to(expected) <= 1.0, "%s visible rect follows native window size" % context)
	if is_instance_valid(ui):
		check(ui.size.distance_to(expected) <= 1.0, "%s world UI follows native window size" % context)

func settle_layout() -> void:
	await process_frame
	await process_frame

func check_page_layout(viewport_size: Vector2i, context: String) -> void:
	check(ui.page_scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "%s disables page horizontal scrolling" % context)
	check(ui.page_scroll.follow_focus, "%s page scroll follows keyboard focus" % context)
	check(ui.columns.size.x > 0.0 and ui.columns.size.y > 0.0, "%s main columns have nonzero size" % context)
	if viewport_size.x <= 640:
		check(ui.columns.vertical, "%s stacks map and details at 640px" % context)
	if viewport_size.x >= 1100:
		check(not ui.columns.vertical, "%s uses the side-by-side map and details layout at desktop width" % context)
	check_tree_bounds(ui)
	if viewport_size.x <= 640:
		check(ui.page_scroll.get_v_scroll_bar().max_value > 0.0, "%s page exposes vertical scrolling for lower content" % context)

func check_column_geometry(viewport_size: Vector2i) -> void:
	var map_rect: Rect2 = ui.map_column.get_global_rect()
	var detail_rect: Rect2 = ui.details_scroll.get_global_rect()
	if ui.columns.vertical:
		check(map_rect.end.y <= detail_rect.position.y + 1.0, "stacked map precedes the country details at %dx%d" % [viewport_size.x, viewport_size.y])
	else:
		check(map_rect.end.x <= detail_rect.position.x + 1.0, "side-by-side map precedes the country details at %dx%d" % [viewport_size.x, viewport_size.y])
	check(map_rect.size.x > 0.0 and map_rect.size.y > 0.0, "map column remains visible at %dx%d" % [viewport_size.x, viewport_size.y])
	check(detail_rect.size.x > 0.0 and detail_rect.size.y > 0.0, "country sidebar remains visible at %dx%d" % [viewport_size.x, viewport_size.y])

func ensure_reachable(control: Control, require_focus: bool, message: String) -> void:
	await settle_layout()
	var ancestor: Node = control.get_parent()
	while ancestor != null:
		if ancestor is ScrollContainer:
			var scroll: ScrollContainer = ancestor
			scroll.ensure_control_visible(control)
		ancestor = ancestor.get_parent()
	await settle_layout()
	var visible_rect: Rect2 = clipped_rect(control)
	var viewport: Rect2 = root.get_visible_rect()
	check(visible_rect.has_area() and viewport.encloses(visible_rect), "%s is visible within its clipping ancestors" % message)
	if require_focus:
		control.grab_focus()
		await settle_layout()
		check(control.has_focus(), "%s can receive keyboard focus" % message)
		visible_rect = clipped_rect(control)
		check(visible_rect.has_area() and viewport.encloses(visible_rect), "%s remains visible after focus follows it" % message)

func clipped_rect(control: Control) -> Rect2:
	var rect: Rect2 = control.get_global_rect()
	var ancestor: Node = control.get_parent()
	while ancestor != null:
		if ancestor is Control and ancestor.clip_contents:
			rect = rect.intersection(ancestor.get_global_rect())
		ancestor = ancestor.get_parent()
	return rect

func check_tree_bounds(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = clipped_rect(node)
		if rect.has_area():
			check(root.get_visible_rect().grow(1.0).encloses(rect), "visible control stays within viewport after clipping: %s" % node.name)
	for child in node.get_children():
		check_tree_bounds(child)

func check_no_horizontal_overflow(context: String) -> void:
	check(ui.page_scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "%s has no page horizontal scroll" % context)
	check(not ui.page_scroll.get_h_scroll_bar().visible, "%s has no visible page horizontal scrollbar" % context)
	check(ui.details_scroll.horizontal_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED, "%s has no sidebar horizontal scroll" % context)
	check(not ui.details_scroll.get_h_scroll_bar().visible, "%s has no visible sidebar horizontal scrollbar" % context)
	check_tree_bounds(ui)

func check_control_in_viewport(control: Control, message: String) -> void:
	var rect: Rect2 = control.get_global_rect()
	var viewport: Rect2 = root.get_visible_rect()
	check(viewport.encloses(rect), "%s stays within the native viewport" % message)

func check_window_in_viewport(window: Window, message: String) -> void:
	var window_rect: Rect2 = Rect2(Vector2(window.position), Vector2(window.size))
	var viewport_rect: Rect2 = root.get_visible_rect() if window.is_embedded() else Rect2(Vector2(root.position), Vector2(root.size))
	check(viewport_rect.grow(2.0).encloses(window_rect), "%s stays within the native viewport" % message)

func control_intersects(control: Control, viewport: Control) -> bool:
	return control.get_global_rect().intersects(viewport.get_global_rect())

func country_index(world: Control, tag: String) -> int:
	for index in world.country_choice.item_count:
		if world.country_choice.get_item_metadata(index) == tag:
			return index
	return -1

func select_country_by_index(index: int) -> void:
	check(index >= 0, "country index exists")
	if index < 0:
		return
	ui.country_choice.select(index)
	ui.country_choice.item_selected.emit(index)

func map_sample_point(province_id: int) -> Vector2:
	if ui.province_image == null or ui.map_view.size.x <= 0.0 or ui.map_view.size.y <= 0.0:
		return Vector2(-1.0, -1.0)
	for y in ui.province_image.get_height():
		for x in ui.province_image.get_width():
			var color: Color = ui.province_image.get_pixel(x, y)
			var key: int = ui.color_key([roundi(color.r * 255.0), roundi(color.g * 255.0), roundi(color.b * 255.0)])
			if int(ui.color_ids.get(key, 0)) == province_id:
				var image_size: Vector2 = Vector2(ui.province_image.get_width(), ui.province_image.get_height())
				var factor: float = minf(ui.map_view.size.x / image_size.x, ui.map_view.size.y / image_size.y)
				var origin: Vector2 = (ui.map_view.size - image_size * factor) / 2.0
				return origin + (Vector2(x, y) + Vector2(0.5, 0.5)) * factor
	return Vector2(-1.0, -1.0)

func select_naval_order(panel, order_id: String) -> bool:
	var tree_root: TreeItem = panel.tree.get_root()
	if tree_root == null:
		return false
	for index in tree_root.get_child_count():
		var item: TreeItem = tree_root.get_child(index)
		var metadata: Variant = item.get_metadata(0)
		if metadata is Dictionary and str(metadata.get("id", "")) == order_id:
			item.select(0)
			panel.tree.item_selected.emit()
			return true
	return false

func save_snapshot_opaque() -> String:
	var packet: Variant = JSON.parse_string(ui.session.save_snapshot())
	if not packet is Dictionary or not packet.get("ok", false):
		check(false, "save opaque campaign state")
		return ""
	return str(packet.get("value", ""))

func prepare_long_fixture(path: String) -> bool:
	var definitions_path: String = path.path_join("map/definition.csv")
	var definitions: String = read_text(definitions_path)
	var old_province_row: String = "1;240;10;10;North;x"
	if not definitions.contains(old_province_row):
		return false
	definitions = definitions.replace(old_province_row, "1;240;10;10;%s;x" % LONG_PROVINCE_NAME)
	if not write_text(definitions_path, definitions):
		return false
	var tags_path: String = path.path_join("common/country_tags/test.txt")
	var tags: String = read_text(tags_path)
	if not tags.contains("countries/Alpha.txt"):
		return false
	var long_country_path: String = path.path_join("common/countries/%s.txt" % LONG_COUNTRY_NAME)
	if DirAccess.copy_absolute(path.path_join("common/countries/Alpha.txt"), long_country_path) != OK:
		return false
	if not write_text(tags_path, tags.replace("countries/Alpha.txt", "countries/%s.txt" % LONG_COUNTRY_NAME)):
		return false
	var history_path: String = path.path_join("history/countries/AAA - Alpha.txt")
	var history: String = read_text(history_path)
	if not history.contains("name = \"Iris\""):
		return false
	if not write_text(history_path, history.replace("name = \"Iris\"", "name = \"%s\"" % LONG_RULER_NAME)):
		return false
	var map_path: String = path.path_join("map/default.map")
	var map_text: String = read_text(map_path)
	if not map_text.contains("force_coastal = { 1 2 }"):
		map_text += "\nforce_coastal = { 1 2 }\n"
	return write_text(map_path, map_text)

func read_text(path: String) -> String:
	var file: FileAccess = FileAccess.open(path, FileAccess.READ)
	if file == null:
		return ""
	var contents: String = file.get_as_text()
	file.close()
	return contents

func write_text(path: String, contents: String) -> bool:
	var file: FileAccess = FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(contents)
	var error: Error = file.get_error()
	file.close()
	return error == OK

func copy_fixture(source: String, destination: String) -> bool:
	if DirAccess.make_dir_recursive_absolute(destination) != OK:
		return false
	for name in DirAccess.get_files_at(source):
		if DirAccess.copy_absolute(source.path_join(name), destination.path_join(name)) != OK:
			return false
	for name in DirAccess.get_directories_at(source):
		if not copy_fixture(source.path_join(name), destination.path_join(name)):
			return false
	return true

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			return args[index + 1]
	return ""

func capture_view(suffix: String) -> void:
	await settle_layout()
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture %s world layout" % suffix)
	if image.is_empty():
		return
	var path: String = capture_prefix + "-" + suffix + ".png"
	var directory: String = path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	check(image.save_png(path) == OK, "write %s layout screenshot" % suffix)

func remove_fixture(path: String) -> void:
	if path != temp_root and not path.begins_with(temp_root + "/"):
		return
	for name in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(name))
	for name in DirAccess.get_directories_at(path):
		remove_fixture(path.path_join(name))
	DirAccess.remove_absolute(path)

func finish() -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	await process_frame
	if not temp_name.is_empty():
		var expected: String = ProjectSettings.globalize_path("user://").path_join(temp_name)
		if temp_root == expected:
			remove_fixture(temp_root)
	print("Augustus world layout integration: %d failures" % failures)
	quit(1 if failures > 0 else 0)

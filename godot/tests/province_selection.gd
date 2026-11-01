# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const WORLD_FIXTURE := "res://../examples/world"
const MISSION_FIXTURE := "res://../examples/mission-highlights"

var failures: int = 0
var ui
var snapshot_path: String = ""
var capture_prefix: String = ""
var has_graphics: bool = false

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	run.call_deferred()

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func run() -> void:
	capture_prefix = read_capture_argument(OS.get_cmdline_user_args())
	has_graphics = DisplayServer.get_name() != "headless"
	if not capture_prefix.is_empty() and not has_graphics:
		check(false, "--capture requires a graphical display server")
		capture_prefix = ""
	snapshot_path = "user://province-selection-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()]
	ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await settle_layout()
	ui.game_path.text = ProjectSettings.globalize_path(WORLD_FIXTURE)
	ui.date_input.text = "1444.11.11"
	ui.player_countries_input.text = "AAA,BBB"
	check(ui.load_world(), "load the self-authored world fixture: %s" % ui.status.text)
	await settle_layout()
	if ui.overview.is_empty():
		await finish()
		return
	check(root.size == Vector2i(1100, 720), "start at the desktop viewport size")
	check(int(ui.map_material.get_shader_parameter("selected_province_key")) == -1, "new world starts without a selected shader province")
	var initial_snapshot: String = save_snapshot_opaque()
	var province_one_key: int = province_key(1)
	var province_two_key: int = province_key(2)
	check(province_one_key >= 0 and province_two_key >= 0, "fixture exposes palette keys for provinces 1 and 2")

	check(ui.select_province(1, false), "select province 1 through the world query")
	check_selected(1, province_one_key, "query selection")
	check(save_snapshot_opaque() == initial_snapshot, "province selection does not change the opaque campaign snapshot")
	var selected_before_invalid: int = ui.selected_province_id
	var key_before_invalid: int = int(ui.map_material.get_shader_parameter("selected_province_key"))
	check(not ui.select_province(2147483647, false), "unknown province selection is rejected")
	check(ui.selected_province_id == selected_before_invalid, "failed province query preserves the current selection")
	check(int(ui.map_material.get_shader_parameter("selected_province_key")) == key_before_invalid, "failed province query preserves the shader selection key")
	check(save_snapshot_opaque() == initial_snapshot, "failed province query leaves the snapshot unchanged")

	var point_one: Vector2 = map_sample_point(1, false)
	var point_two: Vector2 = map_sample_point(2, false)
	check(point_one.x >= 0.0 and point_two.x >= 0.0, "find map pixels for both selectable provinces")
	if point_one.x >= 0.0 and point_two.x >= 0.0:
		await emit_map_click(point_two, MOUSE_BUTTON_LEFT, true)
		check_selected(2, province_two_key, "left press on a map province")
		check(save_snapshot_opaque() == initial_snapshot, "map click leaves the opaque campaign snapshot unchanged")
		await emit_map_click(point_one, MOUSE_BUTTON_RIGHT, true)
		check_selected(2, province_two_key, "right mouse press")
		await emit_map_click(point_one, MOUSE_BUTTON_LEFT, false)
		check_selected(2, province_two_key, "left mouse release")

	await set_viewport(Vector2i(640, 480))
	var letterbox: Vector2 = letterbox_point()
	check(letterbox.x >= 0.0, "map has an aspect-ratio letterbox area at 640x480")
	if letterbox.x >= 0.0:
		await emit_map_click(letterbox, MOUSE_BUTTON_LEFT, true)
		check_selected(2, province_two_key, "left press in the map letterbox")
		check(save_snapshot_opaque() == initial_snapshot, "letterbox click leaves the opaque campaign snapshot unchanged")

	await set_viewport(Vector2i(1100, 720))
	var military_panel = ui.military_panel
	military_panel.render_report({
		"armies": [{"id": "fixture-army", "name": "Self-authored army", "province_id": 1, "province_name": "North", "regiments": []}],
		"fleets": [],
	})
	military_panel.toggle_section()
	check(select_first_panel_row(military_panel.tree), "select the self-authored military formation row")
	check(military_panel.location_button.visible and not military_panel.location_button.disabled, "military formation exposes its station action")
	military_panel.location_button.pressed.emit()
	check_selected(1, province_one_key, "military station action")
	check(save_snapshot_opaque() == initial_snapshot, "military station selection leaves the snapshot unchanged")

	var naval_panel = ui.naval_construction_panel
	naval_panel.render_report({
		"orders": [{
			"id": "self-authored-order",
			"country": "AAA",
			"province_id": 2,
			"province_name": "East",
			"unit_type": "light_ship",
			"queue_position": null,
			"completion_date": {"year": 1444, "month": 11, "day": 20},
		}],
	})
	naval_panel.toggle_section()
	check(select_first_panel_row(naval_panel.tree), "select the self-authored naval order row")
	check(naval_panel.location_button.visible and not naval_panel.location_button.disabled, "naval order exposes its port action")
	naval_panel.location_button.pressed.emit()
	check_selected(2, province_two_key, "naval port action")
	check(save_snapshot_opaque() == initial_snapshot, "naval port selection leaves the snapshot unchanged")

	ui.refresh_calendar()
	check_selected(2, province_two_key, "calendar refresh")
	select_country("BBB")
	check_selected(2, province_two_key, "country switch to BBB")
	select_country("AAA")
	check_selected(2, province_two_key, "country switch back to AAA")
	check(save_snapshot_opaque() == initial_snapshot, "calendar refresh and country selection preserve the opaque snapshot")

	var saved_snapshot: String = save_snapshot_opaque()
	check(ui.save_world(snapshot_path), "save an owned temporary snapshot for restore coverage")
	check(ui.advance_world(1), "advance the self-authored world by one day")
	check(save_snapshot_opaque() != saved_snapshot, "advancing time changes the campaign snapshot")
	check_selected(2, province_two_key, "day advancement")
	check(ui.restore_world(snapshot_path), "restore the owned temporary snapshot")
	check(save_snapshot_opaque() == saved_snapshot, "restore returns the opaque campaign snapshot")
	check_selected(2, province_two_key, "snapshot restore")

	ui.mod_directories.clear()
	ui.external_state.text = ""
	check(ui.load_world(), "load a fresh world after the selection workflow")
	await settle_layout()
	check(ui.selected_province_id == 0, "new world resets the province selection id")
	check(int(ui.map_material.get_shader_parameter("selected_province_key")) == -1, "new world clears the shader selection key")

	ui.mod_directories.append(ProjectSettings.globalize_path(MISSION_FIXTURE))
	ui.external_state.text = ProjectSettings.globalize_path(MISSION_FIXTURE.path_join("external_state.json"))
	check(ui.load_world(), "load the world with the self-authored mission-highlight fixture")
	await settle_layout()
	if ui.overview.is_empty():
		await finish()
		return
	select_country("AAA")
	var province_one_palette_index: int = int(ui.province_palette_indices.get(1, -1))
	var province_two_palette_index: int = int(ui.province_palette_indices.get(2, -1))
	check(province_one_palette_index >= 0 and province_two_palette_index >= 0, "mission fixture provinces have political palette entries")
	var province_one_political: Color = ui.province_palette_image.get_pixel(province_one_palette_index, 1)
	var province_one_edge: Vector2 = map_sample_point(1, true)
	var province_one_inner: Vector2 = map_sample_point(1, false)
	var province_two_edge: Vector2 = map_sample_point(2, true)
	var province_two_inner: Vector2 = map_sample_point(2, false)
	check(province_one_edge.x >= 0.0 and province_one_inner.x >= 0.0, "find an edge and interior sample for province 1")
	check(province_two_edge.x >= 0.0 and province_two_inner.x >= 0.0, "find an edge and interior sample for province 2")
	if ui.mission_highlight_buttons.is_empty():
		ui.mission_header.pressed.emit()
	check(ui.mission_highlight_buttons.has("aaa_highlight_owned"), "AAA exposes its self-authored province target")
	if not ui.mission_highlight_buttons.has("aaa_highlight_owned"):
		await finish()
		return

	if has_graphics:
		await RenderingServer.frame_post_draw
	var province_one_before_mission: Color = rendered_map_color(province_one_inner)
	var mission_snapshot: String = save_snapshot_opaque()
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	await settle_layout()
	if has_graphics:
		await RenderingServer.frame_post_draw
	check(ui.active_mission_highlight.get("mission") == "aaa_highlight_owned", "mission target action activates the fixture target")
	check(save_snapshot_opaque() == mission_snapshot, "mission target display leaves the opaque campaign snapshot unchanged")
	check(ui.highlight_mask_image.get_pixel(province_one_palette_index, 0).r > 0.99, "mission mask marks province 1")
	check(ui.highlight_mask_image.get_pixel(province_two_palette_index, 0).r < 0.01, "mission mask leaves province 2 untinted")
	var province_one_highlighted_edge: Color = rendered_map_color(province_one_edge)
	var province_one_highlighted_inner: Color = rendered_map_color(province_one_inner)
	var province_two_unselected_edge: Color = rendered_map_color(province_two_edge)
	var province_two_unselected_inner: Color = rendered_map_color(province_two_inner)
	if has_graphics:
		check(color_distance(province_one_before_mission, province_one_highlighted_inner) > 0.2, "mission target changes the rendered province 1 fill")

	check(ui.select_province(1, false), "select the province highlighted by the mission")
	await settle_layout()
	if has_graphics:
		await RenderingServer.frame_post_draw
	check_selected(1, province_one_key, "province selection with mission target active")
	if has_graphics:
		check(color_distance(province_one_highlighted_edge, rendered_map_color(province_one_edge)) > 0.25, "selection changes the rendered province edge")
		check(color_distance(province_one_highlighted_inner, rendered_map_color(province_one_inner)) < 0.06, "selection leaves the highlighted province interior fill unchanged")
		check(color_distance(province_two_unselected_inner, rendered_map_color(province_two_inner)) < 0.06, "selection leaves the non-selected province unchanged")
		if not capture_prefix.is_empty():
			await capture_view("desktop-selection-mission")
	check(save_snapshot_opaque() == mission_snapshot, "province selection remains outside the opaque campaign snapshot")

	check(ui.select_province(2, false), "switch selection to province 2")
	await settle_layout()
	if has_graphics:
		await RenderingServer.frame_post_draw
	check_selected(2, province_two_key, "switch selection to province 2")
	check(save_snapshot_opaque() == mission_snapshot, "switching selected provinces leaves the opaque campaign snapshot unchanged")
	if has_graphics:
		check(color_distance(province_one_highlighted_edge, rendered_map_color(province_one_edge)) < 0.06, "switching selection removes the old province 1 outline")
		check(color_distance(province_two_unselected_edge, rendered_map_color(province_two_edge)) > 0.25, "switching selection draws an outline around province 2")

	ui.map_highlight_clear_button.pressed.emit()
	await settle_layout()
	if has_graphics:
		await RenderingServer.frame_post_draw
	check_selected(2, province_two_key, "clearing mission target keeps province selection")
	check(not ui.map_material.get_shader_parameter("highlight_enabled"), "clearing mission target disables only mission tint")
	check(save_snapshot_opaque() == mission_snapshot, "clearing the mission overlay leaves the opaque campaign snapshot unchanged")
	if has_graphics:
		check(color_distance(province_two_unselected_edge, rendered_map_color(province_two_edge)) > 0.25, "mission clear preserves the selected province outline")
		check(color_distance(province_one_political, rendered_map_color(province_one_inner)) < 0.08, "mission clear restores province 1 political fill")
		await set_viewport(Vector2i(640, 480))
		ui.page_scroll.ensure_control_visible(ui.map_view)
		await settle_layout()
		await RenderingServer.frame_post_draw
		check(root.get_visible_rect().size.distance_to(Vector2(640, 480)) <= 1.0, "selection remains in the real 640x480 viewport")
		check(ui.selected_province_id == 2 and int(ui.map_material.get_shader_parameter("selected_province_key")) == province_two_key, "narrow resize preserves the selected province")
		check(count_selection_pixels() > 0, "selected province outline renders at 640x480")
		if not capture_prefix.is_empty():
			await capture_view("narrow-selection")

	if not has_graphics:
		print("Province selection rendered-pixel assertions skipped under headless display")
	await finish()

func select_country(tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.select_country(index, false)
			return
	check(false, "find country %s" % tag)

func check_selected(province_id: int, key: int, context: String) -> void:
	check(ui.selected_province_id == province_id, "%s keeps selected province id %d" % [context, province_id])
	check(int(ui.map_material.get_shader_parameter("selected_province_key")) == key, "%s keeps the matching shader palette key" % context)

func province_key(province_id: int) -> int:
	var palette_index: int = int(ui.province_palette_indices.get(province_id, -1))
	if palette_index < 0:
		return -1
	return ui.color_key(ui.overview.palette[palette_index].color)

func map_sample_point(province_id: int, boundary: bool) -> Vector2:
	if ui.province_image == null or ui.map_view.size.x <= 0.0 or ui.map_view.size.y <= 0.0:
		return Vector2(-1.0, -1.0)
	var image_size: Vector2 = Vector2(ui.province_image.get_width(), ui.province_image.get_height())
	var factor: float = minf(ui.map_view.size.x / image_size.x, ui.map_view.size.y / image_size.y)
	var origin: Vector2 = (ui.map_view.size - image_size * factor) / 2.0
	for y in ui.province_image.get_height():
		for x in ui.province_image.get_width():
			if province_at(x, y) != province_id:
				continue
			var point: Vector2 = Vector2(x, y) + Vector2(0.5, 0.5)
			if not boundary:
				return origin + point * factor
			var edge_inset: float = maxf(0.0, factor * 0.5 - 0.75)
			if province_at(x - 1, y) != province_id:
				point.x -= edge_inset / factor
			elif province_at(x + 1, y) != province_id:
				point.x += edge_inset / factor
			elif province_at(x, y - 1) != province_id:
				point.y -= edge_inset / factor
			elif province_at(x, y + 1) != province_id:
				point.y += edge_inset / factor
			else:
				continue
			return origin + point * factor
	return Vector2(-1.0, -1.0)

func province_at(x: int, y: int) -> int:
	if x < 0 or y < 0 or x >= ui.province_image.get_width() or y >= ui.province_image.get_height():
		return 0
	var color: Color = ui.province_image.get_pixel(x, y)
	var key: int = ui.color_key([roundi(color.r * 255.0), roundi(color.g * 255.0), roundi(color.b * 255.0)])
	return int(ui.color_ids.get(key, 0))

func letterbox_point() -> Vector2:
	if ui.province_image == null:
		return Vector2(-1.0, -1.0)
	var image_size: Vector2 = Vector2(ui.province_image.get_width(), ui.province_image.get_height())
	var factor: float = minf(ui.map_view.size.x / image_size.x, ui.map_view.size.y / image_size.y)
	var origin: Vector2 = (ui.map_view.size - image_size * factor) / 2.0
	if origin.x > 2.0:
		return Vector2(origin.x / 2.0, ui.map_view.size.y / 2.0)
	if origin.y > 2.0:
		return Vector2(ui.map_view.size.x / 2.0, origin.y / 2.0)
	return Vector2(-1.0, -1.0)

func emit_map_click(point: Vector2, button_index: int, pressed: bool) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = button_index
	event.pressed = pressed
	event.position = point
	ui.map_view.gui_input.emit(event)
	await settle_layout()

func select_first_panel_row(tree: Tree) -> bool:
	var root_item: TreeItem = tree.get_root()
	if root_item == null or root_item.get_child_count() == 0:
		return false
	root_item.get_first_child().select(0)
	tree.item_selected.emit()
	return tree.get_selected() != null

func save_snapshot_opaque() -> String:
	var packet: Variant = JSON.parse_string(ui.session.save_snapshot())
	if not packet is Dictionary or packet.get("ok") != true:
		check(false, "read the opaque campaign snapshot")
		return ""
	return str(packet.get("value", ""))

func rendered_map_color(local_point: Vector2) -> Color:
	if not has_graphics or local_point.x < 0.0:
		return Color.BLACK
	var image: Image = root.get_texture().get_image()
	var viewport_point: Vector2 = ui.map_view.get_global_position() + local_point
	var x: int = clampi(roundi(viewport_point.x), 0, image.get_width() - 1)
	var y: int = clampi(roundi(viewport_point.y), 0, image.get_height() - 1)
	return image.get_pixel(x, y)

func count_selection_pixels() -> int:
	if not has_graphics:
		return 0
	var image: Image = root.get_texture().get_image()
	var rect: Rect2 = ui.map_view.get_global_rect()
	var start_x: int = clampi(floori(rect.position.x), 0, image.get_width() - 1)
	var start_y: int = clampi(floori(rect.position.y), 0, image.get_height() - 1)
	var end_x: int = clampi(ceili(rect.end.x), start_x + 1, image.get_width())
	var end_y: int = clampi(ceili(rect.end.y), start_y + 1, image.get_height())
	var matches: int = 0
	var selection_color := Color(0.55, 0.95, 1.0)
	for y in range(start_y, end_y):
		for x in range(start_x, end_x):
			if color_distance(image.get_pixel(x, y), selection_color) < 0.18:
				matches += 1
	return matches

func color_distance(first: Color, second: Color) -> float:
	return absf(first.r - second.r) + absf(first.g - second.g) + absf(first.b - second.b)

func set_viewport(viewport_size: Vector2i) -> void:
	root.size = viewport_size
	await settle_layout()
	check(root.size == viewport_size, "native window resizes to %dx%d" % [viewport_size.x, viewport_size.y])
	check(root.get_visible_rect().size.distance_to(Vector2(viewport_size)) <= 1.0, "visible rect follows %dx%d viewport" % [viewport_size.x, viewport_size.y])

func settle_layout() -> void:
	await process_frame
	await process_frame

func read_capture_argument(args: PackedStringArray) -> String:
	var index: int = 0
	while index < args.size():
		if args[index] == "--capture":
			if index + 1 < args.size():
				return args[index + 1]
			check(false, "--capture requires an output prefix")
			return ""
		index += 1
	return ""

func capture_view(suffix: String) -> void:
	await RenderingServer.frame_post_draw
	var path: String = ProjectSettings.globalize_path(capture_prefix + "-" + suffix + ".png")
	var directory: String = path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create screenshot directory")
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture rendered map and selection")
	if not image.is_empty():
		check(image.save_png(path) == OK, "write screenshot %s" % path)

func finish() -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	if not snapshot_path.is_empty():
		DirAccess.remove_absolute(ProjectSettings.globalize_path(snapshot_path))
	await process_frame
	print("Augustus province selection integration: %d failures" % failures)
	quit(1 if failures else 0)

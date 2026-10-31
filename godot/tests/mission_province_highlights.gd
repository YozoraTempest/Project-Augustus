# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")
var mission_fixture := ProjectSettings.globalize_path("res://../examples/mission-highlights")
var capture_path := ""

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	capture_path = read_capture_argument(OS.get_cmdline_user_args())
	var has_graphics := DisplayServer.get_name() != "headless"
	if not capture_path.is_empty() and not has_graphics:
		check(false, "--capture requires a graphical display server")
		capture_path = ""
	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = fixture
	ui.player_countries_input.text = "AAA,BBB"
	var state_path := write_external_state()
	var snapshot_path := "user://mission-province-highlights-%d.json" % OS.get_process_id()
	check(not state_path.is_empty(), "create province ownership and discovery fixture state")
	if state_path.is_empty():
		await cleanup(ui, "", snapshot_path)
		quit(1)
		return
	ui.external_state.text = state_path
	ui.mod_directories.append(mission_fixture)
	var loaded: bool = ui.load_world()
	check(loaded, "load the mission highlights fixture: %s" % ui.status.text)
	if not loaded:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	await process_frame
	await process_frame
	if has_graphics:
		await RenderingServer.frame_post_draw

	for mission_id in ["aaa_highlight_owned", "aaa_highlight_none", "aaa_highlight_unknown", "aaa_highlight_unsupported", "bbb_highlight_owned"]:
		check(ui.mission_highlight_id_set.has(mission_id), "catalog includes %s" % mission_id)
	check(not ui.mission_highlight_id_set.has("aaa_highlight_empty"), "empty province filter is omitted from the catalog")
	check(not ui.mission_highlight_id_set.has("aaa_highlight_undeclared"), "mission without a filter is omitted from the catalog")
	ui.mission_header.pressed.emit()
	for mission_id in ["aaa_highlight_owned", "aaa_highlight_none", "aaa_highlight_unknown", "aaa_highlight_unsupported"]:
		check(ui.mission_highlight_buttons.has(mission_id), "selected mission row exposes %s target button" % mission_id)
	check(not ui.mission_highlight_buttons.has("aaa_highlight_empty"), "empty filter has no target button")
	check(not ui.mission_highlight_buttons.has("aaa_highlight_undeclared"), "undeclared filter has no target button")

	var empty_packet: Dictionary = JSON.parse_string(ui.session.query_mission_province_highlights("AAA", "aaa_highlight_empty"))
	var empty_report: Dictionary = empty_packet.get("value", {})
	check(empty_packet.get("ok") == true and empty_report.get("defined") == false, "empty block remains distinct from an explicit no-match filter")
	var none_packet: Dictionary = JSON.parse_string(ui.session.query_mission_province_highlights("AAA", "aaa_highlight_none"))
	var none_report: Dictionary = none_packet.get("value", {})
	check(none_packet.get("ok") == true and none_report.get("defined") == true and none_report.get("provinces", []).is_empty(), "always=no is a defined, known-empty result")
	check(none_report.get("unknown_provinces", {}).is_empty() and none_report.get("unresolved", []).is_empty(), "known-empty result carries no unknown diagnostics")

	check(ui.select_province(4, false), "select province 4 before showing a target")
	var selected_before_target: int = ui.selected_province_id
	var province_one_point := map_sample_point(ui, 1)
	var province_two_point := map_sample_point(ui, 2)
	check(province_one_point.x >= 0.0, "find a map pixel for province 1")
	check(province_two_point.x >= 0.0, "find a map pixel for province 2")
	var province_one_palette_index := int(ui.province_palette_indices.get(1, -1))
	var province_two_palette_index := int(ui.province_palette_indices.get(2, -1))
	check(province_one_palette_index >= 0 and province_two_palette_index >= 0, "find palette entries for the two political colors")
	var initial_province_one_palette_color: Color = ui.province_palette_image.get_pixel(province_one_palette_index, 1)
	var initial_province_two_palette_color: Color = ui.province_palette_image.get_pixel(province_two_palette_index, 1)
	var base_color := Color.BLACK
	var province_two_base_color := Color.BLACK
	if has_graphics:
		await RenderingServer.frame_post_draw
		base_color = rendered_map_color(ui, province_one_point)
		province_two_base_color = rendered_map_color(ui, province_two_point)
	var snapshot_before: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	await process_frame
	if has_graphics:
		await RenderingServer.frame_post_draw
	var owned_report: Dictionary = ui.active_mission_highlight.get("report", {})
	check(ui.active_mission_highlight.get("country") == "AAA" and ui.active_mission_highlight.get("mission") == "aaa_highlight_owned", "target button activates the selected country's declared mission")
	check(province_ids(owned_report) == [1], "ROOT resolves to AAA and excludes province 2 while BBB owns it")
	check(ui.selected_province_id == selected_before_target, "target button leaves province selection unchanged")
	var snapshot_after: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(snapshot_before.get("ok") == true and snapshot_after.get("ok") == true and snapshot_before.value == snapshot_after.value, "target query leaves the world snapshot unchanged")
	check(province_one_palette_index >= 0 and ui.highlight_mask_image.get_pixel(province_one_palette_index, 0).r > 0.99, "mask marks province 1 for tinting")
	check(province_two_palette_index >= 0 and ui.highlight_mask_image.get_pixel(province_two_palette_index, 0).r < 0.01, "mask leaves non-target province 2 unchanged")
	province_one_point = map_sample_point(ui, 1)
	province_two_point = map_sample_point(ui, 2)
	if has_graphics:
		var tinted_color := rendered_map_color(ui, province_one_point)
		var province_two_tinted_color := rendered_map_color(ui, province_two_point)
		check(color_distance(base_color, tinted_color) > 0.2, "the province tint appears in the rendered map")
		check(color_distance(province_two_base_color, province_two_tinted_color) < 0.05, "non-target province 2 keeps its rendered political color")
	if province_two_point.x >= 0.0:
		var map_click := InputEventMouseButton.new()
		map_click.button_index = MOUSE_BUTTON_LEFT
		map_click.pressed = true
		map_click.position = province_two_point
		ui.map_input(map_click)
		check(ui.selected_province_id == 2, "map picking still selects province 2 under the tint")
	else:
		check(false, "find a map pixel for province 2")

	var unknown_packet: Dictionary = JSON.parse_string(ui.session.query_mission_province_highlights("AAA", "aaa_highlight_unknown"))
	var unknown_report: Dictionary = unknown_packet.get("value", {})
	check(unknown_packet.get("ok") == true and province_ids(unknown_report) == [1], "unknown filter retains its known positive province")
	check(unknown_report.get("unknown_provinces", {}).has("3"), "province 3 stays unknown when its discovery state is absent")
	ui.mission_highlight_buttons["aaa_highlight_unknown"].pressed.emit()
	var shown_unknown: Dictionary = ui.active_mission_highlight.get("report", {})
	check(province_ids(shown_unknown) == [1] and shown_unknown.get("unknown_provinces", {}).has("3"), "UI displays known and unknown results separately")
	check(ui.map_highlight_status.text.contains("1 个省份未知") and ui.map_highlight_status.tooltip_text.contains("3"), "unknown province diagnostics remain visible outside the mission list")

	var unsupported_packet: Dictionary = JSON.parse_string(ui.session.query_mission_province_highlights("AAA", "aaa_highlight_unsupported"))
	var unsupported_report: Dictionary = unsupported_packet.get("value", {})
	check(unsupported_packet.get("ok") == true and (not unsupported_report.get("unknown_provinces", {}).is_empty() or not unsupported_report.get("unresolved", []).is_empty()), "unsupported filters report unknown province results or diagnostics")
	ui.mission_highlight_buttons["aaa_highlight_unsupported"].pressed.emit()
	check(ui.map_highlight_status.text.contains("未知") or ui.map_highlight_status.text.contains("诊断"), "unsupported filter is not shown as known-empty")
	check(not ui.map_highlight_status.tooltip_text.is_empty(), "unsupported filter exposes diagnostic detail")

	ui.mission_highlight_buttons["aaa_highlight_none"].pressed.emit()
	check(ui.map_highlight_status.text.contains("无匹配省份"), "known-empty target is labeled as no matching province")
	check(ui.map_material.get_shader_parameter("highlight_enabled") == false, "known-empty result does not tint provinces")
	ui.show_mission_highlight("missing_mission")
	check(ui.map_highlight_status.text == "目标查询失败" and ui.active_mission_highlight.is_empty(), "query errors clear the prior mask and remain distinct from unknown results")

	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	ui.mission_header.pressed.emit()
	check(not ui.mission_content.visible and ui.map_highlight_clear_button.visible and not ui.map_highlight_clear_button.disabled, "active targets remain visible and clearable with the mission list folded")
	ui.map_highlight_clear_button.pressed.emit()
	check(ui.active_mission_highlight.is_empty() and ui.map_material.get_shader_parameter("highlight_enabled") == false, "clear button removes active target mask")
	ui.mission_header.pressed.emit()

	var no_selected_missions: Array[Dictionary] = []
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	ui.refresh_active_mission_highlight("AAA", no_selected_missions)
	check(ui.active_mission_highlight.is_empty(), "active target clears when its mission is no longer selected")
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	check(ui.save_world(snapshot_path), "save before claiming the highlighted objective")
	ui.mission_claim_buttons["aaa_highlight_owned"].pressed.emit()
	var completed: Dictionary = ui.active_mission_highlight.get("report", {})
	check(province_ids(completed) == [1, 2], "claim refresh re-evaluates active targets after province ownership changes")
	var claimed_province_two_palette_color: Color = ui.province_palette_image.get_pixel(province_two_palette_index, 1)
	check(claimed_province_two_palette_color.is_equal_approx(initial_province_one_palette_color), "claim refresh changes province 2's political palette to AAA")
	ui.clear_mission_highlight()
	if has_graphics:
		await RenderingServer.frame_post_draw
		var claimed_province_one_point := map_sample_point(ui, 1)
		var claimed_province_two_point := map_sample_point(ui, 2)
		var claimed_province_one_color := rendered_map_color(ui, claimed_province_one_point)
		var claimed_province_two_color := rendered_map_color(ui, claimed_province_two_point)
		check(color_distance(claimed_province_one_color, claimed_province_two_color) < 0.05, "clearing the target tint leaves province 2 in AAA's political color")
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	check(ui.mission_highlight_buttons.has("aaa_highlight_owned"), "completed selected mission keeps its explicit target button")
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	check(province_ids(ui.active_mission_highlight.get("report", {})) == [1, 2], "completed mission target remains explicitly queryable")

	check(ui.restore_world(snapshot_path), "restore the snapshot before the ownership change")
	check(province_ids(ui.active_mission_highlight.get("report", {})) == [1], "restore refreshes an active target from restored ownership")
	var restored_province_two_palette_color: Color = ui.province_palette_image.get_pixel(province_two_palette_index, 1)
	check(restored_province_two_palette_color.is_equal_approx(initial_province_two_palette_color), "restore returns province 2's political palette to BBB")
	ui.clear_mission_highlight()
	if has_graphics:
		await RenderingServer.frame_post_draw
		var restored_province_two_point := map_sample_point(ui, 2)
		var restored_province_two_color := rendered_map_color(ui, restored_province_two_point)
		check(color_distance(province_two_base_color, restored_province_two_color) < 0.05, "clearing the target tint after restore shows province 2 in BBB's color")
	ui.mission_highlight_buttons["aaa_highlight_owned"].pressed.emit()
	check(ui.advance_world(1), "advance to the scripted ownership event")
	check(ui.event_overlay.visible, "ownership event opens its choice overlay")
	check(province_ids(ui.active_mission_highlight.get("report", {})) == [1, 2], "day advancement refreshes the active target after the event immediate")
	check(ui.province_palette_image.get_pixel(province_two_palette_index, 1).is_equal_approx(initial_province_one_palette_color), "day advancement refreshes province 2's political palette")
	if not ui.event_option_buttons.is_empty():
		ui.event_option_buttons[0].pressed.emit()
		await process_frame
		check(province_ids(ui.active_mission_highlight.get("report", {})) == [2], "event choice refreshes the active target after province 1 changes owner")
		check(ui.province_palette_image.get_pixel(province_one_palette_index, 1).is_equal_approx(initial_province_two_palette_color), "event choice refreshes province 1's political palette")
	else:
		check(false, "ownership event presents an option")

	if not capture_path.is_empty():
		await capture_map(ui)
	select_country(ui, "BBB")
	check(ui.active_mission_highlight.is_empty() and ui.map_material.get_shader_parameter("highlight_enabled") == false, "switching countries clears the prior country's overlay")
	check(ui.mission_highlight_buttons.has("bbb_highlight_owned"), "BBB's selected target mission is available after switching")
	if ui.mission_highlight_buttons.has("bbb_highlight_owned"):
		ui.mission_highlight_buttons["bbb_highlight_owned"].pressed.emit()
		check(ui.active_mission_highlight.get("country") == "BBB" and province_ids(ui.active_mission_highlight.get("report", {})) == [1], "BBB evaluates ROOT against BBB")
	select_country(ui, "AAA")
	check(ui.active_mission_highlight.is_empty(), "switching back does not retain BBB's target")
	check(ui.load_world(), "load a new world with the same self-authored fixture")
	check(ui.active_mission_highlight.is_empty() and ui.map_material.get_shader_parameter("highlight_enabled") == false, "new world clears the previous active target")
	check(ui.selected_province_id == 0, "new world retains the existing province-selection reset")
	await cleanup(ui, state_path, snapshot_path)
	print("Augustus mission province highlights integration: %d failures" % failures)
	quit(1 if failures else 0)

func province_ids(report: Dictionary) -> Array[int]:
	var values: Array[int] = []
	for value in report.get("provinces", []):
		values.append(int(value))
	return values

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.select_country(index)
			return
	check(false, "missing country %s" % tag)

func map_sample_point(ui: Control, province_id: int) -> Vector2:
	if ui.province_image == null:
		return Vector2(-1, -1)
	for y in ui.province_image.get_height():
		for x in ui.province_image.get_width():
			var color: Color = ui.province_image.get_pixel(x, y)
			var key: int = ui.color_key([roundi(color.r * 255), roundi(color.g * 255), roundi(color.b * 255)])
			if int(ui.color_ids.get(key, 0)) == province_id:
				var image_size := Vector2(ui.province_image.get_width(), ui.province_image.get_height())
				var factor := minf(ui.map_view.size.x / image_size.x, ui.map_view.size.y / image_size.y)
				var origin: Vector2 = (ui.map_view.size - image_size * factor) / 2.0
				return origin + (Vector2(x, y) + Vector2(0.5, 0.5)) * factor
	return Vector2(-1, -1)

func rendered_map_color(ui: Control, local_point: Vector2) -> Color:
	if local_point.x < 0.0:
		return Color.BLACK
	var viewport_point: Vector2 = ui.map_view.get_global_position() + local_point
	var image: Image = root.get_texture().get_image()
	var x := clampi(roundi(viewport_point.x), 0, image.get_width() - 1)
	var y := clampi(roundi(viewport_point.y), 0, image.get_height() - 1)
	return image.get_pixel(x, y)

func color_distance(first: Color, second: Color) -> float:
	return absf(first.r - second.r) + absf(first.g - second.g) + absf(first.b - second.b)

func write_external_state() -> String:
	var path := ProjectSettings.globalize_path("user://mission-province-highlights-state-%d.json" % OS.get_process_id())
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	file.store_string(JSON.stringify({
		"country_resources": {"AAA": {"treasury_milli": 0}, "BBB": {"treasury_milli": 0}},
		"country_mission_completions": {"AAA": [], "BBB": []},
		"province_owners": {"1": "AAA", "2": "BBB", "4": "AAA"},
		"province_discovered_by": {"1": ["AAA"]}
	}))
	var error := file.get_error()
	file.close()
	if error != OK:
		DirAccess.remove_absolute(path)
		return ""
	return path

func read_capture_argument(args: PackedStringArray) -> String:
	var index := 0
	while index < args.size():
		if args[index] == "--capture":
			if index + 1 < args.size():
				return args[index + 1]
			check(false, "--capture requires an output path")
			return ""
		index += 1
	return ""

func capture_map(ui: Control) -> void:
	var offset: int = roundi(ui.mission_header.global_position.y - ui.details_scroll.global_position.y)
	ui.details_scroll.scroll_vertical += offset
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var path := ProjectSettings.globalize_path(capture_path)
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create capture directory")
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the rendered province map")
	if not image.is_empty():
		check(image.save_png(path) == OK, "write province highlight capture")

func cleanup(ui: Control, state_path: String, snapshot_path: String) -> void:
	if not state_path.is_empty():
		DirAccess.remove_absolute(state_path)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(snapshot_path))
	ui.queue_free()
	await process_frame

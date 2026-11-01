# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var temp_root := ""
var temp_name := ""
var ui: Control
var launch_ui: Control

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	run.call_deferred()

func run() -> void:
	var fixture := ProjectSettings.globalize_path("res://../examples/world")
	temp_name = "naval-constructions-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()]
	temp_root = ProjectSettings.globalize_path("user://").path_join(temp_name)
	var game_root := temp_root.path_join("game")
	var save_path := temp_root.path_join("naval-queues.eu4")
	check(copy_fixture(fixture, game_root), "copy the self-authored base content")
	check(force_test_ports(game_root), "make both saved construction provinces coastal in the temporary fixture")
	check(write_naval_save(save_path), "write a self-authored EU4 naval queue save")
	if failures:
		finish()
		return

	var mod_directories: Array[String] = []
	var legacy_session = AugustusWorldSession.new()
	var legacy_loaded: Dictionary = JSON.parse_string(legacy_session.load_world_with_eu4_save(
		game_root,
		save_path,
		mod_directories,
		"",
		""
	))
	check(legacy_loaded.get("ok", false), "load the save through the unchanged default EU4 entry point: %s" % legacy_loaded.get("error", ""))
	if not legacy_loaded.get("ok", false):
		finish()
		return
	var legacy_all: Dictionary = JSON.parse_string(legacy_session.query_naval_constructions(""))
	var legacy_aaa: Dictionary = JSON.parse_string(legacy_session.query_naval_constructions("AAA"))
	var legacy_bbb: Dictionary = JSON.parse_string(legacy_session.query_naval_constructions("BBB"))
	check(legacy_all.ok and legacy_all.value.orders.size() == 1, "the default loader keeps only the ordinary single-order baseline")
	check(legacy_aaa.ok and legacy_aaa.value.orders.is_empty(), "the default loader leaves the multi-order AAA FIFO unimported")
	check(legacy_bbb.ok and legacy_bbb.value.orders.size() == 1, "the default loader still resumes BBB's single saved order")
	if legacy_bbb.ok and not legacy_bbb.value.orders.is_empty():
		check(legacy_bbb.value.orders[0].province_id == 2 and legacy_bbb.value.orders[0].country == "BBB", "the baseline order belongs to BBB at East")
	check(legacy_aaa.ok and has_unresolved_province(legacy_aaa.value.import_report, 1), "the legacy report retains a source diagnostic for AAA's FIFO province")
	var unknown_legacy: Dictionary = JSON.parse_string(legacy_session.query_naval_constructions("ZZZ"))
	check(not unknown_legacy.get("ok", false) and str(unknown_legacy.get("error", "")).contains("unknown country"), "an unknown country filter returns an error")
	var legacy_snapshot: Dictionary = JSON.parse_string(legacy_session.save_snapshot())
	check(legacy_snapshot.get("ok", false), "save a snapshot through the legacy single-order entry point")
	if not legacy_snapshot.get("ok", false):
		finish()
		return

	ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = game_root
	check(ui.naval_queues_checkbox.disabled and not ui.naval_queues_checkbox.button_pressed, "the queue checkbox starts disabled and unchecked without an EU4 save")
	ui.eu4_save_picker.file_selected.emit(save_path)
	check(ui.eu4_save.text == save_path and not ui.naval_queues_checkbox.disabled and not ui.naval_queues_checkbox.button_pressed, "choosing an EU4 save enables the unchecked queue option")
	ui.naval_queues_checkbox.button_pressed = true
	var ui_loaded: bool = ui.load_world()
	check(ui_loaded, "load the save with the opt-in FIFO queue checkbox: %s" % ui.status.text)
	if not ui_loaded:
		finish()
		return
	await process_frame
	select_country(ui, "AAA")
	var snapshot_before_queries: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var aaa: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	var bbb: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("BBB"))
	var all: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
	check(aaa.get("ok", false) and aaa.value.orders.size() == 2, "opt-in import connects both saved AAA FIFO orders")
	check(bbb.get("ok", false) and bbb.value.orders.size() == 1, "country filtering keeps BBB's separate single order; report=%s" % JSON.stringify(bbb))
	check(all.get("ok", false) and all.value.orders.size() == 3, "an empty country filter returns all three current orders; report=%s" % JSON.stringify(all))
	if not aaa.get("ok", false) or not bbb.get("ok", false) or not all.get("ok", false):
		finish()
		return
	check(JSON.parse_string(ui.session.save_snapshot()).value == snapshot_before_queries.value, "valid country and all-order queries leave the snapshot bytes unchanged")
	if aaa.get("ok", false) and aaa.value.orders.size() == 2:
		var first: Dictionary = aaa.value.orders[0]
		var tail: Dictionary = aaa.value.orders[1]
		check(first.id is String and tail.id is String and first.id != tail.id, "Godot receives distinct construction IDs as strings")
		check(first.province_id == 1 and first.province_name == "North" and tail.province_id == 1, "FIFO orders retain the saved port and province name")
		check(first.queue_position == 1 and tail.queue_position == 2, "FIFO positions follow the original order")
	var source_report: Dictionary = all.value.import_report
	check(source_report.imported_count == 3, "the import report counts the full saved input; report=%s" % JSON.stringify(source_report))
	check(source_report.unresolved.size() == 1 and source_report.unresolved[0].province_id == 4, "the global source report retains the unrelated unresolved province record; report=%s" % JSON.stringify(source_report))
	check(aaa.value.import_report == bbb.value.import_report, "country filters keep the same file-global import report")
	var opt_in_snapshot: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(opt_in_snapshot.get("ok", false), "save the opt-in snapshot before cross-baseline restoration")
	if not opt_in_snapshot.get("ok", false):
		finish()
		return
	var cross_restore: Dictionary = JSON.parse_string(ui.session.restore_snapshot(legacy_snapshot.value))
	check(cross_restore.get("ok", false), "restore the legacy default snapshot into an opt-in session with the same save baseline")
	if cross_restore.get("ok", false):
		var legacy_restored: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
		check(legacy_restored.ok and legacy_restored.value.orders.size() == 1 and legacy_restored.value.orders[0].country == "BBB", "the legacy snapshot restores its single-order state without FIFO re-import")
	var opt_in_restore: Dictionary = JSON.parse_string(ui.session.restore_snapshot(opt_in_snapshot.value))
	check(opt_in_restore.get("ok", false), "restore the opt-in snapshot after the cross-version compatibility check")
	var opt_in_resumed: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
	check(opt_in_resumed.ok and opt_in_resumed.value.orders.size() == 3, "the opt-in snapshot restores the original three current orders")
	var before_unknown_snapshot: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var unknown: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("ZZZ"))
	check(not unknown.get("ok", false) and str(unknown.get("error", "")).contains("unknown country"), "opt-in queries reject an unknown country")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_unknown_snapshot.value, "failed country queries leave the loaded world unchanged")
	check(treasury(ui.session, "AAA") == 20000, "loading saved constructions does not charge AAA again")

	var panel = ui.naval_construction_panel
	panel.toggle_section()
	check(panel.tree.get_root().get_child_count() == 2, "the expanded panel lists both current AAA queue orders")
	if panel.tree.get_root().get_child_count() == 2:
		var tail_id: String = str(aaa.value.orders[1].id)
		var tail_item: TreeItem = panel.tree.get_root().get_child(1)
		tail_item.select(0)
		panel.tree.item_selected.emit()
		check(panel.selected_key == tail_id, "the waiting order has a stable selected identifier")
		check(panel.detail_label.text.contains("North") and panel.detail_label.text.contains("当前排期日期"), "the selected row shows its saved port and current schedule")
		panel.location_button.pressed.emit()
		check(ui.selected_province_id == 1, "the order location button opens its construction province")
	check_layout(ui)
	var before_failed_load: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var before_failed_orders: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
	ui.eu4_save.text = temp_root.path_join("missing-save.eu4")
	check(not ui.load_world(), "a failed save read does not replace the active world")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_failed_load.value, "a failed save read preserves the previous snapshot")
	check(JSON.parse_string(ui.session.query_naval_constructions("")) == before_failed_orders, "a failed save read preserves the previous current-order report")
	ui.eu4_save.text = save_path
	ui.mod_directories.append(temp_root.path_join("not-a-vanilla-mod"))
	check(not ui.load_world(), "opt-in import rejects an EU4 mod overlay")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_failed_load.value, "a rejected mod overlay leaves the active world unchanged")
	ui.mod_directories.clear()

	var initial_ids: Array = []
	for order in all.value.orders:
		initial_ids.append(str(order.id))
	var initial_snapshot: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(initial_snapshot.get("ok", false), "save the campaign with its imported FIFO state")
	var snapshot_path := temp_root.path_join("queue-snapshot.json")
	check(ui.save_world(snapshot_path), "write a snapshot before advancing the queue")
	check(ui.advance_world(3), "advance until the first AAA ship completes")
	var after_head: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	var fleet_report: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
	check(after_head.ok and after_head.value.orders.size() == 1 and str(after_head.value.orders[0].id) == str(aaa.value.orders[1].id), "finishing the first ship leaves the original FIFO tail pending")
	if after_head.ok and after_head.value.orders.size() == 1:
		check(after_head.value.orders[0].queue_position == 1, "the waiting order moves to the head of the remaining queue")
		check(after_head.value.orders[0].completion_date.day > 13, "the waiting order's completion date moves back after the first ship")
		check(after_head.value.import_report.imported_count == source_report.imported_count, "the global imported count remains tied to source input after completion")
		check(after_head.value.import_report.unresolved == source_report.unresolved, "the global unresolved report remains tied to source input after completion")
	check(panel.selected_key == str(aaa.value.orders[1].id), "refresh keeps the still-pending order selected")
	check(fleet_report.get("ok", false) and fleet_report.value.fleets.size() == 1, "a completed ship appears in AAA's military roster")
	if fleet_report.get("ok", false) and fleet_report.value.fleets.size() == 1:
		check(fleet_report.value.fleets[0].ships.size() == 1 and fleet_report.value.fleets[0].ships[0].unit_type == "sample_light_ship", "the completed roster entry has the expected ship template")
	check(treasury(ui.session, "AAA") == 20000, "completion of an imported ship does not charge its cost again")
	var after_head_all: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
	check(after_head_all.value.orders.size() == 2, "the current order list shrinks while the source report still covers all input")
	check(after_head_all.value.import_report.imported_count == 3, "the source imported count is not reused as the current pending count")

	check(ui.restore_world(snapshot_path), "restore the pre-advance FIFO snapshot")
	check(JSON.parse_string(ui.session.save_snapshot()).value == initial_snapshot.value, "restoring the snapshot recovers its original serialized state")
	var restored_aaa: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	var restored_all: Dictionary = JSON.parse_string(ui.session.query_naval_constructions(""))
	check(restored_aaa.ok and restored_aaa.value.orders.size() == 2, "restore recovers both saved queue orders without re-importing")
	if restored_all.ok:
		var restored_ids: Array = []
		for order in restored_all.value.orders:
			restored_ids.append(str(order.id))
		check(restored_ids == initial_ids, "save and restore preserve every construction ID and order")
		check(restored_all.value.import_report.imported_count == source_report.imported_count, "restore keeps the original file-global import count")
	check(panel.selected_key == str(aaa.value.orders[1].id), "restore keeps the selected waiting order")
	check(treasury(ui.session, "AAA") == 20000, "restoring imported orders does not deduct their cost again")
	if restored_aaa.ok and restored_aaa.value.orders.size() == 2 and aaa.value.orders.size() == 2:
		check(restored_aaa.value.orders[0].completion_day == aaa.value.orders[0].completion_day and restored_aaa.value.orders[1].completion_day == aaa.value.orders[1].completion_day, "restore preserves the original FIFO schedule")
	check(ui.advance_world(3), "resume the restored queue to the first completion")
	check(ui.advance_world(1), "advance one day while the waiting ship remains queued")
	var one_day_later: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	check(one_day_later.ok and one_day_later.value.orders.size() == 1, "the waiting ship remains pending through its delayed schedule")
	check(ui.advance_world(2), "advance to the second ship's saved completion")
	var completed_aaa: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	check(completed_aaa.ok and completed_aaa.value.orders.is_empty(), "both FIFO orders leave the current list after completion")
	var completed_roster: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
	check(completed_roster.ok and count_ships(completed_roster.value) == 2, "both completed ships are now present in the military roster")
	check(treasury(ui.session, "AAA") == 20000, "the full restored queue completes without another charge")

	select_country(ui, "BBB")
	check(panel.tree.get_root().get_child_count() == 1 and panel.selected_key.is_empty(), "switching countries clears the prior queue selection")

	ui.naval_queues_checkbox.button_pressed = true
	ui.eu4_save.text = ""
	ui.eu4_save.text_changed.emit("")
	check(ui.naval_queues_checkbox.disabled and not ui.naval_queues_checkbox.button_pressed, "clearing the EU4 save disables and unchecks queue import")
	ui.eu4_save_picker.file_selected.emit(save_path)
	check(not ui.naval_queues_checkbox.disabled and not ui.naval_queues_checkbox.button_pressed, "choosing the save again enables queue import without checking it")
	check(ui.load_world(), "load a new default world with the queue checkbox cleared")
	select_country(ui, "AAA")
	var fresh_default: Dictionary = JSON.parse_string(ui.session.query_naval_constructions("AAA"))
	check(fresh_default.ok and fresh_default.value.orders.is_empty(), "a new unchecked load clears the previous imported queue")
	check(panel.selected_key.is_empty() and panel.tree.get_root().get_child_count() == 0, "a new load clears the old queue panel and selection")
	check(fresh_default.ok and has_unresolved_province(fresh_default.value.import_report, 1), "the unchecked reload returns to legacy FIFO diagnostics")

	launch_ui = load("res://world.tscn").instantiate()
	root.add_child(launch_ui)
	await process_frame
	var flag := "--import-naval-construction-queues"
	check(launch_ui.parse_launch_args(PackedStringArray([flag, "--eu4-save", save_path, "--game-root", game_root])), "parse the queue boolean before the save path")
	check(launch_ui.naval_queues_checkbox.button_pressed and launch_ui.eu4_save.text == save_path and not launch_ui.naval_queues_checkbox.disabled, "the queue option consumes no value when it precedes the save path")
	launch_ui.eu4_save.text = ""
	launch_ui.game_path.text = ""
	launch_ui.naval_queues_checkbox.button_pressed = false
	check(launch_ui.parse_launch_args(PackedStringArray(["--eu4-save", save_path, flag, "--game-root", game_root])), "parse the save path before the queue boolean")
	check(launch_ui.naval_queues_checkbox.button_pressed and launch_ui.eu4_save.text == save_path and not launch_ui.naval_queues_checkbox.disabled, "the queue option consumes no value when it follows the save path")
	launch_ui.eu4_save.text = ""
	launch_ui.naval_queues_checkbox.button_pressed = false
	check(launch_ui.parse_launch_args(PackedStringArray(["--game-root", game_root, flag])), "parse the queue option without a save value")
	check(launch_ui.naval_queues_checkbox.button_pressed and not launch_ui.load_world(), "requesting queue import without a save fails explicitly")
	check(launch_ui.status.text.contains("EU4 存档"), "the no-save error tells the user that an EU4 save is required")
	check_layout(launch_ui)
	finish()

func write_naval_save(path: String) -> bool:
	var first := saved_order("AAA", "1444.11.10", 3, "1444.11.13", true)
	var tail := saved_order("AAA", "1444.11.11", 2, "1444.11.13", false)
	var beta := saved_order("BBB", "1444.11.10", 13, "1444.11.23", true)
	var unresolved := saved_order("AAA", "1444.11.10", 3, "1444.11.13", true)
	var text := "EU4txt\ndate = 1444.11.11\nplayer = AAA\ncountries = { AAA = { treasury = 20.000 } BBB = { treasury = 10.000 } }\n"
	text += "provinces = { -1 = { owner = AAA controller = AAA %s %s } -2 = { owner = BBB controller = BBB %s } -4 = { owner = AAA controller = AAA %s } }\n" % [first, tail, beta, unresolved]
	return write_text(path, text)

func saved_order(country: String, start_date: String, total: int, completion_date: String, with_progress: bool) -> String:
	var progress := "progress = 0.333 " if with_progress else ""
	return "military_construction = { country = %s envoy = -1 type = sample_light_ship start_date = %s total = %d original_total = %d %sdate = %s cost = 5.000 }" % [country, start_date, total, total, progress, completion_date]

func force_test_ports(game_root: String) -> bool:
	var path := game_root.path_join("map/default.map")
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return false
	var contents := file.get_as_text()
	file.close()
	return write_text(path, contents + "\nforce_coastal = { 1 2 }\n")

func write_text(path: String, contents: String) -> bool:
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(contents)
	var error := file.get_error()
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

func select_country(world: Control, tag: String) -> void:
	for index in world.country_choice.item_count:
		if world.country_choice.get_item_metadata(index) == tag:
			world.select_country(index, false)
			return
	check(false, "find country %s in the selector" % tag)

func treasury(session, tag: String) -> Variant:
	var packet: Dictionary = JSON.parse_string(session.query_country(tag))
	if not packet.get("ok", false):
		return null
	return packet.value.resources.treasury_milli

func count_ships(report: Dictionary) -> int:
	var count := 0
	for fleet in report.get("fleets", []):
		count += fleet.get("ships", []).size()
	return count

func has_unresolved_province(report: Dictionary, province_id: int) -> bool:
	for item in report.get("unresolved", []):
		if item.get("province_id") == province_id:
			return true
	return false

func check_layout(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = node.get_global_rect()
		var ancestor := node.get_parent()
		while ancestor != null:
			if ancestor is Control and ancestor.clip_contents:
				rect = rect.intersection(ancestor.get_global_rect())
			ancestor = ancestor.get_parent()
		if rect.has_area():
			check(root.get_visible_rect().grow(1).encloses(rect), "control outside viewport: %s" % node.name)
	for child in node.get_children():
		check_layout(child)

func finish() -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	if is_instance_valid(launch_ui):
		launch_ui.queue_free()
	cleanup()
	print("Augustus naval construction integration: %d failures" % failures)
	quit(1 if failures else 0)

func cleanup() -> void:
	if temp_name.is_empty():
		return
	var expected := ProjectSettings.globalize_path("user://").path_join(temp_name)
	if temp_root == expected:
		remove_fixture(temp_root)

func remove_fixture(path: String) -> void:
	if path != temp_root and not path.begins_with(temp_root + "/"):
		return
	for name in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(name))
	for name in DirAccess.get_directories_at(path):
		remove_fixture(path.path_join(name))
	DirAccess.remove_absolute(path)

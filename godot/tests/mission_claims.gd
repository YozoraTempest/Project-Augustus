# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")
var mission_fixture := ProjectSettings.globalize_path("res://../examples/mission-claims")
var capture_path := ""

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	capture_path = read_capture_argument(OS.get_cmdline_user_args())
	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = fixture
	ui.player_countries_input.text = "AAA,BBB"
	var base_loaded: bool = ui.load_world()
	check(base_loaded, "load base world without mission content: %s" % ui.status.text)
	if not base_loaded:
		await cleanup(ui, "", "user://mission-claims-snapshot-%d.json" % OS.get_process_id())
		quit(1)
		return
	var empty_missions_shown: bool = ui.mission_summary == "无任务" and ui.mission_selection_status.text == "当前内容没有任务。"
	check(empty_missions_shown, "empty mission content is shown as known empty")
	if not empty_missions_shown:
		await cleanup(ui, "", "user://mission-claims-snapshot-%d.json" % OS.get_process_id())
		quit(1)
		return
	var state_path := write_external_state()
	var snapshot_path := "user://mission-claims-snapshot-%d.json" % OS.get_process_id()
	check(not state_path.is_empty(), "create process-specific mission state")
	if state_path.is_empty():
		await cleanup(ui, "", snapshot_path)
		quit(1)
		return
	ui.external_state.text = state_path
	ui.mod_directories.append(mission_fixture)
	var missions_loaded: bool = ui.load_world()
	check(missions_loaded, "load mission-claims fixture: %s" % ui.status.text)
	if not missions_loaded:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	await process_frame
	select_country(ui, "AAA")
	var before_query: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var initial: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
	var after_query: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(initial.ok, "query AAA mission state: %s" % initial.get("error", ""))
	check(before_query.ok and after_query.ok and before_query.value == after_query.value, "mission query leaves the full snapshot unchanged")
	if not initial.ok or not before_query.ok or not after_query.ok:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	var initial_missions: Array = initial.value.missions
	check(initial.value.selection_known and initial.value.completions_known, "fixture reports known selection and completions")
	var first := find_mission(initial_missions, "aaa_claim_first")
	var second := find_mission(initial_missions, "aaa_claim_second")
	var unknown := find_mission(initial_missions, "aaa_claim_unknown")
	var unsupported := find_mission(initial_missions, "aaa_claim_unsupported")
	check(first.get("selected") == true and first.get("requirements_met") == true and first.get("claimable") == true, "AAA first objective is selected and ready from owns=1: %s" % JSON.stringify(first))
	check(second.get("selected") == true and second.get("requirements_met") == false and second.get("claimable") == false, "second objective starts locked behind its prerequisite")
	check(unknown.get("requirements_met") == null and not unknown.get("claimable", true), "unknown objective remains unavailable")
	check(unsupported.get("reward_supported") == false and not unsupported.get("claimable", true), "unsupported reward remains unavailable")
	check(ui.mission_summary == "已选 4 项" and not ui.mission_content.visible, "selected objective summary stays folded by default")
	ui.mission_header.pressed.emit()
	check(ui.mission_content.visible, "expand the selected objective list")
	var controls_ready: bool = ui.mission_claim_buttons.has("aaa_claim_first") and ui.mission_claim_buttons.has("aaa_claim_second") and ui.mission_claim_buttons.has("aaa_claim_unknown") and ui.mission_claim_buttons.has("aaa_claim_unsupported")
	check(controls_ready, "render all selected objective controls")
	if not controls_ready:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(not ui.mission_claim_buttons["aaa_claim_first"].disabled, "ready objective has an enabled claim button")
	check(ui.mission_claim_buttons["aaa_claim_second"].disabled, "locked objective cannot be claimed")
	check(ui.mission_status_labels["aaa_claim_unknown"].text == "未知" and ui.mission_claim_buttons["aaa_claim_unknown"].disabled, "unknown objective cannot be claimed")
	check(ui.mission_status_labels["aaa_claim_unsupported"].text == "不支持" and ui.mission_claim_buttons["aaa_claim_unsupported"].disabled, "unsupported reward is shown as disabled")
	if first.get("claimable") != true:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(ui.select_province(1), "select province 1 before claiming")
	await process_frame
	await process_frame
	ui.details_scroll.scroll_vertical = 120
	var scroll_before_claim: int = ui.details_scroll.scroll_vertical
	ui.set_running(true)
	ui.mission_claim_buttons["aaa_claim_first"].pressed.emit()
	var after_first: Dictionary = JSON.parse_string(ui.session.query_country_missions("AAA"))
	var after_first_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	if not after_first.ok or not after_first_country.ok:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(after_first_country.ok and after_first_country.value.resources.treasury_milli == 5000, "first reward adds exactly 5 treasury")
	check(after_first.ok and find_mission(after_first.value.missions, "aaa_claim_first").get("completed") == true, "first objective is recorded as completed")
	check(after_first.ok and find_mission(after_first.value.missions, "aaa_claim_second").get("requirements_met") == true and find_mission(after_first.value.missions, "aaa_claim_second").get("claimable") == true, "live variable and prerequisite unlock the second objective")
	check(ui.mission_claim_buttons["aaa_claim_second"].disabled == false, "second objective button becomes enabled after the first claim")
	check(ui.selected_province_id == 1 and ui.details_scroll.scroll_vertical == scroll_before_claim, "claim refresh keeps the selected province and sidebar position")
	var pending: Dictionary = JSON.parse_string(ui.session.pending_event_choices())
	check(pending.ok and pending.value.pending_events.size() == 1, "first claim creates one pending report event")
	check(ui.event_overlay.visible and ui.event_option_buttons.size() == 1 and not ui.running, "claim opens the event overlay and pauses the calendar")
	check(ui.time_controls[0].disabled and ui.time_controls[1].disabled and ui.time_controls[2].disabled, "pending event disables calendar controls")
	if not pending.ok or not ui.event_overlay.visible or ui.event_option_buttons.size() != 1:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	ui.event_option_buttons[0].pressed.emit()
	check(not ui.event_overlay.visible and ui.mission_claim_buttons["aaa_claim_second"].disabled == false, "choosing the event keeps the second objective available")
	if not capture_path.is_empty():
		await capture_mission_panel(ui)
	check(ui.save_world(snapshot_path), "save after the first objective")
	ui.mission_claim_buttons["aaa_claim_second"].pressed.emit()
	var after_second: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	if not after_second.ok:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(after_second.ok and after_second.value.resources.treasury_milli == 12000, "second reward brings treasury to exactly 12")
	var duplicate_before: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var duplicate: Dictionary = JSON.parse_string(ui.session.claim_mission("AAA", "aaa_claim_second"))
	var duplicate_after: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(not duplicate.ok, "repeated claim is rejected")
	check(duplicate_before.ok and duplicate_after.ok and duplicate_before.value == duplicate_after.value, "repeated claim preserves the full snapshot")
	var restored: bool = ui.restore_world(snapshot_path)
	check(restored, "restore the snapshot after the first objective")
	if not restored:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	var restored_country: Dictionary = JSON.parse_string(ui.session.query_country("AAA"))
	if not restored_country.ok:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(restored_country.ok and restored_country.value.resources.treasury_milli == 5000, "restore recovers the first reward balance")
	check(ui.mission_claim_buttons["aaa_claim_second"].disabled == false, "restored second objective is claimable again")
	select_country(ui, "BBB")
	var beta: Dictionary = JSON.parse_string(ui.session.query_country_missions("BBB"))
	if not beta.ok:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(ui.selected_country_id == "BBB" and beta.ok and beta.value.selection_known, "country switch loads BBB's mission selection")
	check(ui.mission_claim_buttons.size() == 1 and ui.mission_claim_buttons.has("bbb_claim_first"), "BBB displays its own selected objective")
	select_country(ui, "AAA")
	check(ui.mission_claim_buttons.has("aaa_claim_first") and ui.mission_claim_buttons["aaa_claim_second"].disabled == false, "switching back refreshes AAA's restored claim state")
	var reloaded: bool = ui.load_world()
	check(reloaded, "loading a new world clears the old mission state")
	if not reloaded:
		await cleanup(ui, state_path, snapshot_path)
		quit(1)
		return
	check(ui.selected_province_id == 0 and ui.mission_summary == "已选 4 项" and not ui.mission_content.visible, "new world clears old task rows and starts folded")
	ui.mission_header.pressed.emit()
	check(not ui.mission_claim_buttons["aaa_claim_first"].disabled and ui.mission_claim_buttons["aaa_claim_second"].disabled, "new world resets claim buttons to the initial chain state")
	await cleanup(ui, state_path, snapshot_path)
	print("Augustus mission claims integration: %d failures" % failures)
	quit(1 if failures else 0)

func find_mission(missions: Array, mission_id: String) -> Dictionary:
	for mission in missions:
		if mission.get("id") == mission_id:
			return mission
	return {}

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.select_country(index)
			return
	check(false, "missing country %s" % tag)

func write_external_state() -> String:
	var path := ProjectSettings.globalize_path("user://mission-claims-state-%d.json" % OS.get_process_id())
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	file.store_string(JSON.stringify({
		"country_resources": {"AAA": {"treasury_milli": 0}, "BBB": {"treasury_milli": 0}},
		"country_variables": {"AAA": {"mission_claims_first": 0}},
		"country_mission_completions": {"AAA": [], "BBB": []},
		"province_owners": {"1": "AAA"}
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

func capture_mission_panel(ui: Control) -> void:
	await process_frame
	var offset := roundi(ui.mission_header.global_position.y - ui.details_scroll.global_position.y)
	ui.details_scroll.scroll_vertical += offset
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var path := ProjectSettings.globalize_path(capture_path)
	var directory := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create capture directory")
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the rendered mission panel")
	if not image.is_empty():
		check(image.save_png(path) == OK, "write mission panel capture")

func cleanup(ui: Control, state_path: String, snapshot_path: String) -> void:
	if not state_path.is_empty():
		DirAccess.remove_absolute(state_path)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(snapshot_path))
	ui.queue_free()
	await process_frame

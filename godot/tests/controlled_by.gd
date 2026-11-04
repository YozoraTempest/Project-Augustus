# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var ui
var snapshot_path := ""
var capture_prefix := ""

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	run.call_deferred()

func run() -> void:
	var args := OS.get_cmdline_user_args()
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			capture_prefix = args[index + 1]
	if not capture_prefix.is_empty() and DisplayServer.get_name() == "headless":
		check(false, "capture requires a graphical display server")
		await finish()
		return
	snapshot_path = "user://controlled-by-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()]
	ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = ProjectSettings.globalize_path("res://../examples/world")
	ui.player_countries_input.text = "AAA,BBB"
	ui.mod_directories.append(ProjectSettings.globalize_path("res://../examples/controlled-by"))
	ui.external_state.text = ProjectSettings.globalize_path("res://../examples/controlled-by/external_state.json")
	check(ui.load_world(), "load the self-authored current-controller fixture: %s" % ui.status.text)
	if ui.overview.is_empty():
		await finish()
		return
	await process_frame
	var before: String = opaque_snapshot()
	select_country("AAA")
	if not ui.mission_expanded:
		ui.mission_header.pressed.emit()
	check_missions(false)
	if not ui.mission_highlight_buttons.has("control_north"):
		await finish()
		return
	ui.mission_highlight_buttons["control_north"].pressed.emit()
	check(highlight_ids() == [2], "AAA targets the province it controls despite BBB ownership")
	check(ui.select_province(2, false), "select AAA's controlled province")
	var province: Dictionary = JSON.parse_string(ui.session.query_province(2))
	check(province.ok and province.value.owner == "BBB" and province.value.controller == "AAA", "province panel reads independent owner and controller")
	check(opaque_snapshot() == before, "mission queries, target display and selection preserve the opaque snapshot")
	await capture_view("aaa")
	select_country("BBB")
	check_missions(true)
	ui.mission_highlight_buttons["control_north"].pressed.emit()
	check(highlight_ids() == [1], "BBB ROOT targets province 1 without changing its ownership")
	check(opaque_snapshot() == before, "switching the query country preserves the opaque snapshot")
	await capture_view("bbb")
	ui.mission_highlight_buttons["control_unknown"].pressed.emit()
	var report: Dictionary = ui.active_mission_highlight.get("report", {})
	check(highlight_ids().is_empty() and report.get("unknown_provinces", {}).has("4"), "NOT controlled_by keeps unknown province 4 unresolved")
	check(ui.map_highlight_status.text.contains("未知"), "unknown controller target is explained in the UI")
	check(opaque_snapshot() == before, "unknown target queries preserve the opaque snapshot")
	ui.mission_highlight_buttons["control_north"].pressed.emit()
	ui.mission_claim_buttons["control_north"].pressed.emit()
	var country: Dictionary = JSON.parse_string(ui.session.query_country("BBB"))
	check(country.ok and country.value.resources.treasury_milli == 3000, "BBB claims exactly three treasury through the live controller condition")
	check(mission("BBB", "control_north").get("completed") == true, "controller-based mission completion is recorded")
	province = JSON.parse_string(ui.session.query_province(1))
	check(province.ok and province.value.owner == "AAA" and province.value.controller == "BBB", "claim leaves ownership and control untouched")
	var after_claim: String = opaque_snapshot()
	check(ui.save_world(snapshot_path), "save the claimed current-controller mission")
	check(ui.advance_world(1), "advance after the claim")
	check(ui.restore_world(snapshot_path), "restore the current-controller mission snapshot")
	check(opaque_snapshot() == after_claim, "restore reproduces the complete claimed snapshot byte for byte")
	check(mission("BBB", "control_north").get("completed") == true, "restored controller mission remains completed")
	check(ui.mission_claim_buttons["control_north"].disabled, "completed controller mission cannot be claimed twice")
	ui.mission_claim_buttons["control_north"].pressed.emit()
	check(opaque_snapshot() == after_claim, "a stale repeat-claim signal cannot duplicate the reward")
	await finish()

func mission(country: String, id: String) -> Dictionary:
	var packet: Dictionary = JSON.parse_string(ui.session.query_country_missions(country))
	check(packet.get("ok") == true, "query country missions")
	for item in packet.get("value", {}).get("missions", []):
		if item.id == id:
			return item
	check(false, "find mission %s" % id)
	return {}

func check_missions(north_ready: bool) -> void:
	var country: String = ui.selected_country_id
	var north := mission(country, "control_north")
	var literal := mission(country, "control_literal")
	var unknown := mission(country, "control_unknown")
	check(north.get("requirements_met") == north_ready and north.get("claimable") == north_ready, "ROOT condition follows %s's current control" % country)
	check(literal.get("requirements_met") == true and literal.get("claimable") == true, "literal BBB condition is independent of the query country")
	check(unknown.get("requirements_met") == null and unknown.get("claimable") == false, "unknown control is not made claimable by negation")
	for id in ["control_north", "control_literal", "control_owner", "control_unknown"]:
		check(ui.mission_claim_buttons.has(id) and ui.mission_highlight_buttons.has(id), "render controller mission controls for %s" % id)
	var owner := mission(country, "control_owner")
	check(owner.get("requirements_met") == false and owner.get("claimable") == false, "owner compares the province controller with its owner rather than the selected country")
	if ui.mission_claim_buttons.has("control_north"):
		check(ui.mission_claim_buttons["control_north"].disabled == (not north_ready), "ROOT mission button reflects its current condition")
	if ui.mission_claim_buttons.has("control_unknown"):
		check(ui.mission_claim_buttons["control_unknown"].disabled and ui.mission_status_labels["control_unknown"].text == "未知", "unknown control is shown separately from an unmet objective")

func select_country(tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.select_country(index, false)
			return
	check(false, "find country %s" % tag)

func highlight_ids() -> Array:
	var ids: Array = []
	for id in ui.active_mission_highlight.get("report", {}).get("provinces", []):
		ids.append(int(id))
	return ids

func opaque_snapshot() -> String:
	var packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(packet.get("ok") == true, "save the opaque snapshot")
	return str(packet.get("value", ""))

func capture_view(suffix: String) -> void:
	if capture_prefix.is_empty():
		return
	await process_frame
	await process_frame
	ui.details_scroll.scroll_vertical += roundi(ui.mission_header.global_position.y - ui.details_scroll.global_position.y)
	await process_frame
	await RenderingServer.frame_post_draw
	check(root.get_texture().get_image().save_png(capture_prefix + "-" + suffix + ".png") == OK, "save the controller mission screenshot")

func finish() -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	if not snapshot_path.is_empty():
		DirAccess.remove_absolute(ProjectSettings.globalize_path(snapshot_path))
	await process_frame
	print("Augustus current-controller conditions integration: %d failures" % failures)
	quit(1 if failures else 0)

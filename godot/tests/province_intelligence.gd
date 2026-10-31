# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	check(not JSON.parse_string(ui.session.query_province_discovery(1, "AAA")).ok, "discovery requires a loaded world")
	var state_path := "user://province-intelligence-%d.json" % OS.get_process_id()
	var snapshot_path := "user://province-intelligence-snapshot-%d.json" % OS.get_process_id()
	var file := FileAccess.open(state_path, FileAccess.WRITE)
	file.store_string(JSON.stringify({
		"province_discovered_by": {"1": ["AAA"]},
		"province_center_of_trade_levels": {"1": 0}
	}))
	file.close()
	ui.game_path.text = ProjectSettings.globalize_path("res://../examples/world")
	ui.external_state.text = ProjectSettings.globalize_path(state_path)
	ui.player_countries_input.text = "AAA"
	ui.mod_directories.append(ProjectSettings.globalize_path("res://../examples/province-intelligence"))
	check(ui.load_world(), "load province-intelligence fixture: %s" % ui.status.text)
	if ui.overview.is_empty():
		DirAccess.remove_absolute(ProjectSettings.globalize_path(state_path))
		quit(1)
		return
	await process_frame
	select_country(ui, "AAA")
	check(ui.select_province(1), "select a province")
	await process_frame
	await process_frame
	check(absf(ui.province_details.global_position.y - ui.details_scroll.global_position.y) <= 1.0, "province selection reveals the start of its details")
	check(ui.province_details.text.contains("贸易中心：无"), "zero level is displayed as no center")
	check(ui.province_details.text.contains("AAA 的发现状态：已发现"), "selected country is known to have discovered the province")
	var before: String = JSON.parse_string(ui.session.save_snapshot()).value
	select_country(ui, "BBB")
	check(ui.province_details.text.contains("BBB 的发现状态：未发现"), "switching country refreshes known absence")
	check(ui.selected_province_id == 1, "country switching keeps the selected province")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before, "panel queries preserve the complete snapshot")
	check(not JSON.parse_string(ui.session.query_province_discovery(-1, "AAA")).ok, "reject negative province")
	check(not JSON.parse_string(ui.session.query_province_discovery(999, "AAA")).ok, "reject unknown province")
	check(not JSON.parse_string(ui.session.query_province_discovery(1, "ZZZ")).ok, "reject unknown country")
	check(ui.select_province(3), "select province with unknown state")
	check(ui.province_details.text.contains("贸易中心：未知") and ui.province_details.text.contains("BBB 的发现状态：未知"), "unknown discovery and center remain unknown")
	ui.select_province(1)
	await process_frame
	await process_frame
	check(ui.save_world(snapshot_path), "save before the delayed event")
	ui.details_scroll.scroll_vertical = 100
	var scroll_before: int = ui.details_scroll.scroll_vertical
	check(ui.advance_world(1), "advance to delayed discovery event")
	check(ui.province_details.text.contains("贸易中心：3 级") and ui.province_details.text.contains("BBB 的发现状态：已发现"), "time advancement refreshes live province values")
	await process_frame
	await process_frame
	check(ui.details_scroll.scroll_vertical == scroll_before, "automatic refresh does not reveal the province again")
	check(ui.event_overlay.visible and ui.event_option_buttons.size() == 1, "delayed event exposes a choice")
	if ui.event_option_buttons.size() == 1:
		ui.event_option_buttons[0].pressed.emit()
		check(ui.province_details.text.contains("贸易中心：1 级"), "event choice refreshes the province immediately")
		select_country(ui, "AAA")
		ui.select_province(3)
		check(ui.province_details.text.contains("AAA 的发现状态：已发现"), "event effect creates individually known discovery")
	ui.select_province(1)
	check(ui.restore_world(snapshot_path), "restore before the event")
	check(ui.selected_province_id == 1 and ui.province_details.text.contains("贸易中心：无"), "restore refreshes the selected province")
	select_country(ui, "BBB")
	check(ui.province_details.text.contains("BBB 的发现状态：未发现"), "restore recovers discovery state")
	check(not ui.select_province(999) and ui.selected_province_id == 1, "failed selection preserves current province")
	check(ui.load_world(), "reload world")
	check(ui.selected_province_id == 0 and ui.province_details.text == "点击地图查看省份。", "new world clears old province selection")
	DirAccess.remove_absolute(ProjectSettings.globalize_path(state_path))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(snapshot_path))
	ui.queue_free()
	await process_frame
	print("Augustus province intelligence integration: %d failures" % failures)
	quit(1 if failures else 0)

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.select_country(index)
			return
	check(false, "missing country %s" % tag)

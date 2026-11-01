# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures: int = 0
var fixture: String = ProjectSettings.globalize_path("res://../examples/world")
var culture_mod: String = ProjectSettings.globalize_path("res://../examples/accepted-cultures")
var snapshot_path: String = "user://accepted-cultures-%d.json" % OS.get_process_id()
var ui: Variant

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var scene: PackedScene = load("res://world.tscn") as PackedScene
	ui = scene.instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = fixture
	ui.player_countries_input.text = "BBB"
	ui.mod_directories.append(culture_mod)
	var loaded: bool = ui.load_world()
	check(loaded, "load the base world with the accepted-cultures Mod: %s" % ui.status.text)
	if not loaded:
		await cleanup()
		finish()
		return

	var initial_snapshot: String = core_snapshot()
	check(not initial_snapshot.is_empty(), "read the initial opaque core snapshot")
	var initial_packet: Dictionary = query_province()
	check(initial_packet.get("ok") == true, "query province 1: %s" % str(initial_packet.get("error", "")))
	if initial_packet.get("ok") != true:
		await cleanup()
		finish()
		return
	var initial_province: Dictionary = initial_packet.get("value", {})
	var initial_tax: Dictionary = tax_basis(initial_province)
	check(int(initial_tax.get("known_subtotal_micros", -1)) == 212000, "province 1 starts with the known tax-basis subtotal 212000")
	check(initial_tax.get("value_micros") == null, "unresolved tax inputs keep the full tax-basis value unknown")
	check(has_contribution(initial_tax, "static:same_culture_group", -150000), "initial tax basis includes the same-culture-group contribution")
	check(initial_snapshot == core_snapshot(), "province query leaves the opaque core snapshot unchanged")

	var start_packet: Dictionary = {}
	var start_pending: Array = []
	for _day in 32:
		start_packet = pending_event_packet()
		if start_packet.get("ok") == true:
			var packet_value: Dictionary = start_packet.get("value", {})
			start_pending = packet_value.get("pending_events", [])
			if not start_pending.is_empty():
				break
		var advanced: bool = ui.advance_world(1)
		if not advanced:
			break
	check(start_pending.size() == 1, "the monthly pulse opens only the BBB start event")
	if start_pending.size() != 1:
		await cleanup()
		finish()
		return
	check(start_pending[0].get("event") == "accepted_cultures.start", "the pending monthly event is the start event")
	check(ui.event_option_buttons.size() == 1, "the start event exposes its acceptance option")
	if ui.event_option_buttons.size() != 1:
		await cleanup()
		finish()
		return
	check(current_date() == {"year": 1444, "month": 12, "day": 1}, "the monthly event starts on 1444.12.01")
	ui.event_option_buttons[0].pressed.emit()

	var accepted_snapshot: String = core_snapshot()
	check(not accepted_snapshot.is_empty(), "read the snapshot after accepting alpha")
	var accepted_packet: Dictionary = query_province()
	check(accepted_packet.get("ok") == true, "query province 1 after accepting alpha")
	if accepted_packet.get("ok") != true:
		await cleanup()
		finish()
		return
	var accepted_province: Dictionary = accepted_packet.get("value", {})
	var accepted_tax: Dictionary = tax_basis(accepted_province)
	check(int(accepted_tax.get("known_subtotal_micros", -1)) == 531000, "accepting alpha increases the known tax-basis subtotal")
	check(accepted_tax.get("value_micros") == null, "accepting alpha does not resolve the other tax inputs")
	check(not has_contribution(accepted_tax, "static:same_culture_group", -150000), "accepted alpha removes the same-culture-group tax contribution")
	check(accepted_snapshot == core_snapshot(), "the live province query leaves the accepted core snapshot unchanged")
	var saved: bool = ui.save_world(snapshot_path)
	check(saved, "save the world while BBB accepts alpha")
	if not saved:
		await cleanup()
		finish()
		return

	var advanced_to_followup: bool = ui.advance_world(1)
	check(advanced_to_followup, "advance one day to the scheduled follow-up")
	if not advanced_to_followup:
		await cleanup()
		finish()
		return
	var followup_packet: Dictionary = pending_event_packet()
	var followup_events: Array = followup_packet.get("value", {}).get("pending_events", [])
	check(followup_events.size() == 1 and followup_events[0].get("event") == "accepted_cultures.remove_alpha", "the next-day follow-up is pending")
	check(current_date() == {"year": 1444, "month": 12, "day": 2}, "the follow-up runs on 1444.12.02")
	check(ui.event_option_buttons.size() == 1, "accepted_culture = alpha keeps the removal option available")
	if ui.event_option_buttons.size() != 1:
		await cleanup()
		finish()
		return
	ui.event_option_buttons[0].pressed.emit()

	var removed_packet: Dictionary = query_province()
	check(removed_packet.get("ok") == true, "query province 1 after removing acceptance")
	if removed_packet.get("ok") != true:
		await cleanup()
		finish()
		return
	var removed_province: Dictionary = removed_packet.get("value", {})
	var removed_tax: Dictionary = tax_basis(removed_province)
	check(int(removed_tax.get("known_subtotal_micros", -1)) == 212000, "removing alpha restores the known tax-basis subtotal to 212000")
	check(removed_tax.get("value_micros") == null, "the full tax-basis value remains unknown after removal")
	check(has_contribution(removed_tax, "static:same_culture_group", -150000), "removing alpha restores the same-culture-group contribution")
	var uninterrupted_snapshot: String = core_snapshot()

	var restored: bool = ui.restore_world(snapshot_path)
	check(restored, "restore the snapshot taken while BBB accepted alpha")
	if not restored:
		await cleanup()
		finish()
		return
	var restored_date: Dictionary = current_date()
	check(restored_date == {"year": 1444, "month": 12, "day": 1}, "restore recovers the accepted snapshot date")
	var restored_snapshot: String = core_snapshot()
	var restored_packet: Dictionary = query_province()
	check(restored_packet.get("ok") == true, "query province 1 after restore")
	if restored_packet.get("ok") != true:
		await cleanup()
		finish()
		return
	var restored_province: Dictionary = restored_packet.get("value", {})
	var restored_tax: Dictionary = tax_basis(restored_province)
	check(int(restored_tax.get("known_subtotal_micros", -1)) == 531000, "restore recovers the accepted-culture tax basis")
	check(not has_contribution(restored_tax, "static:same_culture_group", -150000), "restore keeps the same-culture-group contribution removed")
	check(restored_snapshot == core_snapshot(), "the restored province query leaves the opaque core snapshot unchanged")
	var advanced_after_restore: bool = ui.advance_world(1)
	check(advanced_after_restore, "continue one day from the accepted snapshot")
	if not advanced_after_restore:
		await cleanup()
		finish()
		return
	var restored_followup: Dictionary = pending_event_packet()
	var restored_events: Array = restored_followup.get("value", {}).get("pending_events", [])
	check(restored_events.size() == 1 and restored_events[0].get("event") == "accepted_cultures.remove_alpha", "restore preserves the scheduled follow-up")
	check(current_date() == {"year": 1444, "month": 12, "day": 2}, "restored follow-up runs on the next calendar day")
	check(ui.event_option_buttons.size() == 1, "the restored acceptance still satisfies the live event condition")
	if ui.event_option_buttons.size() == 1:
		ui.event_option_buttons[0].pressed.emit()
		check(core_snapshot() == uninterrupted_snapshot, "restored continuation matches the uninterrupted removal snapshot")
	else:
		await cleanup()
		finish()
		return

	var restored_removed_packet: Dictionary = query_province()
	check(restored_removed_packet.get("ok") == true, "query province 1 after restored removal")
	if restored_removed_packet.get("ok") == true:
		var restored_removed_province: Dictionary = restored_removed_packet.get("value", {})
		var restored_removed_tax: Dictionary = tax_basis(restored_removed_province)
		check(int(restored_removed_tax.get("known_subtotal_micros", -1)) == 212000, "restored continuation removes acceptance and restores subtotal 212000")
		check(has_contribution(restored_removed_tax, "static:same_culture_group", -150000), "restored continuation restores the same-culture-group contribution")
	await cleanup()
	finish()

func query_province() -> Dictionary:
	var packet: Variant = JSON.parse_string(ui.session.query_province(1))
	if packet is Dictionary:
		return packet
	return {"ok": false, "error": "invalid province-query response"}

func pending_event_packet() -> Dictionary:
	var packet: Variant = JSON.parse_string(ui.session.pending_event_choices())
	if packet is Dictionary:
		return packet
	return {"ok": false, "error": "invalid pending-event response"}

func tax_basis(province: Dictionary) -> Dictionary:
	var attributes: Dictionary = province.get("attributes", {})
	return attributes.get("tax_basis", {})

func has_contribution(evaluation: Dictionary, source: String, amount: int) -> bool:
	var steps: Array = evaluation.get("steps", [])
	for step_value in steps:
		if not step_value is Dictionary:
			continue
		var step: Dictionary = step_value
		var contributions: Array = step.get("contributions", [])
		for contribution_value in contributions:
			if not contribution_value is Dictionary:
				continue
			var contribution: Dictionary = contribution_value
			if contribution.get("source") == source and int(contribution.get("micros", 0)) == amount:
				return true
	return false

func core_snapshot() -> String:
	var packet: Variant = JSON.parse_string(ui.session.save_snapshot())
	if packet is Dictionary and packet.get("ok") == true:
		return str(packet.get("value", ""))
	return ""

func current_date() -> Dictionary:
	var packet: Variant = JSON.parse_string(ui.session.simulation_view())
	if not packet is Dictionary or packet.get("ok") != true:
		return {}
	var value: Dictionary = packet.get("value", {})
	var date: Dictionary = value.get("date", {})
	return {
		"year": int(date.get("year", -1)),
		"month": int(date.get("month", -1)),
		"day": int(date.get("day", -1))
	}

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func cleanup() -> void:
	var absolute_snapshot: String = ProjectSettings.globalize_path(snapshot_path)
	if FileAccess.file_exists(absolute_snapshot):
		check(DirAccess.remove_absolute(absolute_snapshot) == OK, "remove this process's accepted-cultures snapshot")
	if is_instance_valid(ui):
		ui.queue_free()
	await process_frame

func finish() -> void:
	print("Augustus accepted cultures integration: %d failures" % failures)
	quit(1 if failures > 0 else 0)

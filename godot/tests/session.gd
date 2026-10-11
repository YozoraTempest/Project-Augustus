# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	var uninterrupted = AugustusSession.new()
	var resumed = AugustusSession.new()
	var native = AugustusSession.new()
	check(JSON.parse_string(uninterrupted.start_session(true, false)).ok, "start EU4 session")
	check(JSON.parse_string(resumed.start_session(true, false)).ok, "start resumed session")
	check(JSON.parse_string(native.start_session(true, true)).ok, "start native session")
	for session in [uninterrupted, native]:
		check(JSON.parse_string(session.build(0, 0)).ok, "queue first building")
		check(JSON.parse_string(session.build(1, 1)).ok, "queue second building")
		check(JSON.parse_string(session.advance(14)).ok, "advance to mid-queue save")
	var save: Dictionary = JSON.parse_string(uninterrupted.save_snapshot())
	check(save.ok, "save queued tasks")
	check(JSON.parse_string(resumed.restore_snapshot(save.value)).ok, "restore queued tasks")
	check(not JSON.parse_string(resumed.build(-1, 0)).ok, "reject invalid command")
	for session in [uninterrupted, native]:
		session.advance(86)
	for _day in 86:
		resumed.query()
		resumed.query()
		resumed.advance(1)
	check(uninterrupted.save_snapshot() == resumed.save_snapshot(), "refresh frequency and save-resume equivalence")
	var legacy_state: Dictionary = JSON.parse_string(uninterrupted.query()).value
	var native_state: Dictionary = JSON.parse_string(native.query()).value
	legacy_state.erase("input")
	native_state.erase("input")
	check(legacy_state == native_state, "both content inputs reach the same core state")
	check(legacy_state.treasury_milli == 667500, "cost and rewards are exact")
	check(legacy_state.pending_tasks == 0, "no duplicate standard settlement remains")
	for session in [uninterrupted, resumed, native]:
		check(JSON.parse_string(session.reorganise(1)).ok, "selected provider changes persistent capacity")
	check(uninterrupted.save_snapshot() == resumed.save_snapshot(), "random state survives the cross-language save")
	var packed: PackedScene = load("res://main.tscn")
	var ui = packed.instantiate()
	root.add_child.call_deferred(ui)
	verify_ui.call_deferred(ui)

func verify_ui(ui: Control) -> void:
	await process_frame
	await process_frame
	check_layout(ui)
	check(ui.state.day == 0, "UI starts paused")
	ui.build_button.pressed.emit()
	check(ui.state.treasury_milli == 900000, "UI build button dispatches to Rust")
	ui.apply_packet(ui.session.advance(30))
	check(ui.state.provinces[0].buildings == ["market"], "UI displays the actual completion")
	await process_frame
	await process_frame
	check_layout(ui)
	ui.system_choice.select(1)
	ui.new_session()
	ui.build_button.pressed.emit()
	await process_frame
	await process_frame
	check_layout(ui)
	var arguments := OS.get_cmdline_user_args()
	if arguments.size() == 2 and arguments[0] == "--capture":
		await RenderingServer.frame_post_draw
		var image := root.get_texture().get_image()
		check(not image.is_empty(), "rendered preview is available")
		if not image.is_empty():
			check(image.save_png(arguments[1]) == OK, "save rendered preview")
	print("Augustus Godot integration: %d failures" % failures)
	quit(1 if failures else 0)

func check_layout(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = node.get_global_rect()
		var viewport: Rect2 = root.get_visible_rect().grow(1)
		check(viewport.encloses(rect), "control outside viewport: %s %s" % [node.name, rect])
	for child in node.get_children():
		check_layout(child)

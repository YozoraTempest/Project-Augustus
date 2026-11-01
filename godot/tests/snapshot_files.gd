# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const WORLD_FIXTURE := "res://../examples/world"
const MARKER_TEXT := "snapshot-files-test-marker"

var failures := 0
var test_root := ""
var capture_prefix := ""

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	test_root = ProjectSettings.globalize_path(
		"user://snapshot-files-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()]
	)
	if DirAccess.make_dir_recursive_absolute(test_root) != OK:
		push_error("create snapshot file test directory")
		quit(1)
		return
	capture_prefix = read_capture_prefix(OS.get_cmdline_user_args())
	if not capture_prefix.is_empty() and DisplayServer.get_name() == "headless":
		check(false, "--capture requires a graphical display server")
		capture_prefix = ""
	run.call_deferred()

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func run() -> void:
	verify_uninitialised_sessions_preserve_file()
	await verify_world_snapshot_files()
	await verify_construction_snapshot_files()
	remove_owned_tree(test_root)
	print("Snapshot file integration: %d failures" % failures)
	quit(1 if failures else 0)

func verify_uninitialised_sessions_preserve_file() -> void:
	var path := test_root.path_join("uninitialised.json")
	check(write_text(path, MARKER_TEXT), "create existing file for uninitialised session checks")
	for session in [AugustusWorldSession.new(), AugustusSession.new()]:
		var packet: Dictionary = JSON.parse_string(session.save_snapshot_to_file(path))
		check(not packet.get("ok", true), "uninitialised session rejects file save")
		check(FileAccess.get_file_as_string(path) == MARKER_TEXT, "uninitialised session preserves existing file")
	check(DirAccess.get_files_at(test_root).size() == 1, "uninitialised sessions leave no temporary files")

func verify_world_snapshot_files() -> void:
	root.size = Vector2i(1100, 720)
	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	await process_frame
	ui.game_path.text = ProjectSettings.globalize_path(WORLD_FIXTURE)
	ui.player_countries_input.text = "AAA"
	var loaded: bool = ui.load_world()
	check(loaded, "world fixture loads for snapshot file integration")
	if not loaded:
		ui.queue_free()
		await process_frame
		return
	var world_path: String = test_root.path_join("world/campaign.json")
	check(DirAccess.make_dir_recursive_absolute(world_path.get_base_dir()) == OK, "create world snapshot directory")
	var initial_snapshot: String = opaque_snapshot(ui)
	check(ui.save_world(world_path), "world UI saves to an explicit path")
	check(FileAccess.get_file_as_string(world_path) == initial_snapshot, "world file contains the exact opaque snapshot string")
	var first_day: int = world_day(ui)
	check(ui.advance_world(1) and world_day(ui) == first_day + 1, "world advances after its first file save")
	var replacement_snapshot: String = opaque_snapshot(ui)
	check(replacement_snapshot != initial_snapshot, "world advance changes the opaque snapshot before overwrite")
	check(ui.save_world(world_path), "world UI overwrites the same snapshot path")
	check(FileAccess.get_file_as_string(world_path) == replacement_snapshot, "world overwrite contains the latest opaque snapshot")
	var saved_day: int = world_day(ui)
	check(ui.advance_world(1) and world_day(ui) == saved_day + 1, "world advances after the overwritten save")
	check(ui.restore_world(world_path), "world UI restores the overwritten snapshot")
	check(world_day(ui) == saved_day and opaque_snapshot(ui) == replacement_snapshot, "world restore returns to the overwritten snapshot byte-for-byte")
	await verify_failed_snapshot_writes(
		ui, Callable(ui, "save_world"), ui.status, "world", "战役保存失败：", world_path, replacement_snapshot
	)
	ui.queue_free()
	await process_frame

func verify_construction_snapshot_files() -> void:
	root.size = Vector2i(1100, 720)
	var ui = load("res://main.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	await process_frame
	var demo_path: String = test_root.path_join("construction/campaign.json")
	check(DirAccess.make_dir_recursive_absolute(demo_path.get_base_dir()) == OK, "create construction snapshot directory")
	var initial_snapshot: String = opaque_snapshot(ui)
	var initial_day: int = int(ui.state.day)
	check(ui.save_game(demo_path), "construction UI saves to an explicit path")
	check(FileAccess.get_file_as_string(demo_path) == initial_snapshot, "construction file contains the exact opaque snapshot string")
	ui.advance_one_button.pressed.emit()
	check(int(ui.state.day) == initial_day + 1, "construction demo advances after its first file save")
	var replacement_snapshot: String = opaque_snapshot(ui)
	check(replacement_snapshot != initial_snapshot, "construction advance changes the opaque snapshot before overwrite")
	check(ui.save_game(demo_path), "construction UI overwrites the same snapshot path")
	check(FileAccess.get_file_as_string(demo_path) == replacement_snapshot, "construction overwrite contains the latest opaque snapshot")
	var saved_day: int = int(ui.state.day)
	ui.advance_one_button.pressed.emit()
	check(int(ui.state.day) == saved_day + 1, "construction demo advances after the overwritten save")
	check(ui.restore_game(demo_path), "construction UI restores the overwritten snapshot")
	check(int(ui.state.day) == saved_day and opaque_snapshot(ui) == replacement_snapshot, "construction restore returns to the overwritten snapshot byte-for-byte")
	await verify_failed_snapshot_writes(
		ui, Callable(ui, "save_game"), ui.status_label, "construction", "保存失败：", demo_path, replacement_snapshot
	)
	ui.queue_free()
	await process_frame

func verify_failed_snapshot_writes(
	ui,
	save_method: Callable,
	status_label: Label,
	name: String,
	error_prefix: String,
	prior_save_path: String,
	prior_snapshot: String
) -> void:
	var failure_root: String = test_root.path_join(name + "-write-failures")
	check(DirAccess.make_dir_recursive_absolute(failure_root) == OK, "create %s failure-test directory" % name)
	var blocked_target: String = failure_root.path_join("existing-directory-target")
	check(DirAccess.make_dir_recursive_absolute(blocked_target) == OK, "create %s directory save target" % name)
	var marker_path: String = blocked_target.path_join("marker.txt")
	check(write_text(marker_path, MARKER_TEXT), "write %s protected marker" % name)
	var snapshot_before_failure: String = opaque_snapshot(ui)
	root.size = Vector2i(1100, 720)
	var directory_result: Variant = save_method.call(blocked_target)
	check(directory_result == false, "%s UI rejects an existing directory as a snapshot file" % name)
	check_error_status(status_label, error_prefix, "%s directory-target failure" % name)
	check(opaque_snapshot(ui) == snapshot_before_failure, "%s directory-target failure preserves opaque session bytes" % name)
	check(FileAccess.get_file_as_string(prior_save_path) == prior_snapshot, "%s directory-target failure preserves the prior snapshot file" % name)
	check(FileAccess.get_file_as_string(marker_path) == MARKER_TEXT, "%s directory-target failure preserves the existing marker" % name)
	check(DirAccess.get_files_at(failure_root).is_empty(), "%s directory-target failure leaves no sibling temporary file" % name)
	var target_files: PackedStringArray = DirAccess.get_files_at(blocked_target)
	check(target_files.size() == 1 and target_files.has("marker.txt"), "%s directory-target failure leaves no temporary file in the target" % name)
	await capture_error_status(ui, status_label, "%s-1100-directory" % name)

	var missing_parent: String = failure_root.path_join("missing-parent")
	var missing_target: String = missing_parent.path_join("campaign.json")
	check(not DirAccess.dir_exists_absolute(missing_parent), "%s missing-parent path starts absent" % name)
	root.size = Vector2i(640, 480)
	var missing_result: Variant = save_method.call(missing_target)
	check(missing_result == false, "%s UI rejects a snapshot path under a missing parent" % name)
	check_error_status(status_label, error_prefix, "%s missing-parent failure" % name)
	check(opaque_snapshot(ui) == snapshot_before_failure, "%s missing-parent failure preserves opaque session bytes" % name)
	check(FileAccess.get_file_as_string(prior_save_path) == prior_snapshot, "%s missing-parent failure preserves the prior snapshot file" % name)
	check(not DirAccess.dir_exists_absolute(missing_parent), "%s missing-parent failure does not create the absent directory" % name)
	check(DirAccess.get_files_at(failure_root).is_empty(), "%s missing-parent failure leaves no temporary file" % name)
	await capture_error_status(ui, status_label, "%s-640-missing-parent" % name)

func check_error_status(status_label: Label, prefix: String, context: String) -> void:
	check(status_label.is_visible_in_tree() and str(status_label.text).begins_with(prefix), "%s updates the visible error status" % context)

func capture_error_status(ui, status_label: Label, suffix: String) -> void:
	await process_frame
	await process_frame
	ui.page_scroll.ensure_control_visible(status_label)
	await process_frame
	await process_frame
	check(status_label.get_visible_line_count() == status_label.get_line_count(), "failure status is not truncated in %s" % suffix)
	check(ui.page_scroll.get_global_rect().encloses(status_label.get_global_rect()), "failure status is reachable in %s" % suffix)
	if capture_prefix.is_empty():
		return
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture %s failure status" % suffix)
	if image.is_empty():
		return
	var path: String = capture_prefix + "-" + suffix + ".png"
	var directory: String = path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create snapshot failure screenshot directory")
	check(image.save_png(path) == OK, "save %s failure screenshot" % suffix)

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture":
			if index + 1 < args.size():
				return args[index + 1]
			check(false, "--capture requires an output prefix")
	return ""

func world_day(ui) -> int:
	var packet: Dictionary = JSON.parse_string(ui.session.simulation_view())
	if not packet.get("ok", false):
		check(false, "read world day from the simulation view")
		return -1
	return int(packet.value.day)

func opaque_snapshot(ui) -> String:
	var packet: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	if not packet.get("ok", false):
		check(false, "read current opaque snapshot string")
		return ""
	return str(packet.value)

func write_text(path: String, content: String) -> bool:
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(content)
	var write_ok: bool = file.get_error() == OK
	file.close()
	return write_ok

func remove_owned_tree(path: String) -> void:
	var owned_root := test_root.simplify_path()
	var resolved_path := ProjectSettings.globalize_path(path).simplify_path()
	if owned_root.is_empty() or not owned_root.is_absolute_path() or not (
		resolved_path == owned_root or resolved_path.begins_with(owned_root + "/")
	):
		check(false, "refuse cleanup outside the owned snapshot test directory")
		return
	if not DirAccess.dir_exists_absolute(path):
		return
	for filename in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(filename))
	for dirname in DirAccess.get_directories_at(path):
		remove_owned_tree(path.path_join(dirname))
	DirAccess.remove_absolute(path)

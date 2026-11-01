# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

const WORLD_FIXTURE := "res://../examples/world"

var failures := 0
var test_root := ""
var capture_prefix := ""
var world_snapshot_for_cross_restore := ""

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	test_root = ProjectSettings.globalize_path(
		"user://snapshot-dialogs-114-owned-long-integration-path-%d-%d-for-overwrite-confirmation" % [OS.get_process_id(), Time.get_ticks_usec()]
	)
	if DirAccess.make_dir_recursive_absolute(test_root) != OK:
		push_error("create snapshot dialog test directory")
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
	await wait_frames(2)
	await verify_world_dialogs()
	await verify_construction_dialogs()
	remove_owned_tree(test_root)
	print("Snapshot file dialog integration: %d failures" % failures)
	quit(1 if failures else 0)

func verify_world_dialogs() -> void:
	var ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await wait_frames(2)
	ui.game_path.text = ProjectSettings.globalize_path(WORLD_FIXTURE)
	ui.player_countries_input.text = "AAA"
	var loaded: bool = ui.load_world()
	check(loaded, "load the self-authored world fixture before opening project snapshots")
	if loaded:
		await exercise_dialog_workflow(ui, true, "world")
	ui.queue_free()
	await process_frame

func verify_construction_dialogs() -> void:
	var ui = load("res://main.tscn").instantiate()
	root.add_child(ui)
	await wait_frames(2)
	await exercise_dialog_workflow(ui, false, "construction")
	ui.queue_free()
	await process_frame

func exercise_dialog_workflow(ui, is_world: bool, name: String) -> void:
	var dialogs = ui.snapshot_dialogs
	var save_dialog: FileDialog = dialogs.save_dialog
	var load_dialog: FileDialog = dialogs.load_dialog
	var save_button: Button = ui.snapshot_save_as_button
	var open_button: Button = ui.snapshot_open_button
	var status_label: Label = ui.status if is_world else ui.status_label
	var load_method: Callable = Callable(ui, "restore_world") if is_world else Callable(ui, "restore_game")
	var owned_dir: String = test_root.path_join(name + "-owned-long-subdirectory-for-project-snapshot-dialogs")
	check(DirAccess.make_dir_recursive_absolute(owned_dir) == OK, "create %s owned snapshot directory" % name)
	var target_path: String = owned_dir.path_join("campaign-with-a-long-file-name-for-save-dialog-overwrite.json")
	var older_path: String = owned_dir.path_join("older-snapshot.json")
	var invalid_path: String = owned_dir.path_join("invalid-snapshot.json")
	var late_save_path: String = owned_dir.path_join("late-save-must-not-be-created.json")
	var initial_snapshot: String = opaque_snapshot(ui)
	check(write_text(older_path, initial_snapshot), "%s create an owned opaque restore fixture" % name)

	await set_viewport(Vector2i(1100, 720), name + " desktop")
	await check_control_reachable(ui, save_button, name + " save-as button is reachable at 1100x720")
	await check_control_reachable(ui, open_button, name + " open button is reachable at 1100x720")
	start_running(ui, is_world)
	check(is_running(ui, is_world), "%s starts running before modal open pauses it" % name)
	var before_open_snapshot: String = opaque_snapshot(ui)
	await open_file_dialog(save_button, save_dialog, target_path, name + " new save", true)
	check(not is_running(ui, is_world), "%s pauses before showing the save dialog" % name)
	check(opaque_snapshot(ui) == before_open_snapshot, "%s opening the save dialog does not change the session")
	check_window_bounds(save_dialog, Vector2i(1100, 720), name + " save dialog fits 1100x720")
	await capture_window(save_dialog, name + "-1100-save")
	await cancel_outer_dialog(save_dialog, name + " cancel new save")
	check(not FileAccess.file_exists(target_path), "%s cancelling a new save creates no file" % name)
	check(opaque_snapshot(ui) == before_open_snapshot, "%s cancelling a new save leaves the session unchanged" % name)
	check(not dialogs.pending, "%s outer save cancellation clears the active request" % name)

	await open_file_dialog(save_button, save_dialog, target_path, name + " create save")
	await press_dialog_ok(save_dialog, name + " create save")
	var saved_snapshot: String = opaque_snapshot(ui)
	check(saved_snapshot == before_open_snapshot, "%s save selection preserves the opaque session snapshot" % name)
	check(FileAccess.file_exists(target_path), "%s save-as selection creates the selected owned file" % name)
	check(FileAccess.get_file_as_string(target_path) == saved_snapshot, "%s selected save path receives exact opaque snapshot bytes" % name)
	check(str(status_label.text).contains(target_path.get_file()), "%s success status names the selected file" % name)

	check(advance_one(ui, is_world), "%s advances before testing overwrite" % name)
	var overwrite_snapshot: String = opaque_snapshot(ui)
	check(overwrite_snapshot != saved_snapshot, "%s advance changes the snapshot before overwrite" % name)
	if is_world:
		world_snapshot_for_cross_restore = overwrite_snapshot
	await open_file_dialog(save_button, save_dialog, target_path, name + " overwrite cancel")
	await press_dialog_ok(save_dialog, name + " overwrite cancel")
	var overwrite_dialog: ConfirmationDialog = find_visible_confirmation(save_dialog)
	check(overwrite_dialog != null, "%s existing-file save opens an overwrite confirmation" % name)
	if overwrite_dialog != null:
		check_window_bounds(overwrite_dialog, Vector2i(1100, 720), name + " overwrite confirmation fits 1100x720")
		await capture_window(overwrite_dialog, name + "-1100-overwrite")
		await press_confirmation_cancel(overwrite_dialog, name + " cancel overwrite")
	check(FileAccess.get_file_as_string(target_path) == saved_snapshot, "%s cancelling overwrite preserves the existing file" % name)
	check(opaque_snapshot(ui) == overwrite_snapshot, "%s cancelling overwrite preserves the session" % name)
	dialogs.dismiss()
	await wait_frames(2)

	await open_file_dialog(save_button, save_dialog, target_path, name + " overwrite confirm")
	await press_dialog_ok(save_dialog, name + " overwrite confirm")
	overwrite_dialog = find_visible_confirmation(save_dialog)
	check(overwrite_dialog != null, "%s overwrite confirmation can be reopened" % name)
	if overwrite_dialog != null:
		await press_confirmation_ok(overwrite_dialog, name + " confirm overwrite")
	await wait_frames(2)
	check(FileAccess.get_file_as_string(target_path) == overwrite_snapshot, "%s confirming overwrite replaces the selected file")
	check(opaque_snapshot(ui) == overwrite_snapshot, "%s successful overwrite does not mutate session state")

	await set_viewport(Vector2i(640, 480), name + " narrow")
	await check_control_reachable(ui, save_button, name + " save-as button is reachable at 640x480")
	await check_control_reachable(ui, open_button, name + " open button is reachable at 640x480")
	var before_narrow_save: String = opaque_snapshot(ui)
	await open_file_dialog(save_button, save_dialog, target_path, name + " narrow save dialog")
	check_window_bounds(save_dialog, Vector2i(640, 480), name + " save dialog fits 640x480")
	await capture_window(save_dialog, name + "-640-save")
	await cancel_outer_dialog(save_dialog, name + " narrow save cancel")
	check(opaque_snapshot(ui) == before_narrow_save, "%s cancelling narrow save leaves session unchanged" % name)
	check(FileAccess.get_file_as_string(target_path) == overwrite_snapshot, "%s cancelling narrow save leaves file unchanged" % name)

	check(advance_one(ui, is_world), "%s advances before selecting a saved file")
	var divergent_snapshot: String = opaque_snapshot(ui)
	await open_file_dialog(open_button, load_dialog, target_path, name + " open saved file")
	check(not is_running(ui, is_world), "%s stays paused while the open dialog is visible" % name)
	check_window_bounds(load_dialog, Vector2i(640, 480), name + " open dialog fits 640x480")
	await capture_window(load_dialog, name + "-640-open")
	await press_dialog_ok(load_dialog, name + " select saved file")
	check(opaque_snapshot(ui) == overwrite_snapshot, "%s opening the selected file restores its exact saved snapshot" % name)
	check(opaque_snapshot(ui) != divergent_snapshot, "%s selected-file restore replaces the divergent session" % name)
	check(str(status_label.text).contains(target_path.get_file()), "%s open status names the selected file" % name)

	check(advance_one(ui, is_world), "%s advances before cancelling an open dialog")
	var before_open_cancel: String = opaque_snapshot(ui)
	await open_file_dialog(open_button, load_dialog, target_path, name + " open cancel")
	await cancel_outer_dialog(load_dialog, name + " cancel open")
	check(opaque_snapshot(ui) == before_open_cancel, "%s cancelling open leaves the session unchanged" % name)
	check(FileAccess.get_file_as_string(target_path) == overwrite_snapshot, "%s cancelling open leaves the selected file unchanged" % name)

	check(write_text(invalid_path, "not-a-project-snapshot"), "%s write invalid snapshot fixture" % name)
	var before_invalid_load: String = opaque_snapshot(ui)
	await open_file_dialog(open_button, load_dialog, invalid_path, name + " invalid open")
	var invalid_status_before: String = str(status_label.text)
	await press_dialog_ok(load_dialog, name + " invalid open")
	check(opaque_snapshot(ui) == before_invalid_load, "%s invalid selected file preserves the current session" % name)
	var invalid_status_after: String = str(status_label.text)
	check(not invalid_status_after.is_empty() and invalid_status_after != invalid_status_before, "%s invalid selected file changes the success status to a read error" % name)
	check(FileAccess.get_file_as_string(invalid_path) == "not-a-project-snapshot", "%s invalid read leaves the source file unchanged" % name)

	var valid_restore_result: Variant = load_method.call(target_path)
	check(valid_restore_result == true, "%s restore a valid project snapshot before cross-format rejection" % name)
	var success_status_before_foreign: String = str(status_label.text)
	check(success_status_before_foreign.contains(target_path.get_file()), "%s has a success status before cross-format read" % name)
	var foreign_path: String = owned_dir.path_join("foreign-snapshot.json")
	var foreign_snapshot: String = ""
	if is_world:
		var construction_session = AugustusSession.new()
		var construction_started: Dictionary = JSON.parse_string(construction_session.start_session(true, false))
		check(construction_started.get("ok", false), "start a self-authored construction session for foreign snapshot input")
		var construction_packet: Dictionary = JSON.parse_string(construction_session.save_snapshot())
		check(construction_packet.get("ok", false), "save a self-authored construction snapshot for world rejection")
		foreign_snapshot = str(construction_packet.get("value", ""))
	else:
		foreign_snapshot = world_snapshot_for_cross_restore
		check(not foreign_snapshot.is_empty(), "retain an opaque world snapshot for construction rejection")
	check(write_text(foreign_path, foreign_snapshot), "%s write a valid snapshot from the other session type" % name)
	var before_foreign_read: String = opaque_snapshot(ui)
	await open_file_dialog(open_button, load_dialog, foreign_path, name + " foreign-format open")
	var foreign_status_before: String = str(status_label.text)
	await press_dialog_ok(load_dialog, name + " foreign-format open")
	check(opaque_snapshot(ui) == before_foreign_read, "%s foreign-format read preserves the current session" % name)
	var foreign_status_after: String = str(status_label.text)
	check(not foreign_status_after.is_empty() and foreign_status_after != foreign_status_before, "%s foreign-format read changes the success status to an error" % name)
	check(FileAccess.get_file_as_string(foreign_path) == foreign_snapshot, "%s foreign-format rejection preserves the selected source file" % name)

	await open_file_dialog(open_button, load_dialog, target_path, name + " direct restore invalidation")
	var direct_restore_result: Variant = load_method.call(older_path)
	check(direct_restore_result == true, "%s direct restore succeeds while the load dialog is open" % name)
	var direct_restore_snapshot: String = opaque_snapshot(ui)
	check(direct_restore_snapshot == initial_snapshot, "%s direct restore loads the older owned snapshot" % name)
	check(not load_dialog.visible and not save_dialog.visible and not dialogs.pending, "%s direct restore dismisses active snapshot dialogs" % name)
	load_dialog.file_selected.emit(target_path)
	await wait_frames(2)
	check(opaque_snapshot(ui) == direct_restore_snapshot, "%s late load selection cannot override direct restore" % name)

	await open_file_dialog(save_button, save_dialog, target_path, name + " reset while overwrite is open")
	await press_dialog_ok(save_dialog, name + " reset while overwrite is open")
	overwrite_dialog = find_visible_confirmation(save_dialog)
	check(overwrite_dialog != null, "%s reset test opens nested overwrite confirmation" % name)
	if overwrite_dialog != null:
		check_window_bounds(overwrite_dialog, Vector2i(640, 480), name + " nested overwrite fits 640x480")
		await capture_window(overwrite_dialog, name + "-640-overwrite")
	var reset_ok: bool
	if is_world:
		reset_ok = ui.load_world()
	else:
		ui.new_session()
		reset_ok = not ui.state.is_empty()
	check(reset_ok, "%s loads a new world or starts a new session with overwrite open" % name)
	check(not save_dialog.visible and not load_dialog.visible and not dialogs.pending, "%s reset dismisses active file dialogs" % name)
	if overwrite_dialog != null:
		check(not overwrite_dialog.visible, "%s reset closes the nested overwrite confirmation" % name)
	var after_reset_snapshot: String = opaque_snapshot(ui)
	save_dialog.file_selected.emit(late_save_path)
	await wait_frames(2)
	check(not FileAccess.file_exists(late_save_path), "%s late save selection after reset creates no file" % name)
	check(opaque_snapshot(ui) == after_reset_snapshot, "%s late save selection after reset preserves the new session")
	check(FileAccess.get_file_as_string(target_path) == overwrite_snapshot, "%s stale save callback leaves the prior selected file unchanged" % name)

func open_file_dialog(button: Button, dialog: FileDialog, path: String, context: String, omit_json_extension: bool = false) -> void:
	prepare_file_dialog(dialog, path)
	button.pressed.emit()
	await wait_frames(2)
	check(dialog.visible, "%s opens its FileDialog" % context)
	check(dialog.current_dir == path.get_base_dir(), "%s uses the test-owned directory" % context)
	var line_edit: LineEdit = dialog.get_line_edit()
	check(line_edit != null, "%s exposes the public file-name control" % context)
	var entered_name: String = path.get_file().get_basename() if omit_json_extension else path.get_file()
	if line_edit != null:
		line_edit.text = entered_name
		line_edit.text_changed.emit(line_edit.text)
		await wait_frames(2)
		if FileAccess.file_exists(path):
			var selected: bool = select_listed_file(dialog, path.get_file())
			if not selected:
				var entries := PackedStringArray()
				describe_item_lists(dialog, entries)
				print("%s could not select %s: current_dir=%s current_file=%s items=%s" % [context, path.get_file(), dialog.current_dir, dialog.current_file, " | ".join(entries)])
			check(selected, "%s selects the existing file in the public ItemList" % context)
			await wait_frames(2)
		check(line_edit.text == entered_name, "%s selects through the public file-name control" % context)
		check(not dialog.get_ok_button().disabled, "%s enables the accept button after file-name input" % context)

func prepare_file_dialog(dialog: FileDialog, path: String) -> void:
	dialog.current_dir = path.get_base_dir()
	dialog.invalidate()

func select_listed_file(node: Node, filename: String) -> bool:
	for child in node.get_children(true):
		if child is ItemList:
			var item_list: ItemList = child
			for index in item_list.item_count:
				var metadata: Variant = item_list.get_item_metadata(index)
				var item_name: String = str(metadata.get("name", item_list.get_item_text(index))) if metadata is Dictionary else item_list.get_item_text(index)
				if item_name == filename or item_list.get_item_text(index) == filename:
					item_list.select(index)
					item_list.item_selected.emit(index)
					return item_list.is_selected(index)
		if select_listed_file(child, filename):
			return true
	return false

func describe_item_lists(node: Node, entries: PackedStringArray) -> void:
	for child in node.get_children(true):
		if child is ItemList:
			var item_list: ItemList = child
			for index in item_list.item_count:
				var metadata: Variant = item_list.get_item_metadata(index)
				var item_name: String = str(metadata.get("name", item_list.get_item_text(index))) if metadata is Dictionary else item_list.get_item_text(index)
				entries.append("%s selected=%s" % [item_name, item_list.is_selected(index)])
		describe_item_lists(child, entries)

func press_dialog_ok(dialog: FileDialog, context: String) -> void:
	var button: Button = dialog.get_ok_button()
	check(button != null and not button.disabled, "%s exposes an enabled FileDialog accept button" % context)
	if button != null and not button.disabled:
		button.pressed.emit()
	await wait_frames(3)

func cancel_outer_dialog(dialog: FileDialog, context: String) -> void:
	var button: Button = dialog.get_cancel_button()
	check(button != null, "%s exposes the FileDialog cancel button" % context)
	if button != null:
		button.pressed.emit()
	await wait_frames(3)
	check(not dialog.visible, "%s closes the FileDialog" % context)

func find_visible_confirmation(node: Node) -> ConfirmationDialog:
	for child in node.get_children(true):
		if child is ConfirmationDialog and (child as Window).visible:
			return child as ConfirmationDialog
		var nested: ConfirmationDialog = find_visible_confirmation(child)
		if nested != null:
			return nested
	return null

func press_confirmation_cancel(dialog: ConfirmationDialog, context: String) -> void:
	var button: Button = dialog.get_cancel_button()
	check(button != null, "%s exposes the overwrite cancel button" % context)
	if button != null:
		button.pressed.emit()
	await wait_frames(3)
	check(not dialog.visible, "%s closes the overwrite confirmation" % context)

func press_confirmation_ok(dialog: ConfirmationDialog, context: String) -> void:
	var button: Button = dialog.get_ok_button()
	check(button != null, "%s exposes the overwrite confirmation button" % context)
	if button != null:
		button.pressed.emit()
	await wait_frames(4)
	check(not dialog.visible, "%s closes after confirming overwrite" % context)

func set_viewport(viewport_size: Vector2i, context: String) -> void:
	root.size = viewport_size
	await wait_frames(2)
	check(root.size == viewport_size, "%s uses native viewport %dx%d" % [context, viewport_size.x, viewport_size.y])
	check(root.get_visible_rect().size == Vector2(viewport_size), "%s visible viewport follows its window size" % context)

func check_control_reachable(ui, control: Control, context: String) -> void:
	ui.page_scroll.ensure_control_visible(control)
	await wait_frames(2)
	var viewport_rect: Rect2 = ui.page_scroll.get_global_rect()
	var control_rect: Rect2 = control.get_global_rect()
	check(
		control_rect.position.x >= viewport_rect.position.x - 1.0
		and control_rect.end.x <= viewport_rect.end.x + 1.0
		and control_rect.position.y >= viewport_rect.position.y - 1.0
		and control_rect.end.y <= viewport_rect.end.y + 1.0,
		context
	)

func check_window_bounds(window: Window, viewport_size: Vector2i, context: String) -> void:
	var viewport_rect: Rect2 = Rect2(Vector2.ZERO, Vector2(viewport_size))
	if not window.is_embedded():
		viewport_rect = Rect2(Vector2(root.position), Vector2(root.size))
	var window_rect: Rect2 = Rect2(Vector2(window.position), Vector2(window.size))
	check(
		window_rect.position.x >= viewport_rect.position.x - 1.0
		and window_rect.position.y >= viewport_rect.position.y - 1.0
		and window_rect.end.x <= viewport_rect.end.x + 1.0
		and window_rect.end.y <= viewport_rect.end.y + 1.0,
		context
	)

func capture_window(window: Window, suffix: String) -> void:
	if capture_prefix.is_empty():
		return
	await RenderingServer.frame_post_draw
	var image: Image = window.get_texture().get_image()
	check(not image.is_empty(), "capture %s dialog window" % suffix)
	if image.is_empty():
		return
	var path: String = ProjectSettings.globalize_path(capture_prefix) + "-" + suffix + ".png"
	var directory: String = path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create snapshot dialog screenshot directory")
	check(image.save_png(path) == OK, "write %s dialog screenshot" % suffix)

func start_running(ui, is_world: bool) -> void:
	if is_world:
		if not ui.running:
			ui.run_button.pressed.emit()
	else:
		if ui.paused:
			ui.pause_button.pressed.emit()

func is_running(ui, is_world: bool) -> bool:
	return ui.running if is_world else not ui.paused

func advance_one(ui, is_world: bool) -> bool:
	if is_world:
		return ui.advance_world(1)
	return ui.apply_packet(ui.session.advance(1))

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

func wait_frames(count: int) -> void:
	for _index in count:
		await process_frame

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture":
			if index + 1 < args.size():
				return args[index + 1]
			check(false, "--capture requires an output prefix")
	return ""

func remove_owned_tree(path: String) -> void:
	var owned_root := test_root.simplify_path()
	var resolved_path := ProjectSettings.globalize_path(path).simplify_path()
	if owned_root.is_empty() or not owned_root.is_absolute_path() or not (
		resolved_path == owned_root or resolved_path.begins_with(owned_root + "/")
	):
		check(false, "refuse cleanup outside the owned snapshot dialog test directory")
		return
	if not DirAccess.dir_exists_absolute(path):
		return
	for filename in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(filename))
	for dirname in DirAccess.get_directories_at(path):
		remove_owned_tree(path.path_join(dirname))
	DirAccess.remove_absolute(path)

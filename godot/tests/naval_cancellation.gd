# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures: int = 0
var ui: Control
var synthetic_host: Control
var temp_name: String = ""
var temp_root: String = ""
var capture_prefix: String = ""
var synthetic_cancellations: Array = []

func _initialize() -> void:
	run.call_deferred()

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func run() -> void:
	capture_prefix = read_capture_prefix(OS.get_cmdline_user_args())
	if not capture_prefix.is_empty() and DisplayServer.get_name() == "headless":
		check(false, "--capture requires a graphical display server")
		capture_prefix = ""
	root.content_scale_size = Vector2i.ZERO
	root.size = Vector2i(1100, 720)

	var world_fixture: String = ProjectSettings.globalize_path("res://../examples/world")
	var save_fixture: String = ProjectSettings.globalize_path("res://../examples/naval-queues/save.eu4")
	temp_name = "naval-cancellation-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()]
	temp_root = ProjectSettings.globalize_path("user://").path_join(temp_name)
	var game_root: String = temp_root.path_join("game")
	var initial_snapshot_path: String = temp_root.path_join("initial.snapshot")
	var cancelled_snapshot_path: String = temp_root.path_join("cancelled.snapshot")
	var invalid_snapshot_path: String = temp_root.path_join("invalid.snapshot")
	check(copy_fixture(world_fixture, game_root), "copy the self-authored world fixture once")
	check(force_test_ports(game_root), "make both sample construction provinces coastal in the temporary world")
	check(write_text(invalid_snapshot_path, "{}"), "write a self-authored invalid snapshot for restore rejection")
	if failures:
		await finish()
		return

	ui = load("res://world.tscn").instantiate()
	root.add_child(ui)
	await process_frame
	ui.game_path.text = game_root
	ui.eu4_save_picker.file_selected.emit(save_fixture)
	check(ui.eu4_save.text == save_fixture, "select the existing self-authored naval-queues save")
	check(not ui.naval_queues_checkbox.disabled, "choosing the save enables the FIFO import option")
	ui.naval_queues_checkbox.button_pressed = true
	var loaded: bool = ui.load_world()
	check(loaded, "load the save with both countries' valid coastal orders: %s" % ui.status.text)
	if not loaded:
		await finish()
		return
	await process_frame
	await process_frame
	select_country(ui, "AAA")

	var panel = ui.naval_construction_panel
	panel.toggle_section()
	var initial_aaa: Dictionary = query_naval(ui.session, "AAA")
	var initial_bbb: Dictionary = query_naval(ui.session, "BBB")
	var initial_all: Dictionary = query_naval(ui.session, "")
	check(initial_aaa.get("ok", false) and initial_aaa.value.orders.size() == 2, "AAA has the saved FIFO head and tail")
	check(initial_bbb.get("ok", false) and initial_bbb.value.orders.size() == 1, "BBB has a separate valid order for cross-country rejection checks")
	check(initial_all.get("ok", false) and initial_all.value.orders.size() == 3, "the complete report contains all three saved orders")
	if not initial_aaa.get("ok", false) or not initial_bbb.get("ok", false) or not initial_all.get("ok", false):
		await finish()
		return

	var initial_orders: Array = initial_aaa.value.orders
	var initial_head: Dictionary = initial_orders[0]
	var initial_tail: Dictionary = initial_orders[1]
	var head_id: String = str(initial_head.get("id", ""))
	var tail_id: String = str(initial_tail.get("id", ""))
	var bbb_id: String = str(initial_bbb.value.orders[0].get("id", ""))
	check(initial_head.get("id") is String and initial_tail.get("id") is String and initial_head.get("id") != initial_tail.get("id"), "construction IDs cross into Godot as distinct decimal strings")
	check(initial_head.get("queue_position") == 1 and initial_tail.get("queue_position") == 2, "the saved AAA orders preserve FIFO positions")
	check(not head_id.is_empty() and not tail_id.is_empty() and not bbb_id.is_empty(), "all queried order IDs are nonempty strings")
	var initial_head_schedule: Variant = initial_head.get("completion_date")
	var initial_tail_schedule: Variant = initial_tail.get("completion_date")
	var initial_snapshot: String = save_snapshot_opaque(ui.session)
	check(not initial_snapshot.is_empty(), "save the campaign snapshot as opaque bytes")
	check(write_text(initial_snapshot_path, initial_snapshot), "write the original snapshot for isolated restore cases")
	var initial_military: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
	check(initial_military.get("ok", false), "query AAA's starting military roster")
	var initial_ship_count: int = count_ships(initial_military.get("value", {}))
	check_layout(ui)

	await verify_large_id_signal(initial_snapshot)
	check(save_snapshot_opaque(ui.session) == initial_snapshot, "the standalone large-ID panel never writes synthetic state into the core snapshot")

	check_rejected_without_mutation(ui.session, "BBB", head_id, "AAA order cannot be cancelled through BBB")
	check_rejected_without_mutation(ui.session, "AAA", bbb_id, "BBB order cannot be cancelled through AAA")
	check_rejected_without_mutation(ui.session, "AAA", "not-a-decimal-id", "malformed decimal IDs are rejected")
	check_rejected_without_mutation(ui.session, "AAA", "18446744073709551615", "an unknown but valid decimal ID is rejected", "not pending")

	var snapshot_before_dialog: String = save_snapshot_opaque(ui.session)
	ui.set_running(true)
	await open_cancellation(panel, head_id)
	check(not ui.running, "opening cancellation confirmation pauses the world")
	check(panel.cancel_button.text.contains("不退款"), "the action button states that cancellation does not refund")
	check(panel.cancel_dialog.dialog_text.contains("不会退款"), "confirmation text explicitly says there is no refund")
	check(panel.cancel_dialog.dialog_text.contains("已记录的支出"), "confirmation text preserves the recorded expense")
	check(panel.cancel_dialog.dialog_text.contains("保留当前排期"), "confirmation text says remaining orders keep their current schedule")
	check(panel.cancel_dialog.dialog_autowrap, "confirmation dialog wraps its details")
	check(save_snapshot_opaque(ui.session) == snapshot_before_dialog, "opening confirmation does not mutate the core snapshot")
	check_cancel_dialog_layout(panel)
	check_layout(ui)
	if not capture_prefix.is_empty():
		await capture_view("confirmation")
	panel.cancel_dialog.get_cancel_button().pressed.emit()
	await process_frame
	await process_frame
	check(not panel.cancel_dialog.visible, "choosing to keep the order closes the confirmation")
	check(not ui.running, "choosing to keep the order does not resume the simulation automatically")
	check(save_snapshot_opaque(ui.session) == snapshot_before_dialog, "dismissing confirmation leaves the opaque snapshot unchanged")

	await open_cancellation(panel, head_id)
	select_country(ui, "BBB")
	await process_frame
	check(not panel.cancel_dialog.visible, "switching countries dismisses a pending confirmation")
	var after_country_switch: String = save_snapshot_opaque(ui.session)
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	check(save_snapshot_opaque(ui.session) == after_country_switch, "a confirmation emitted after country change cannot cancel an order")
	check(query_naval(ui.session, "AAA").value.orders.size() == 2, "country switch leaves both AAA orders intact")
	select_country(ui, "AAA")

	await open_cancellation(panel, head_id)
	var reloaded: bool = ui.load_world()
	check(reloaded, "loading the same save replaces the active session: %s" % ui.status.text)
	await process_frame
	await process_frame
	check(not panel.cancel_dialog.visible, "a new load dismisses an old cancellation confirmation")
	var reloaded_aaa: Dictionary = query_naval(ui.session, "AAA")
	check(reloaded_aaa.get("ok", false) and reloaded_aaa.value.orders.size() == 2, "the same save reloads both AAA orders")
	if reloaded_aaa.get("ok", false) and reloaded_aaa.value.orders.size() == 2:
		check(str(reloaded_aaa.value.orders[0].get("id", "")) == head_id, "same-save reload reuses the stable head ID for the stale-confirmation check")
	var after_reload_snapshot: String = save_snapshot_opaque(ui.session)
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	check(save_snapshot_opaque(ui.session) == after_reload_snapshot, "a stale confirmation cannot cancel a reused ID after a new load")
	check(query_naval(ui.session, "AAA").value.orders.size() == 2, "both orders survive a stale confirmation after reload")

	await open_cancellation(panel, head_id)
	var failed_restore: bool = ui.restore_world(invalid_snapshot_path)
	check(not failed_restore, "an invalid snapshot restore is rejected")
	check(panel.cancel_dialog.visible, "failed restore keeps the current confirmation open")
	check(save_snapshot_opaque(ui.session) == after_reload_snapshot, "failed restore preserves the active core snapshot")
	var before_head_cancel_treasury: Variant = country_treasury(ui.session, "AAA")
	var before_head_cancel_ledger_packet: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(before_head_cancel_ledger_packet.get("ok", false), "query ledger before cancelling the head")
	var before_head_cancel_ledger: Variant = before_head_cancel_ledger_packet.get("value")
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	await process_frame
	check(not panel.cancel_dialog.visible, "confirming cancellation closes the dialog")
	var after_head_cancel: Dictionary = query_naval(ui.session, "AAA")
	var after_head_cancel_bbb: Dictionary = query_naval(ui.session, "BBB")
	check(after_head_cancel.get("ok", false) and after_head_cancel.value.orders.size() == 1, "cancelling the FIFO head leaves exactly one AAA order")
	if after_head_cancel.get("ok", false) and after_head_cancel.value.orders.size() == 1:
		var remaining_tail: Dictionary = after_head_cancel.value.orders[0]
		check(str(remaining_tail.get("id", "")) == tail_id, "head cancellation preserves the original tail ID")
		check(remaining_tail.get("queue_position") == 1, "the remaining tail becomes FIFO position one")
		check(remaining_tail.get("completion_date") == initial_tail_schedule, "head cancellation preserves the tail's current completion schedule")
	check(after_head_cancel_bbb.get("ok", false) and after_head_cancel_bbb.value.orders.size() == 1 and str(after_head_cancel_bbb.value.orders[0].get("id", "")) == bbb_id, "cancelling AAA's head leaves BBB's separate order unchanged")
	check(country_treasury(ui.session, "AAA") == before_head_cancel_treasury, "cancellation does not refund AAA's treasury")
	var after_head_cancel_ledger_packet: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(after_head_cancel_ledger_packet.get("ok", false), "query ledger after cancelling the head")
	check(after_head_cancel_ledger_packet.get("value") == before_head_cancel_ledger, "cancellation leaves every ledger value unchanged")
	var cancelled_snapshot: String = save_snapshot_opaque(ui.session)
	check(not cancelled_snapshot.is_empty() and cancelled_snapshot != initial_snapshot, "cancelling an order changes only the campaign snapshot state")
	check_rejected_without_mutation(ui.session, "AAA", head_id, "a cancelled ID is stale and cannot be cancelled again")
	check(write_text(cancelled_snapshot_path, cancelled_snapshot), "write the one-order snapshot for restore and completion")

	check(ui.restore_world(cancelled_snapshot_path), "restore the snapshot after head cancellation")
	await process_frame
	await process_frame
	check(save_snapshot_opaque(ui.session) == cancelled_snapshot, "restoring the cancelled snapshot preserves it byte-for-byte")
	var restored_tail: Dictionary = query_naval(ui.session, "AAA")
	check(restored_tail.get("ok", false) and restored_tail.value.orders.size() == 1, "restore recovers the uncancelled tail")
	if restored_tail.get("ok", false) and restored_tail.value.orders.size() == 1:
		check(str(restored_tail.value.orders[0].get("id", "")) == tail_id, "restored tail keeps its original decimal ID")
		check(restored_tail.value.orders[0].get("completion_date") == initial_tail_schedule, "restored tail keeps its unmodified schedule")
	check(ui.advance_world(10), "advance the restored remaining order through completion")
	var completed_tail: Dictionary = query_naval(ui.session, "AAA")
	check(completed_tail.get("ok", false) and completed_tail.value.orders.is_empty(), "the restored remaining order completes and leaves the queue")
	var completed_military: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
	check(completed_military.get("ok", false), "query the roster after the remaining order completes")
	if completed_military.get("ok", false):
		check(count_ships(completed_military.value) == initial_ship_count + 1, "only the uncancelled ship is added to the roster")

	check(ui.restore_world(initial_snapshot_path), "restore the original two-order campaign")
	await process_frame
	await process_frame
	check(save_snapshot_opaque(ui.session) == initial_snapshot, "the original opaque snapshot roundtrips exactly")
	await open_cancellation(panel, tail_id)
	check(ui.restore_world(initial_snapshot_path), "restore the same snapshot while a confirmation is open")
	await process_frame
	await process_frame
	check(not panel.cancel_dialog.visible, "successful restore dismisses the old confirmation even when IDs are reused")
	var after_same_restore: String = save_snapshot_opaque(ui.session)
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	check(save_snapshot_opaque(ui.session) == after_same_restore, "a hidden pre-restore confirmation cannot cancel the restored ID")
	check(query_naval(ui.session, "AAA").value.orders.size() == 2, "restore leaves both reused IDs pending")

	await open_cancellation(panel, head_id)
	check(select_order(panel, tail_id), "selecting a different order while confirmation is open")
	await process_frame
	check(not panel.cancel_dialog.visible, "changing the selected order dismisses the old confirmation")
	var after_selection_change: String = save_snapshot_opaque(ui.session)
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	check(save_snapshot_opaque(ui.session) == after_selection_change, "a stale confirmation after selection change has no effect")

	var before_tail_cancel: String = save_snapshot_opaque(ui.session)
	var before_tail_treasury: Variant = country_treasury(ui.session, "AAA")
	var before_tail_ledger_packet: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(before_tail_ledger_packet.get("ok", false), "query ledger before cancelling the tail")
	var before_tail_ledger: Variant = before_tail_ledger_packet.get("value")
	await open_cancellation(panel, tail_id)
	panel.cancel_dialog.confirmed.emit()
	await process_frame
	await process_frame
	var after_tail_cancel: Dictionary = query_naval(ui.session, "AAA")
	check(after_tail_cancel.get("ok", false) and after_tail_cancel.value.orders.size() == 1, "cancelling the FIFO tail leaves exactly one AAA order")
	if after_tail_cancel.get("ok", false) and after_tail_cancel.value.orders.size() == 1:
		var remaining_head: Dictionary = after_tail_cancel.value.orders[0]
		check(str(remaining_head.get("id", "")) == head_id, "tail cancellation preserves the original head ID")
		check(remaining_head.get("queue_position") == 1, "the head remains FIFO position one")
		check(remaining_head.get("completion_date") == initial_head_schedule, "tail cancellation preserves the head schedule")
	check(country_treasury(ui.session, "AAA") == before_tail_treasury, "tail cancellation does not refund treasury")
	var after_tail_ledger_packet: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(after_tail_ledger_packet.get("ok", false), "query ledger after cancelling the tail")
	check(after_tail_ledger_packet.get("value") == before_tail_ledger, "tail cancellation leaves every ledger value unchanged")
	check(save_snapshot_opaque(ui.session) != before_tail_cancel, "tail cancellation updates the campaign snapshot")
	check_rejected_without_mutation(ui.session, "AAA", tail_id, "the cancelled tail ID becomes stale")

	ui.details_scroll.ensure_control_visible(panel.tree)
	await process_frame
	if not capture_prefix.is_empty():
		await capture_view("remaining-queue")
	root.size = Vector2i(800, 600)
	await process_frame
	await open_cancellation(panel, head_id)
	check_cancel_dialog_layout(panel)
	if not capture_prefix.is_empty():
		await capture_view("narrow")
	panel.cancel_dialog.get_cancel_button().pressed.emit()
	root.size = Vector2i(1100, 720)
	await process_frame
	await process_frame
	check_layout(ui)
	await finish()

func query_naval(session, country: String) -> Dictionary:
	return JSON.parse_string(session.query_naval_constructions(country))

func country_treasury(session, country: String) -> Variant:
	var packet: Dictionary = JSON.parse_string(session.query_country(country))
	if not packet.get("ok", false):
		return null
	return packet.get("value", {}).get("resources", {}).get("treasury_milli")

func save_snapshot_opaque(session) -> String:
	var packet: Variant = JSON.parse_string(session.save_snapshot())
	if not packet is Dictionary or not packet.get("ok", false):
		check(false, "save opaque campaign snapshot")
		return ""
	return str(packet.get("value", ""))

func check_rejected_without_mutation(session, country: String, order_id: String, message: String, expected_error: String = "") -> void:
	var before: String = save_snapshot_opaque(session)
	var packet: Variant = JSON.parse_string(session.cancel_naval_construction(country, order_id))
	check(packet is Dictionary and not packet.get("ok", false), "%s" % message)
	if not expected_error.is_empty():
		check(packet is Dictionary and str(packet.get("error", "")).contains(expected_error), "%s returns the expected error" % message)
	check(save_snapshot_opaque(session) == before, "%s leaves the opaque snapshot unchanged" % message)

func open_cancellation(panel, order_id: String) -> void:
	check(select_order(panel, order_id), "select order %s" % order_id)
	check(panel.cancel_button.visible and not panel.cancel_button.disabled, "selected order enables its cancellation action")
	panel.cancel_button.pressed.emit()
	await process_frame
	await process_frame
	check(panel.cancel_dialog.visible, "selected order opens its confirmation dialog")

func select_order(panel, order_id: String) -> bool:
	var tree_root: TreeItem = panel.tree.get_root()
	if tree_root == null:
		return false
	for index in tree_root.get_child_count():
		var item: TreeItem = tree_root.get_child(index)
		var metadata: Variant = item.get_metadata(0)
		if metadata is Dictionary and str(metadata.get("id", "")) == order_id:
			item.select(0)
			panel.tree.item_selected.emit()
			return true
	return false

func verify_large_id_signal(snapshot_before: String) -> void:
	synthetic_host = Control.new()
	synthetic_host.visible = false
	root.add_child(synthetic_host)
	var synthetic_panel = load("res://naval_construction_panel.gd").new()
	synthetic_host.add_child(synthetic_panel)
	await process_frame
	var large_id: String = "18446744073709551615"
	synthetic_panel.render_report({
		"orders": [{
			"id": large_id,
			"country": "AAA",
			"province_id": 1,
			"province_name": "North",
			"unit_type": "sample_light_ship",
			"queue_position": 1,
			"completion_date": {"year": 1444, "month": 11, "day": 30},
		}],
		"import_report": {"unresolved": []},
	})
	synthetic_panel.toggle_section()
	var emissions_before: int = synthetic_cancellations.size()
	synthetic_panel.cancellation_confirmed.connect(record_synthetic_cancellation)
	check(select_order(synthetic_panel, large_id), "synthetic panel selects a u64-sized ID without numeric conversion")
	check(synthetic_panel.selected_key == large_id, "Tree metadata retains the full decimal ID string")
	synthetic_panel.cancel_button.pressed.emit()
	synthetic_panel.cancel_dialog.confirmed.emit()
	check(synthetic_cancellations.size() == emissions_before + 1, "synthetic confirmation emits one cancellation")
	if synthetic_cancellations.size() > emissions_before:
		var emission: Array = synthetic_cancellations.back()
		check(emission[0] == "AAA" and emission[1] == large_id, "panel signal preserves every digit of a u64 ID")
	check(save_snapshot_opaque(ui.session) == snapshot_before, "synthetic panel cancellation does not touch the real session")
	synthetic_host.queue_free()
	synthetic_host = null
	await process_frame

func record_synthetic_cancellation(country: String, order_id: String) -> void:
	synthetic_cancellations.append([country, order_id])

func check_cancel_dialog_layout(panel) -> void:
	var dialog_rect: Rect2 = Rect2(Vector2(panel.cancel_dialog.position), Vector2(panel.cancel_dialog.size))
	var viewport_rect: Rect2 = root.get_visible_rect() if panel.cancel_dialog.is_embedded() else Rect2(Vector2(root.position), Vector2(root.size))
	viewport_rect = viewport_rect.grow(2.0)
	check(panel.cancel_dialog.size.x <= root.size.x, "confirmation dialog width fits the logical viewport")
	check(panel.cancel_dialog.size.y <= root.size.y, "confirmation dialog height fits the logical viewport")
	check(viewport_rect.encloses(dialog_rect), "confirmation dialog stays within the logical viewport")

func check_layout(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = node.get_global_rect()
		var ancestor: Node = node.get_parent()
		while ancestor != null:
			if ancestor is Control and ancestor.clip_contents:
				rect = rect.intersection(ancestor.get_global_rect())
			ancestor = ancestor.get_parent()
		if rect.has_area():
			check(root.get_visible_rect().grow(1.0).encloses(rect), "control outside viewport: %s" % node.name)
	for child in node.get_children():
		check_layout(child)

func capture_view(suffix: String) -> void:
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var image: Image = root.get_texture().get_image()
	check(not image.is_empty(), "capture the naval cancellation view")
	if image.is_empty():
		return
	var path: String = capture_prefix + "-" + suffix + ".png"
	var directory: String = path.get_base_dir()
	if not DirAccess.dir_exists_absolute(directory):
		check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create capture directory")
	check(image.save_png(path) == OK, "write the cancellation dialog capture")

func read_capture_prefix(args: PackedStringArray) -> String:
	for index in args.size():
		if args[index] == "--capture" and index + 1 < args.size():
			return args[index + 1]
	return ""

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

func force_test_ports(game_root: String) -> bool:
	var path: String = game_root.path_join("map/default.map")
	var file: FileAccess = FileAccess.open(path, FileAccess.READ)
	if file == null:
		return false
	var contents: String = file.get_as_text()
	file.close()
	return write_text(path, contents + "\nforce_coastal = { 1 2 }\n")

func write_text(path: String, contents: String) -> bool:
	var file: FileAccess = FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(contents)
	var error: Error = file.get_error()
	file.close()
	return error == OK

func count_ships(report: Dictionary) -> int:
	var count: int = 0
	for fleet in report.get("fleets", []):
		count += fleet.get("ships", []).size()
	return count

func select_country(world: Control, tag: String) -> void:
	for index in world.country_choice.item_count:
		if world.country_choice.get_item_metadata(index) == tag:
			world.select_country(index, false)
			return
	check(false, "find country %s in the selector" % tag)

func cleanup_fixture() -> void:
	if temp_name.is_empty():
		return
	var expected: String = ProjectSettings.globalize_path("user://").path_join(temp_name)
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

func finish() -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	if is_instance_valid(synthetic_host):
		synthetic_host.queue_free()
	await process_frame
	cleanup_fixture()
	print("Augustus naval cancellation integration: %d failures" % failures)
	quit(1 if failures else 0)

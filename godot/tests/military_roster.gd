# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	root.set_deferred("size", Vector2i(1100, 720))
	var ui = load("res://world.tscn").instantiate()
	root.add_child.call_deferred(ui)
	verify_ui.call_deferred(ui)

func verify_ui(ui: Control) -> void:
	await process_frame
	var external_state_path := write_military_state()
	check(not external_state_path.is_empty(), "create a self-authored military state bundle")
	if external_state_path.is_empty():
		finish()
		return
	ui.game_path.text = fixture
	ui.player_countries_input.text = "AAA,BBB"
	ui.external_state.text = external_state_path
	var loaded: bool = ui.load_world()
	check(loaded, "load the example world with translated military state: %s" % ui.status.text)
	await process_frame
	await process_frame
	if not loaded:
		DirAccess.remove_absolute(external_state_path)
		finish()
		return
	select_country(ui, "AAA")
	var panel = ui.military_panel
	var aaa_report: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
	check(aaa_report.ok, "query the selected country's military roster")
	if aaa_report.ok:
		check(aaa_report.value.armies.size() == 1 and aaa_report.value.fleets.size() == 1, "AAA receives only its army and fleet")
		check(aaa_report.value.armies[0].regiments.size() == 2, "query includes the army's two regiments")
		check(aaa_report.value.armies[0].id is String and aaa_report.value.fleets[0].id is String, "formation IDs reach Godot without numeric precision loss")
	check(panel.section_button.text.contains("已载入编队：2"), "section reports loaded army and fleet formations")
	check(panel.tree.get_root().get_child_count() == 2, "Tree lists the selected country's two formations")
	check(not panel.section_expanded, "military section starts collapsed")
	check(panel.tree.custom_minimum_size.y == 190, "roster Tree uses a constrained list height")
	check((panel.tree.size_flags_vertical & Control.SIZE_EXPAND) == 0, "roster Tree does not force the sidebar taller")
	select_country(ui, "BBB")
	check(panel.tree.get_root().get_child_count() == 1, "country selection filters out other countries' formations")
	if panel.tree.get_root().get_child_count() == 1:
		check(panel.tree.get_root().get_child(0).get_text(0).contains("Northern Guard"), "BBB's own army appears in its roster")
	select_country(ui, "AAA")
	panel.toggle_section()
	var army_item: TreeItem = panel.tree.get_root().get_child(0)
	var army_metadata: Dictionary = army_item.get_metadata(0)
	check(str(army_metadata.get("key", "")).begins_with("army:"), "army metadata keys include the formation category")
	army_item.set_collapsed(false)
	army_item.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("Eastern Army") and panel.detail_label.text.contains("当前驻地：East (2)"), "formation selection displays its current location")
	panel.location_button.pressed.emit()
	check(ui.selected_province_id == 2, "formation location button opens the current province")
	var named_regiment := army_item.get_first_child()
	named_regiment.select(0)
	panel.tree.item_selected.emit()
	var regiment_metadata: Dictionary = named_regiment.get_metadata(0)
	var named_key: String = str(regiment_metadata.get("key", ""))
	check(named_key.begins_with("regiment:"), "member metadata keys include the member category")
	check(panel.detail_label.text.contains("1st Infantry") and panel.detail_label.text.contains("infantry"), "selected regiment displays its raw name and type")
	check(panel.detail_label.text.contains("0.800") and panel.detail_label.text.contains("1.250"), "selected regiment displays strength and morale")
	check(panel.detail_label.text.contains("登记本籍省份（EU4编号）：1"), "regiment home province is displayed as its supplied number")
	panel.location_button.pressed.emit()
	check(ui.selected_province_id == 2 and ui.province_details.text.contains("East · 2"), "formation and member location buttons open the current province details")
	var unknown_regiment: TreeItem = army_item.get_child(1)
	unknown_regiment.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("兵力：未载入") and panel.detail_label.text.contains("士气：未载入"), "unknown regiment values remain unknown")
	check(panel.detail_label.text.contains("登记本籍省份（EU4编号）：未载入") and not panel.detail_label.text.contains("0.000"), "missing home province is not rendered as zero")
	var fleet_item: TreeItem = panel.tree.get_root().get_child(1)
	var fleet_metadata: Dictionary = fleet_item.get_metadata(0)
	check(str(fleet_metadata.get("key", "")).begins_with("fleet:"), "fleet metadata keys include the formation category")
	fleet_item.set_collapsed(false)
	var ship_item := fleet_item.get_first_child()
	ship_item.select(0)
	panel.tree.item_selected.emit()
	var ship_metadata: Dictionary = ship_item.get_metadata(0)
	var ship_key: String = str(ship_metadata.get("key", ""))
	check(ship_key.begins_with("ship:") and panel.detail_label.text.contains("North Star"), "fleet member uses its category key and raw name")
	check(panel.detail_label.text.contains("light_ship") and panel.detail_label.text.contains("1.050"), "ship details display type and morale")
	check(panel.detail_label.text.contains("登记本籍省份（EU4编号）：2"), "ship home province is displayed as its supplied number")
	panel.location_button.pressed.emit()
	check(ui.selected_province_id == 2, "ship location button opens its fleet's current province")
	named_regiment.select(0)
	panel.tree.item_selected.emit()
	var changed_report: Dictionary = aaa_report.value.duplicate(true)
	changed_report.armies[0].regiments[0].morale_milli = 1251
	panel.render_report(changed_report)
	var rebuilt_army: TreeItem = panel.tree.get_root().get_child(0)
	check(panel.selected_key == named_key and not rebuilt_army.is_collapsed(), "changed reports preserve a surviving selection and expanded formation")
	select_country(ui, "AAA")
	var selected_item: TreeItem = panel.tree.get_selected()
	check(ui.advance_world(1), "advance the world while the roster is selected")
	check(panel.tree.get_selected() == selected_item and panel.selected_key == named_key, "an unchanged roster refresh preserves its selected Tree item")
	var saved: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	check(saved.ok, "save the campaign with imported military units")
	if saved.ok:
		var snapshot_path := ProjectSettings.globalize_path("user://augustus-military-roster-%d.json" % OS.get_process_id())
		var file := FileAccess.open(snapshot_path, FileAccess.WRITE)
		if file != null:
			file.store_string(str(saved.value))
			file.close()
			check(ui.restore_world(snapshot_path), "restore the campaign with its military roster")
			var restored: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
			check(restored.ok and restored.value.armies.size() == 1 and restored.value.fleets.size() == 1, "save and restore keeps the imported army and fleet")
			check(ui.selected_country_id == "AAA" and panel.selected_key == named_key, "restore refreshes the current country and retains its selected roster row")
			DirAccess.remove_absolute(snapshot_path)
		else:
			check(false, "open a temporary campaign snapshot")
	if saved.ok:
		var source_state: Dictionary = JSON.parse_string(saved.value)
		var large_state := {
			"armies": integer_numbers(source_state.armies),
			"naval_units": integer_numbers(source_state.naval_units),
			"fleets": integer_numbers(source_state.fleets),
		}
		var round_trip: Dictionary = JSON.parse_string(ui.session.restore_snapshot(replace_military_sections(saved.value, large_state)))
		check(round_trip.ok, "prepare the integer-valued test snapshot: %s" % round_trip.get("error", ""))
		check(JSON.parse_string(ui.session.save_snapshot()).value == saved.value, "test preparation preserves every original snapshot value")
		var first_id: int = 9007199254740992
		var next_regiment: int = first_id
		for index in large_state.armies.armies.size():
			var army: Dictionary = large_state.armies.armies[index]
			army.id = first_id + index
			army.owner = large_state.armies.armies[0].owner
			for regiment in army.regiments:
				regiment.id = next_regiment
				next_regiment += 1
		large_state.armies.next_army_id = first_id + large_state.armies.armies.size()
		large_state.armies.next_regiment_id = next_regiment
		large_state.fleets.fleets[0].id = first_id
		large_state.fleets.next_id = first_id + 1
		large_state.naval_units.units[0].id = first_id
		large_state.naval_units.next_id = first_id + 1
		large_state.fleets.fleets[0].ships[0] = first_id
		var large_restored: Dictionary = JSON.parse_string(ui.session.restore_snapshot(replace_military_sections(saved.value, large_state)))
		check(large_restored.ok, "restore valid military IDs above the exact JSON floating-point integer range: %s" % large_restored.get("error", ""))
		if large_restored.ok:
			var large_report: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
			check(large_report.value.armies[0].id == "9007199254740992" and large_report.value.armies[1].id == "9007199254740993", "adjacent large army IDs remain distinct")
			check(large_report.value.armies[0].regiments[1].id == "9007199254740993", "large regiment IDs remain exact")
			check(large_report.value.fleets[0].id == "9007199254740992" and large_report.value.fleets[0].ships[0].id == "9007199254740992", "large fleet and ship IDs remain exact")
			select_country(ui, "AAA")
			var second_army: TreeItem = panel.tree.get_root().get_child(1)
			second_army.select(0)
			panel.tree.item_selected.emit()
			check(panel.selected_key == "army:9007199254740993", "Tree selection uses the full stable identifier")
	var event_mod := write_reinforcement_event()
	if not event_mod.is_empty():
		ui.mod_directories.append(event_mod)
		check(ui.load_world(), "load the self-authored reinforcement event")
		select_country(ui, "AAA")
		var old_army: TreeItem = panel.tree.get_root().get_child(0)
		old_army.set_collapsed(false)
		old_army.get_first_child().select(0)
		panel.tree.item_selected.emit()
		var retained_key: String = panel.selected_key
		var selected_template: Dictionary = JSON.parse_string(ui.session.select_unit_template("AAA", "infantry", "test_infantry"))
		check(selected_template.ok, "select the available reinforcement template")
		check(ui.event_overlay.visible and not ui.event_option_buttons.is_empty(), "reinforcement waits for the event option")
		if not ui.event_option_buttons.is_empty():
			ui.event_option_buttons[0].pressed.emit()
			var reinforced: Dictionary = JSON.parse_string(ui.session.query_military("AAA"))
			check(reinforced.ok and reinforced.value.armies.size() == 2, "the real event effect creates a second current army")
			check(panel.tree.get_root().get_child_count() == 3, "choosing an event option refreshes the live roster")
			check(panel.selected_key == retained_key and not panel.tree.get_root().get_child(0).is_collapsed(), "event refresh retains a surviving member selection and expanded formation")
		ui.mod_directories.clear()
		remove_reinforcement_event(event_mod)
	ui.external_state.text = ""
	check(ui.load_world(), "load a new world without translated military data")
	await process_frame
	await process_frame
	check(panel.selected_key.is_empty() and panel.tree.get_root().get_child_count() == 0, "loading a new world clears the previous roster selection")
	check(panel.report_status.text == "暂无已载入编队", "an empty imported roster is described without claiming complete knowledge")
	check_layout(ui)
	DirAccess.remove_absolute(external_state_path)
	ui.queue_free()
	await process_frame
	finish()

func write_military_state() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-military-roster-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()])
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	var state := {
		"armies": [
			{
				"owner": "AAA",
				"location": 2,
				"name": "Eastern Army",
				"regiments": ["test_infantry", "test_cavalry"],
				"regiment_states": [
					{"name": "1st Infantry", "strength_milli": 800, "morale_milli": 1250, "home_province": 1},
					{},
				],
			},
			{
				"owner": "BBB",
				"location": 1,
				"name": "Northern Guard",
				"regiments": ["test_infantry"],
			},
		],
		"naval_fleets": [
			{
				"owner": "AAA",
				"location": 2,
				"name": "Eastern Fleet",
				"ship_types": ["sample_light_ship"],
				"ship_states": [{"name": "North Star", "morale_milli": 1050, "home_province": 2}],
			},
		],
	}
	file.store_string(JSON.stringify(state))
	var error := file.get_error()
	file.close()
	if error != OK:
		DirAccess.remove_absolute(path)
		return ""
	return path

func integer_numbers(value: Variant) -> Variant:
	if value is float and value == floor(value):
		return int(value)
	if value is Dictionary:
		for key in value:
			value[key] = integer_numbers(value[key])
	elif value is Array:
		for index in value.size():
			value[index] = integer_numbers(value[index])
	return value

func write_reinforcement_event() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-military-event-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()])
	var files := {
		"descriptor.mod": 'name="Augustus military event test"\nsupported_version="1.37.*"\n',
		"common/on_actions/military.txt": 'on_startup={if={limit={tag=AAA} country_event={id=military_roster.test}}}\n',
		"events/military.txt": 'country_event={id=military_roster.test is_triggered_only=yes option={name=reinforce infantry=2}}\n',
	}
	for relative in files:
		var target: String = path.path_join(relative)
		if DirAccess.make_dir_recursive_absolute(target.get_base_dir()) != OK:
			check(false, "create reinforcement event fixture directory")
			return ""
		var file := FileAccess.open(target, FileAccess.WRITE)
		if file == null:
			check(false, "write reinforcement event fixture")
			return ""
		file.store_string(files[relative])
		file.close()
	return path

func remove_reinforcement_event(path: String) -> void:
	for relative in ["descriptor.mod", "common/on_actions/military.txt", "events/military.txt", "common/on_actions", "common", "events"]:
		DirAccess.remove_absolute(path.path_join(relative))
	DirAccess.remove_absolute(path)

# Only edit the three military blocks in this self-authored pretty snapshot.
# Other blocks include integer bounds that must retain their original text.
func replace_military_sections(snapshot: String, state: Dictionary) -> String:
	for key in ["armies", "naval_units", "fleets"]:
		var pattern := RegEx.new()
		pattern.compile('(?ms)^  "%s": \\{.*?^  \\}' % key)
		var found := pattern.search(snapshot)
		check(found != null, "find the fixture's military snapshot block: " + key)
		if found == null:
			return ""
		var replacement := '  "%s": %s' % [key, JSON.stringify(state[key])]
		snapshot = snapshot.substr(0, found.get_start()) + replacement + snapshot.substr(found.get_end())
	return snapshot

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.country_choice.select(index)
			ui.select_country(index, false)
			return
	check(false, "find country %s in the selector" % tag)

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
	print("Military roster integration: %d failures" % failures)
	quit(1 if failures else 0)

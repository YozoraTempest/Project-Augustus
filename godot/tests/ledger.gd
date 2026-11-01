# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")
var external_fixture := ProjectSettings.globalize_path("res://../examples/ledger/external_state.json")

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
	ui.game_path.text = fixture
	ui.external_state.text = external_fixture
	var event_mod := write_monthly_ledger_event()
	check(not event_mod.is_empty(), "create a self-authored monthly balance event")
	if event_mod.is_empty():
		await finish(ui)
		return
	ui.mod_directories.append(event_mod)
	var loaded: bool = ui.load_world()
	check(loaded, "load the ledger fixture: %s" % ui.status.text)
	await process_frame
	await process_frame
	if not loaded:
		remove_monthly_ledger_event(event_mod)
		await finish(ui)
		return

	select_country(ui, "AAA")
	var panel = ui.ledger_panel
	var before_queries: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var packet: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(packet.ok, "query AAA's current country ledger")
	if not packet.ok:
		remove_monthly_ledger_event(event_mod)
		await finish(ui)
		return
	var aaa: Dictionary = packet.value
	check(aaa.country == "AAA" and aaa.day == "0", "ledger query reports AAA and exact world day")
	check(is_integral_number(aaa.date.year) and int(aaa.date.year) == 1444, "ledger date accepts an integer-valued parsed number")
	check(is_integral_number(aaa.date.month) and int(aaa.date.month) == 11, "ledger date includes the starting month")
	check(is_integral_number(aaa.date.day) and int(aaa.date.day) == 11, "ledger date includes the starting day")
	check(aaa.ledger == null, "a country with no saved ledger stays distinct from an explicit empty ledger")
	check(aaa.income_keys is Array and aaa.income_keys.size() == 19, "query exposes all 19 native income keys")
	check(aaa.expense_keys is Array and aaa.expense_keys.size() == 38, "query exposes all 38 native expense keys")
	check(aaa.income_keys[0] == "INCOMETAX" and aaa.expense_keys[8] == "EXPENSEFORTM", "native key arrays retain their slot order")
	check(panel.selected_key.is_empty() and not panel.view_choice.visible, "no-ledger report has no selected category or view control")
	check(panel.section_button.text.contains("暂无记录"), "ledger header distinguishes no saved ledger from loaded zero values")
	var economy_before: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
	check(economy_before.ok and economy_before.value.treasury_milli == "10000", "AAA starts with the exact 10.000 treasury")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_queries.value, "ledger queries do not change the snapshot")
	var unknown: Dictionary = JSON.parse_string(ui.session.query_country_ledger("ZZZ"))
	check(not unknown.ok, "unknown ledger country returns an error")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_queries.value, "failed ledger query leaves the snapshot unchanged")
	var bbb: Dictionary = JSON.parse_string(ui.session.query_country_ledger("BBB"))
	check(bbb.ok and bbb.value.ledger == null, "BBB with unresolved finance inputs also has no ledger record")
	var bbb_economy: Dictionary = JSON.parse_string(ui.session.query_economy("BBB"))
	check(bbb_economy.ok and bbb_economy.value.treasury_milli == null, "BBB's unknown treasury stays null")

	check(ui.advance_world(20), "advance through the first monthly settlement")
	var settled_economy: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
	check(settled_economy.ok, "query AAA after monthly settlement")
	if settled_economy.ok:
		var settled: Dictionary = settled_economy.value
		check(settled.day == "20" and settled.date.month == 12 and settled.date.day == 1, "first settlement reaches 1444.12.01")
		check(settled.tax.last_receipt.amount_milli == "1000", "fixture records 1.000 tax")
		check(settled.production.last_receipt.amount_milli == "500", "fixture records 0.500 production")
		check(settled.cashflow.last_receipt.amount_milli == "250", "fixture records 0.500 trade less 0.250 expense")
		check(settled.treasury_milli == "11750", "first month leaves AAA with 11.750")
	var after_settlement_ledger: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	var settled_ledger_value: Variant = null
	check(after_settlement_ledger.ok and after_settlement_ledger.value.ledger is Dictionary, "monthly economy records AAA's country ledger")
	if after_settlement_ledger.ok and after_settlement_ledger.value.ledger is Dictionary:
		var live_ledger: Dictionary = after_settlement_ledger.value.ledger
		settled_ledger_value = live_ledger
		check(live_ledger.pending_milli == "0", "settled ledger has no pending balance")
		check(live_ledger.categories["0"].income_milli == "1000", "ledger records tax in income slot 0")
		check(live_ledger.categories["1"].income_milli == "500", "ledger records ordinary production in income slot 1")
		check(live_ledger.categories["2"].income_milli == "500", "ledger records trade in income slot 2")
		check(live_ledger.categories["3"].income_milli == "0", "ledger preserves known zero gold in income slot 3")
		check(live_ledger.categories["8"].expense_milli == "250", "ledger classifies fort maintenance in expense slot 8")
		check(live_ledger.income_sources_milli.size() == 19 and live_ledger.income_sources_milli[2] == "500", "income source vector keeps its native 19-slot layout")
		check(live_ledger.expense_sources_milli.size() == 38 and live_ledger.expense_sources_milli[8] == "250", "expense source vector keeps its native 38-slot layout")
		check(live_ledger.income_accumulator_milli[0] == "1000" and live_ledger.income_accumulator_milli[2] == "500", "annual income accumulator preserves settled sources")
		check(live_ledger.expense_accumulator_milli[8] == "250", "annual expense accumulator preserves the category")
		check(live_ledger.income_snapshot_milli.size() == 19 and live_ledger.income_rollover_milli.size() == 19, "snapshot and rollover vectors retain the income layout")
		check(live_ledger.expense_snapshot_milli.size() == 38 and live_ledger.expense_rollover_milli.size() == 38, "snapshot and rollover vectors retain the expense layout")
		check(live_ledger.income_snapshot_milli[2] == "0" and live_ledger.income_rollover_milli[2] == "0", "full zero snapshots do not imply a consolidation or rollover date")
	check(panel.view_choice.visible and panel.tree.get_root().get_child_count() == 2, "settled report renders both ledger groups")
	var live_income_tax := ledger_item(panel, "income:0")
	check(live_income_tax != null and live_income_tax.get_text(0).contains("1.000"), "live category row formats tax in thousandths")
	select_ledger_item(panel, "income:0")
	panel.tree.get_root().get_child(0).set_collapsed(false)
	check(ui.event_overlay.visible and not ui.event_option_buttons.is_empty(), "monthly pulse presents the self-authored balance event")
	if ui.event_overlay.visible and not ui.event_option_buttons.is_empty():
		ui.event_option_buttons[0].pressed.emit()
	var after_event: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
	check(after_event.ok and after_event.value.treasury_milli == "12750", "event changes the current balance by 1.000")
	var ledger_after_event: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(ledger_after_event.ok and ledger_after_event.value.ledger == settled_ledger_value, "event balance change leaves the saved ledger source unchanged")
	check(panel.selected_key == "income:0", "event refresh keeps the selected ledger category")
	check(not panel.tree.get_root().get_child(0).is_collapsed(), "event refresh keeps expanded ledger groups")
	check(JSON.parse_string(ui.session.save_snapshot()).value != before_queries.value, "settlement and event update the campaign snapshot")

	var snapshot_path := ProjectSettings.globalize_path("user://augustus-ledger-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()])
	check(ui.save_world(snapshot_path), "save the campaign after the balance event")
	check(ui.advance_world(5), "advance beyond the settlement date")
	check(ui.restore_world(snapshot_path), "restore the campaign snapshot")
	var restored_economy: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
	check(restored_economy.ok and restored_economy.value.day == "20", "restore recovers the saved ledger query date")
	check(restored_economy.ok and restored_economy.value.treasury_milli == "12750", "restore recovers the event-adjusted treasury")
	var restored_ledger: Dictionary = JSON.parse_string(ui.session.query_country_ledger("AAA"))
	check(restored_ledger.ok and restored_ledger.value.ledger == settled_ledger_value, "restore returns the same settled ledger source")
	check(panel.selected_key == "income:0", "restore refresh keeps the selected ledger category")
	DirAccess.remove_absolute(snapshot_path)

	verify_synthetic_reports(panel, aaa)
	var synthetic_snapshot: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var synthetic := make_report(make_full_zero_ledger(), aaa)
	panel.render_report(synthetic)
	select_ledger_item(panel, "income:8")
	select_country(ui, "BBB")
	check(panel.selected_key.is_empty(), "changing country clears the selected category")
	check(not panel.view_choice.visible and not panel.tree.visible, "country without a ledger hides prior rows and view choices")
	check(JSON.parse_string(ui.session.save_snapshot()).value == synthetic_snapshot.value, "switching ledger views and selecting a country never edits campaign state")

	var missing_external := ProjectSettings.globalize_path("user://missing-ledger-state.json")
	ui.external_state.text = missing_external
	var active_report := make_report(make_full_zero_ledger(), aaa)
	panel.render_report(active_report)
	select_ledger_item(panel, "income:8")
	var income_group: TreeItem = panel.tree.get_root().get_child(0)
	income_group.set_collapsed(false)
	var selected_detail: String = panel.detail_label.text
	check(not ui.load_world(), "failed candidate load is rejected")
	check(panel.selected_key == "income:8", "failed candidate load preserves the selected category")
	check(not panel.tree.get_root().get_child(0).is_collapsed(), "failed candidate load preserves expanded groups")
	check(panel.detail_label.text == selected_detail, "failed candidate load preserves the selected detail")
	check(JSON.parse_string(ui.session.query_economy("AAA")).value.treasury_milli == "12750", "failed candidate load preserves the active campaign")
	ui.external_state.text = external_fixture

	remove_monthly_ledger_event(event_mod)
	ui.mod_directories.clear()
	panel.render_report(make_report(make_full_zero_ledger(), aaa))
	select_ledger_item(panel, "income:8")
	ui.external_state.text = ""
	check(ui.load_world(), "load a new world without external ledger state")
	check(panel.selected_key.is_empty(), "loading a new world clears the prior category selection")
	check(not panel.tree.visible and not panel.view_choice.visible, "new world's missing ledger hides the previous report")
	check_layout(ui)
	await finish(ui)

func verify_synthetic_reports(panel: Control, source: Dictionary) -> void:
	var empty_report := make_report(make_empty_ledger(), source)
	panel.render_report(empty_report)
	check(not panel.section_expanded, "ledger section starts collapsed")
	check(panel.tree.get_root().get_child_count() == 2, "ledger tree has separate income and expense groups")
	var income_group: TreeItem = panel.tree.get_root().get_child(0)
	var expense_group: TreeItem = panel.tree.get_root().get_child(1)
	check(income_group.get_metadata(0).kind == "group" and income_group.get_child_count() == 19, "income group has 19 native slots")
	check(expense_group.get_metadata(0).kind == "group" and expense_group.get_child_count() == 38, "expense group has 38 native slots")
	panel.toggle_section()
	select_ledger_item(panel, "income:7")
	var income_seven := ledger_item(panel, "income:7")
	check(income_seven != null and income_seven.get_text(0).contains("收入分类 7"), "unmapped income slot uses its Chinese category number")
	check(income_seven != null and income_seven.get_tooltip_text(0).contains("native_key: " + str(source.income_keys[7])), "income tooltip retains its native key")
	check(panel.detail_label.text.contains("无分类记录"), "missing category entry is distinct from a recorded zero")
	set_ledger_view(panel, "sources")
	check(panel.selected_view == "sources" and panel.selected_key == "income:7", "view change emits the selected view and keeps the selected row")
	check(panel.detail_label.text.contains("未载入此向量"), "empty source vector reports that the vector is not loaded")
	set_ledger_view(panel, "auxiliary")
	check(panel.detail_label.text.contains("不适用"), "income rows mark the expense-only auxiliary vector as not applicable")
	check(panel.detail_label.text.contains("仅记录支出"), "income detail explains auxiliary values do not apply")
	set_ledger_view(panel, "snapshot")
	check(panel.report_status.text.contains("归集执行情况与日期未知"), "snapshot view does not invent an execution date")
	check(panel.detail_label.text.contains("全零向量也不能证明已执行"), "snapshot details explain that all-zero values do not establish execution")
	set_ledger_view(panel, "accumulator")
	check(panel.selected_view == "accumulator", "accumulator view selection is wired")
	set_ledger_view(panel, "rollover")
	check(panel.report_status.text.contains("结转执行情况与日期未知"), "rollover view does not invent an execution date")
	check(panel.detail_label.text.contains("全零向量也不能证明已执行"), "rollover details explain that all-zero values do not establish execution")

	var zeros := make_full_zero_ledger()
	var zero_report := make_report(zeros, source)
	zero_report.day = "1"
	zero_report.date = {"year": 1444, "month": 11, "day": 12}
	panel.render_report(zero_report)
	check(panel.selected_key == "income:7", "same-country report refresh keeps the selected category")
	check(not panel.tree.get_root().get_child(0).is_collapsed(), "same-country report refresh keeps expanded groups")
	set_ledger_view(panel, "sources")
	check(panel.detail_label.text.contains("0.000"), "full-length zero vector displays a known zero")
	check(panel.detail_label.text.contains("未载入此向量") == false, "full-length zero vector is not described as missing")
	set_ledger_view(panel, "categories")
	check(panel.detail_label.text.contains("无分类记录"), "a full zero vector does not create a missing category record")

	var synthetic_categories: Dictionary = zeros.categories
	synthetic_categories["8"] = {"income_milli": "800", "expense_milli": "80"}
	zeros.categories = synthetic_categories
	var income_sources: Array[String] = zeros.income_sources_milli
	income_sources[8] = "800"
	zeros.income_sources_milli = income_sources
	var expense_sources: Array[String] = zeros.expense_sources_milli
	expense_sources[8] = "80"
	zeros.expense_sources_milli = expense_sources
	var auxiliary: Array[String] = zeros.expense_auxiliary_milli
	auxiliary[8] = "250"
	zeros.expense_auxiliary_milli = auxiliary
	panel.render_report(make_report(zeros, source))
	set_ledger_view(panel, "categories")
	select_ledger_item(panel, "income:8")
	var income_eight := ledger_item(panel, "income:8")
	var income_metadata: Dictionary = income_eight.get_metadata(0)
	check(income_metadata.key == "income:8" and income_metadata.kind == "income" and income_metadata.category == 8, "income row metadata keeps its own kind and slot")
	check(income_metadata.native_key == str(source.income_keys[8]), "income row metadata retains its native key")
	check(income_eight.get_text(0).contains("收入分类 8") and income_eight.get_text(0).contains("0.800"), "income slot 8 shows its own recorded amount")
	select_ledger_item(panel, "expense:8")
	var expense_eight := ledger_item(panel, "expense:8")
	var expense_metadata: Dictionary = expense_eight.get_metadata(0)
	check(expense_metadata.key == "expense:8" and expense_metadata.kind == "expense" and expense_metadata.category == 8, "expense row metadata keeps the same numeric slot separate")
	check(expense_metadata.native_key == str(source.expense_keys[8]), "expense row metadata retains its native key")
	check(expense_eight.get_text(0).contains("要塞维护") and expense_eight.get_text(0).contains("0.080"), "expense slot 8 shows fort maintenance rather than income slot 8")
	check(expense_eight.get_tooltip_text(0).contains("native_key: " + str(source.expense_keys[8])), "expense tooltip exposes its own native key")
	set_ledger_view(panel, "auxiliary")
	check(panel.detail_label.text.contains("辅助支出记录：0.250（不额外再加支出）"), "auxiliary amount is clearly detail, not another expense")
	set_ledger_view(panel, "rollover")
	check(panel.report_status.text.contains("结转执行情况与日期未知"), "all-zero rollover vectors do not imply a recorded execution date")
	var expense_thirty_seven := ledger_item(panel, "expense:37")
	check(expense_thirty_seven != null and expense_thirty_seven.get_text(0).contains("支出分类 37"), "unmapped expense slot keeps its Chinese category number")
	check(expense_thirty_seven != null and expense_thirty_seven.get_tooltip_text(0).contains("native_key: " + str(source.expense_keys[37])), "last expense tooltip retains its native key")

	var high := make_full_zero_ledger()
	high.pending_milli = "-9223372036854775808"
	var high_income: Array[String] = high.income_sources_milli
	high_income[0] = "9223372036854775807"
	high.income_sources_milli = high_income
	var high_expense: Array[String] = high.expense_sources_milli
	high_expense[8] = "9223372036854775807"
	high.expense_sources_milli = high_expense
	var high_report := make_report(high, source)
	high_report.date = JSON.parse_string("{\"year\":1444.0,\"month\":12.0,\"day\":1.0}")
	panel.render_report(high_report)
	check(panel.report_status.text.contains("1444.12.01"), "integer-valued JSON floats render as the same calendar date")
	check(panel.report_status.text.contains("-9223372036854775.808"), "i64 minimum pending amount formats exactly")
	set_ledger_view(panel, "sources")
	select_ledger_item(panel, "income:0")
	check(panel.detail_label.text.contains("9223372036854775.807"), "i64 maximum income formats exactly")
	select_ledger_item(panel, "expense:8")
	check(panel.detail_label.text.contains("9223372036854775.807"), "i64 maximum expense formats exactly")
	panel.clear()
	check(panel.selected_key.is_empty() and not panel.tree.visible, "clear removes synthetic selection and rows")
	panel.render_report(empty_report)
	check(panel.tree.visible and panel.tree.get_root().get_child_count() == 2, "render_report restores the category tree after clear")

func make_empty_ledger() -> Dictionary:
	return {
		"pending_milli": "0",
		"categories": {},
		"income_sources_milli": [],
		"expense_sources_milli": [],
		"income_accumulator_milli": [],
		"expense_accumulator_milli": [],
		"expense_auxiliary_milli": [],
		"income_snapshot_milli": [],
		"expense_snapshot_milli": [],
		"income_rollover_milli": [],
		"expense_rollover_milli": [],
		"income_total_milli": "0",
		"expense_total_milli": "0",
		"income_filtered_total_milli": "0",
		"expense_filtered_total_milli": "0",
		"income_rollover_total_milli": "0",
		"expense_rollover_total_milli": "0",
	}

func make_full_zero_ledger() -> Dictionary:
	var ledger := make_empty_ledger()
	ledger.income_sources_milli = filled_values(19, "0")
	ledger.expense_sources_milli = filled_values(38, "0")
	ledger.income_accumulator_milli = filled_values(19, "0")
	ledger.expense_accumulator_milli = filled_values(38, "0")
	ledger.expense_auxiliary_milli = filled_values(38, "0")
	ledger.income_snapshot_milli = filled_values(19, "0")
	ledger.expense_snapshot_milli = filled_values(38, "0")
	ledger.income_rollover_milli = filled_values(19, "0")
	ledger.expense_rollover_milli = filled_values(38, "0")
	return ledger

func filled_values(count: int, value: String) -> Array[String]:
	var values: Array[String] = []
	for _index in range(count):
		values.append(value)
	return values

func make_report(ledger: Dictionary, source: Dictionary) -> Dictionary:
	return {
		"country": "AAA",
		"day": "0",
		"date": {"year": 1444, "month": 11, "day": 11},
		"ledger": ledger.duplicate(true),
		"income_keys": source.income_keys.duplicate(),
		"expense_keys": source.expense_keys.duplicate(),
	}

func ledger_item(panel: Control, key: String) -> TreeItem:
	var root_item: TreeItem = panel.tree.get_root()
	if root_item == null:
		return null
	for group_index in range(root_item.get_child_count()):
		var group: TreeItem = root_item.get_child(group_index)
		for row_index in range(group.get_child_count()):
			var row: TreeItem = group.get_child(row_index)
			var metadata: Variant = row.get_metadata(0)
			if metadata is Dictionary and metadata.get("key") == key:
				return row
	return null

func select_ledger_item(panel: Control, key: String) -> void:
	var item := ledger_item(panel, key)
	check(item != null, "find ledger row %s" % key)
	if item == null:
		return
	item.select(0)
	panel.tree.item_selected.emit()

func set_ledger_view(panel: Control, key: String) -> void:
	for index in panel.view_choice.item_count:
		if panel.view_choice.get_item_metadata(index) == key:
			panel.view_choice.select(index)
			panel.view_choice.item_selected.emit(index)
			return
	check(false, "find ledger view %s" % key)

func is_integral_number(value: Variant) -> bool:
	return value is int or (value is float and floor(value) == value)

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.country_choice.select(index)
			ui.select_country(index, false)
			return
	check(false, "find country %s in the selector" % tag)

func write_monthly_ledger_event() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-ledger-event-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()])
	var files := {
		"descriptor.mod": 'name="Augustus ledger event test"\nsupported_version="1.37.*"\n',
		"common/on_actions/monthly.txt": "on_monthly_pulse={events={ledger_refresh.test}}\n",
		"events/ledger.txt": 'country_event={id=ledger_refresh.test is_triggered_only=yes trigger={tag=AAA} option={name=ledger_refresh.test_a add_treasury=1}}\n',
	}
	for relative in files:
		var target: String = path.path_join(relative)
		if DirAccess.make_dir_recursive_absolute(target.get_base_dir()) != OK:
			check(false, "create monthly ledger event fixture directory")
			return ""
		var file := FileAccess.open(target, FileAccess.WRITE)
		if file == null:
			check(false, "write monthly ledger event fixture")
			return ""
		file.store_string(files[relative])
		file.close()
	return path

func remove_monthly_ledger_event(path: String) -> void:
	for relative in ["descriptor.mod", "common/on_actions/monthly.txt", "events/ledger.txt", "common/on_actions", "common", "events"]:
		DirAccess.remove_absolute(path.path_join(relative))
	DirAccess.remove_absolute(path)

func check_layout(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = node.get_global_rect()
		var ancestor := node.get_parent()
		while ancestor != null:
			if ancestor is Control and ancestor.clip_contents:
				rect = rect.intersection(ancestor.get_global_rect())
			ancestor = ancestor.get_parent()
		var viewport: Rect2 = root.get_visible_rect().grow(1)
		if rect.has_area():
			check(viewport.encloses(rect), "control outside viewport: %s %s" % [node.name, rect])
	for child in node.get_children():
		check_layout(child)

func finish(ui: Control) -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	await process_frame
	print("Augustus Godot ledger integration: %d failures" % failures)
	quit(1 if failures else 0)

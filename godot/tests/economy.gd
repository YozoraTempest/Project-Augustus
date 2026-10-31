# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var fixture := ProjectSettings.globalize_path("res://../examples/world")
var external_fixture := ProjectSettings.globalize_path("res://../examples/economy/external_state.json")

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	var ui = load("res://world.tscn").instantiate()
	root.add_child.call_deferred(ui)
	verify_ui.call_deferred(ui)

func verify_ui(ui: Control) -> void:
	await process_frame
	ui.game_path.text = fixture
	ui.external_state.text = external_fixture
	var event_mod := write_monthly_economy_event()
	check(not event_mod.is_empty(), "create a self-authored monthly economy event")
	if event_mod.is_empty():
		await finish(ui)
		return
	ui.mod_directories.append(event_mod)
	var loaded: bool = ui.load_world()
	check(loaded, "load the shared economy fixture: %s" % ui.status.text)
	await process_frame
	await process_frame
	if not loaded:
		remove_monthly_economy_event(event_mod)
		await finish(ui)
		return

	select_country(ui, "AAA")
	var panel = ui.economy_panel
	var before_queries: Dictionary = JSON.parse_string(ui.session.save_snapshot())
	var aaa_packet: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
	check(aaa_packet.ok, "query AAA's economy report")
	if not aaa_packet.ok:
		remove_monthly_economy_event(event_mod)
		await finish(ui)
		return
	var aaa: Dictionary = aaa_packet.value
	check(has_exact_finance_types(aaa), "all finance amounts and counters use exact strings or null")
	check(aaa.country == "AAA" and aaa.day == "0", "query reports the country and exact day string")
	check(aaa.treasury_milli == "10000", "Godot receives the starting treasury as a decimal string")
	check(aaa.date.year == 1444 and aaa.date.month == 11 and aaa.date.day == 11, "query includes the current calendar date")
	check(aaa.tax.settlement_count == "0" and aaa.tax.last_receipt == null, "installed tax stage without a country receipt stays empty")
	check(aaa.production.settlement_count == "0" and aaa.production.last_receipt == null, "installed production stage without a receipt stays empty")
	check(aaa.cashflow.settlement_count == "0" and aaa.cashflow.last_receipt == null, "installed cashflow stage without a receipt stays empty")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_queries.value, "economy queries do not change the snapshot or random stream")
	var unknown: Dictionary = JSON.parse_string(ui.session.query_economy("ZZZ"))
	check(not unknown.ok, "unknown economy country returns an error")
	check(JSON.parse_string(ui.session.save_snapshot()).value == before_queries.value, "failed economy query leaves the snapshot unchanged")
	check(not panel.section_expanded, "economy section starts collapsed")
	check(panel.tree.get_root().get_child_count() == 3, "economy history has three stage rows")
	var stage_keys := ["tax", "production", "cashflow"]
	for index in stage_keys.size():
		var item: TreeItem = panel.tree.get_root().get_child(index)
		var metadata: Variant = item.get_metadata(0)
		check(metadata is Dictionary and metadata.get("key") == stage_keys[index], "stage row uses its stable key")
		check(item.is_collapsed(), "stage history row starts collapsed")
	check(panel.selected_key.is_empty(), "new report starts without a selected stage")

	panel.toggle_section()
	var tax_item: TreeItem = panel.tree.get_root().get_child(0)
	tax_item.select(0)
	panel.tree.item_selected.emit()
	check(panel.selected_key == "tax" and panel.detail_label.text.contains("税收"), "selecting a stage shows its details")
	check(ui.advance_world(19), "advance to the day before the monthly boundary")
	check(panel.selected_key == "tax", "same-country refresh keeps the selected stage")
	var before_month: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	check(before_month.tax.last_receipt == null, "no monthly receipt appears before the boundary")
	check(ui.advance_world(1), "advance through the monthly boundary")
	check(panel.selected_key == "tax", "monthly settlement refresh keeps the selected stage")
	check(ui.event_overlay.visible and not ui.event_option_buttons.is_empty(), "monthly pulse presents the test event choice")
	var after_month: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	check(has_exact_finance_types(after_month), "settled finance report keeps exact strings for every amount and counter")
	check(after_month.day == "20" and after_month.date.month == 12 and after_month.date.day == 1, "monthly report uses the boundary day and date")
	check(after_month.tax.last_receipt.day == "20" and after_month.tax.last_receipt.date.day == 1, "receipt keeps its settlement date")
	check(after_month.tax.last_receipt.amount_milli == "1000", "tax receipt amount remains an exact string")
	check(panel.tree.get_root().get_child(0).get_child_count() == 1, "a completed receipt creates an expandable history row")
	panel.tree.get_root().get_child(0).set_collapsed(false)
	check(after_month.production.last_receipt.amount_milli == "500", "production receipt amount remains an exact string")
	check(after_month.cashflow.last_receipt.amount_milli == "-250", "signed cashflow remains an exact string")
	var historical_cashflow_treasury: String = after_month.cashflow.last_receipt.treasury_after_milli
	ui.event_option_buttons[0].pressed.emit()
	var after_event: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	check(after_event.treasury_milli != historical_cashflow_treasury, "event choice refreshes the current treasury")
	check(after_event.cashflow.last_receipt.treasury_after_milli == historical_cashflow_treasury, "event refresh leaves historical cashflow balance intact")
	check(panel.selected_key == "tax", "event refresh keeps the selected stage")
	check(not panel.tree.get_root().get_child(0).is_collapsed(), "event refresh keeps an expanded history row")
	check(JSON.parse_string(ui.session.save_snapshot()).value != before_queries.value, "advancement and event choice update the campaign snapshot")

	var bbb: Dictionary = JSON.parse_string(ui.session.query_economy("BBB")).value
	check(bbb.treasury_milli == null, "unknown starting treasury remains null")
	var bbb_production: Dictionary = bbb.production.last_receipt
	check(bbb_production.known_subtotal_milli == "0" and bbb_production.amount_milli == null, "known zero subtotal stays distinct from unknown production")
	check(bbb.cashflow.last_receipt.known_subtotal_milli == null and bbb.cashflow.last_receipt.amount_milli == "0", "zero cashflow stays distinct from its absent subtotal")
	select_country(ui, "BBB")
	check(panel.selected_key.is_empty(), "changing country clears the selected stage")
	var production_item: TreeItem = panel.tree.get_root().get_child(1)
	production_item.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("已知月产值：0.000") and panel.detail_label.text.contains("完整产值：未载入"), "stage details display zero and unknown values differently")
	check(panel.detail_label.text.contains("生产输入存在缺项"), "missing production is described in visible details")
	check(panel.detail_label.tooltip_text.contains("self_authored_missing_production"), "tooltip retains the source diagnostic")
	var tax_bbb: TreeItem = panel.tree.get_root().get_child(0)
	tax_bbb.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("省份年税收输入尚未确定"), "partial tax identifies the unresolved province input")
	check(panel.detail_label.tooltip_text.contains("unknown_external_annual_tax"), "tax tooltip retains the unresolved source identifier")
	var cashflow_bbb: TreeItem = panel.tree.get_root().get_child(2)
	cashflow_bbb.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("现金流净额：0.000"), "zero cashflow is displayed as a known amount")
	check(panel.detail_label.text.contains("国库余额尚未确定"), "unknown treasury is described in visible details")
	check(not panel.detail_label.tooltip_text.is_empty(), "cashflow tooltip keeps diagnostic detail")

	select_country(ui, "AAA")
	var cashflow_aaa: TreeItem = panel.tree.get_root().get_child(2)
	cashflow_aaa.set_collapsed(false)
	cashflow_aaa.select(0)
	panel.tree.item_selected.emit()
	var snapshot_path := ProjectSettings.globalize_path("user://augustus-economy-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()])
	check(ui.save_world(snapshot_path), "save the campaign after the event choice")
	check(ui.advance_world(5), "advance beyond the settlement date")
	var later: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	check(later.day == "25" and later.cashflow.last_receipt.day == "20", "later days do not rewrite the prior monthly receipt")
	check(panel.selected_key == "cashflow", "same-country advance keeps the selected cashflow stage")
	check(not panel.tree.get_root().get_child(2).is_collapsed(), "same-country advance keeps the expanded history")
	check(ui.restore_world(snapshot_path), "restore the campaign snapshot")
	var restored: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	check(restored.day == "20" and restored.treasury_milli == after_event.treasury_milli, "restore recovers the saved date and current balance")
	check(restored.cashflow.last_receipt == after_event.cashflow.last_receipt, "restore keeps the original historical receipt")
	check(panel.selected_key == "cashflow", "restore refresh keeps the selected stage")
	DirAccess.remove_absolute(snapshot_path)

	var before_failed_load: Dictionary = JSON.parse_string(ui.session.query_economy("AAA")).value
	var missing_external := ProjectSettings.globalize_path("user://missing-economy-state.json")
	ui.external_state.text = missing_external
	check(not ui.load_world(), "failed candidate load is rejected")
	check(panel.selected_key == "cashflow", "failed candidate load does not clear selected stage")
	check(JSON.parse_string(ui.session.query_economy("AAA")).value == before_failed_load, "failed candidate load preserves the active economy report")
	ui.external_state.text = external_fixture

	verify_extreme_formatting(panel)
	var synthetic_item: TreeItem = panel.tree.get_root().get_child(0)
	synthetic_item.select(0)
	panel.tree.item_selected.emit()
	check(panel.selected_key == "tax", "synthetic report selection works before world replacement")
	ui.mod_directories.clear()
	ui.external_state.text = ""
	check(ui.load_world(), "load a new world without the external economy bundle")
	check(panel.selected_key.is_empty(), "new world clears the prior stage selection")
	check(panel.tree.get_root().get_child_count() == 3, "new world rebuilds the stable economy stages")
	check(panel.tree.get_root().get_child(1).get_text(0).contains("未安装"), "new world reports the uninstalled production stage")
	check_layout(ui)
	remove_monthly_economy_event(event_mod)

	var extreme_state_path := write_extreme_cashflow_state()
	check(not extreme_state_path.is_empty(), "write exact cashflow input without JSON number round-tripping")
	if not extreme_state_path.is_empty():
		ui.external_state.text = extreme_state_path
		var extreme_loaded: bool = ui.load_world()
		check(extreme_loaded, "load exact cashflow input through the real world session: %s" % ui.status.text)
		if extreme_loaded:
			await process_frame
			await process_frame
			select_country(ui, "AAA")
			var advanced: bool = ui.advance_world(20)
			check(advanced, "settle the exact cashflow input: %s" % ui.status.text)
			var extreme_packet: Dictionary = JSON.parse_string(ui.session.query_economy("AAA"))
			check(extreme_packet.ok, "query the exact cashflow receipt")
			if advanced and extreme_packet.ok:
				var exact_cashflow := "9007199254740993"
				check(extreme_packet.value.cashflow.last_receipt.amount_milli == exact_cashflow, "Godot preserves a reachable amount above the exact JSON float range")
				check(extreme_packet.value.cashflow.last_receipt.credited_milli == exact_cashflow, "Godot preserves the exact large confirmed adjustment")
		DirAccess.remove_absolute(extreme_state_path)
	await finish(ui)

func verify_extreme_formatting(panel: Control) -> void:
	var maximum := "9223372036854775807"
	var minimum := "-9223372036854775808"
	var huge_count := "18446744073709551615"
	var report := {
		"country": "AAA",
		"day": huge_count,
		"date": {"year": 1444, "month": 12, "day": 1},
		"treasury_milli": maximum,
		"ledger_pending_milli": minimum,
		"tax": {
			"settlement_count": huge_count,
			"last_receipt": {
				"day": huge_count,
				"date": {"year": 1444, "month": 12, "day": 1},
				"known_subtotal_milli": maximum,
				"amount_milli": minimum,
				"credited_milli": "0",
				"treasury_before_milli": null,
				"treasury_after_milli": maximum,
				"unresolved": [],
			},
		},
		"production": null,
		"cashflow": null,
	}
	panel.render_report(report)
	check(panel.section_button.tooltip_text.contains("9223372036854775.807"), "header tooltip preserves i64 maximum")
	check(panel.report_status.text.contains("-9223372036854775.808"), "status displays i64 minimum")
	var item: TreeItem = panel.tree.get_root().get_child(0)
	item.select(0)
	panel.tree.item_selected.emit()
	check(panel.detail_label.text.contains("全局月结次数：18446744073709551615"), "stage count stays exact above the JSON-safe range")
	check(panel.detail_label.text.contains("第 18446744073709551615 天"), "receipt day remains an exact string")
	check(panel.detail_label.text.contains("已知月税收：9223372036854775.807"), "known subtotal preserves i64 maximum")
	check(panel.detail_label.text.contains("完整税收：-9223372036854775.808"), "negative amount preserves i64 minimum")
	check(panel.detail_label.text.contains("阶段确认入账：0.000"), "zero credit remains visible")
	check(panel.detail_label.text.contains("结算前：未载入"), "null balance remains distinct from zero")

func has_exact_finance_types(report: Dictionary) -> bool:
	if not report.get("day") is String:
		return false
	var date: Variant = report.get("date")
	if not date is Dictionary or not is_integral_number(date.get("year")) or not is_integral_number(date.get("month")) or not is_integral_number(date.get("day")):
		return false
	for field in ["treasury_milli", "ledger_pending_milli"]:
		var amount: Variant = report.get(field)
		if amount != null and not amount is String:
			return false
	for name in ["tax", "production", "cashflow"]:
		var stage: Variant = report.get(name)
		if stage == null:
			continue
		if not stage is Dictionary or not stage.get("settlement_count") is String:
			return false
		var receipt: Variant = stage.get("last_receipt")
		if receipt == null:
			continue
		if not receipt is Dictionary or not receipt.get("day") is String:
			return false
		var receipt_date: Variant = receipt.get("date")
		if not receipt_date is Dictionary or not is_integral_number(receipt_date.get("year")) or not is_integral_number(receipt_date.get("month")) or not is_integral_number(receipt_date.get("day")):
			return false
		for field in ["known_subtotal_milli", "amount_milli", "credited_milli", "treasury_before_milli", "treasury_after_milli"]:
			var amount: Variant = receipt.get(field)
			if amount != null and not amount is String:
				return false
	return true

func is_integral_number(value: Variant) -> bool:
	return value is int or (value is float and floor(value) == value)

func select_country(ui: Control, tag: String) -> void:
	for index in ui.country_choice.item_count:
		if ui.country_choice.get_item_metadata(index) == tag:
			ui.country_choice.select(index)
			ui.select_country(index, false)
			return
	check(false, "find country %s in the selector" % tag)

func write_monthly_economy_event() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-economy-event-%d-%d" % [OS.get_process_id(), Time.get_ticks_usec()])
	var files := {
		"descriptor.mod": 'name="Augustus economy event test"\nsupported_version="1.37.*"\n',
		"common/on_actions/monthly.txt": "on_monthly_pulse={events={economy_refresh.test}}\n",
		"events/economy.txt": 'country_event={id=economy_refresh.test is_triggered_only=yes trigger={tag=AAA} option={name=economy_refresh.test_a add_treasury=1}}\n',
	}
	for relative in files:
		var target: String = path.path_join(relative)
		if DirAccess.make_dir_recursive_absolute(target.get_base_dir()) != OK:
			check(false, "create monthly economy event fixture directory")
			return ""
		var file := FileAccess.open(target, FileAccess.WRITE)
		if file == null:
			check(false, "write monthly economy event fixture")
			return ""
		file.store_string(files[relative])
		file.close()
	return path

func remove_monthly_economy_event(path: String) -> void:
	for relative in ["descriptor.mod", "common/on_actions/monthly.txt", "events/economy.txt", "common/on_actions", "common", "events"]:
		DirAccess.remove_absolute(path.path_join(relative))
	DirAccess.remove_absolute(path)

func write_extreme_cashflow_state() -> String:
	var path := ProjectSettings.globalize_path("user://augustus-economy-i64-%d-%d.json" % [OS.get_process_id(), Time.get_ticks_usec()])
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	file.store_string("{\"country_resources\":{\"AAA\":{\"treasury_milli\":0}},\"country_annual_tax_milli\":{\"AAA\":0},\"province_annual_tax_milli\":{\"2\":0},\"country_monthly_cashflows_milli\":{\"AAA\":9007199254740993}}")
	var error := file.get_error()
	file.close()
	if error != OK:
		DirAccess.remove_absolute(path)
		return ""
	return path

func check_layout(node: Node) -> void:
	if node is Control and node.is_visible_in_tree():
		var rect: Rect2 = node.get_global_rect()
		var ancestor := node.get_parent()
		while ancestor != null:
			if ancestor is Control and ancestor.clip_contents:
				rect = rect.intersection(ancestor.get_global_rect())
			ancestor = ancestor.get_parent()
		var viewport: Rect2 = root.get_visible_rect().grow(1)
		check(viewport.encloses(rect), "control outside viewport: %s %s" % [node.name, rect])
	for child in node.get_children():
		check_layout(child)

func finish(ui: Control) -> void:
	if is_instance_valid(ui):
		ui.queue_free()
	await process_frame
	print("Augustus Godot economy integration: %d failures" % failures)
	quit(1 if failures else 0)

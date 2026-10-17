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
	var session := AugustusWorldSession.new()
	var no_mods: Array[String] = []
	check(not JSON.parse_string(session.query_country("AAA")).ok, "queries require a loaded world")
	check(not JSON.parse_string(session.advance(1)).ok, "advancement requires a loaded world")
	check(JSON.parse_string(session.load_world_with_content(fixture, "1444.11.11", no_mods, "", "")).ok, "load fixture through shared installation resolver")
	check(JSON.parse_string(session.load_world(fixture, "1444.11.11")).ok, "load independent fixture")
	var alpha: Dictionary = JSON.parse_string(session.query_country("AAA"))
	check(alpha.value.bound_capital == 2, "country query uses core capital relation")
	check(alpha.value.owned_provinces.size() == 1 and int(alpha.value.owned_provinces[0]) == 2, "country query uses actual ownership")
	check(alpha.value.politics.ruler.name == "Iris", "ruler query reads authoritative state")
	check(alpha.value.politics.national_focus.skill == null, "known empty national focus reaches the query")
	check(alpha.value.attributes.monthly_adm.value_micros == null and alpha.value.attributes.monthly_adm.unresolved.has("power_income:advisors_not_initialized"), "partial monarch income remains unknown")
	check(alpha.value.politics.heir.claim_milli == 87125, "heir claim preserves precision")
	check(alpha.value.diplomacy.size() == 5, "diplomatic relationships reach the UI query")
	check(alpha.value.technology.administrative == 3 and alpha.value.technology.military == 3, "baseline technology reaches the country query")
	check(alpha.value.government.reforms == ["test_charter", "test_court"] and alpha.value.government.metrics.legitimacy == 75125, "selected reforms and legitimacy reach the country query")
	check(alpha.value.ideas.groups.test_alpha == 0 and alpha.value.declared_idea_effects.known_totals_micros.test_tradition == 125000, "national traditions apply without unlocking future ideas")
	check(alpha.value.resources.mercantilism_milli == 18500 and alpha.value.resources.treasury_milli == null, "declared resources preserve precision and unknown stocks")
	var north: Dictionary = JSON.parse_string(session.query_province(1))
	check(north.value.owner == "BBB" and north.value.tax_milli == 2125, "dated history reaches the Rust core")
	check(north.value.buildings == ["test_fort"], "existing buildings reach the province query")
	check(north.value.trade_good_definition.base_price_milli == 2125, "base price preserves precision")
	check(north.value.trade_node == "river" and north.value.trade_node_outgoing == ["harbor"], "province query follows the trade graph")
	check(north.value.modifiers == ["test_harbor", "test_river"] and north.value.declared_modifier_totals_micros.test_growth == -2500, "permanent modifier bindings retain signed precision")
	check(north.value.attributes.tax_basis.known_subtotal_micros == 212000 and north.value.attributes.tax_basis.value_micros == null, "attribute query separates known calculation from complete result")
	check(north.value.attributes.manpower_contribution.known_subtotal_micros == 831250000, "manpower calculation includes trade good and culture effects")
	check(alpha.value.province_attribute_totals.tax_basis.known_subtotal_micros == 1100000, "country aggregation includes buildings and trade goods")
	check(not JSON.parse_string(session.advance(-1)).ok, "negative advancement is rejected")
	var first_month: Dictionary = JSON.parse_string(session.advance(20))
	check(first_month.ok and first_month.value.date.year == 1444 and first_month.value.date.month == 12 and first_month.value.date.day == 1, "calendar reaches the first month boundary")
	check(first_month.value.tax_settlements == 1 and first_month.value.days_until_next_month == 31, "one monthly settlement reaches the binding")
	var first_tax: Dictionary = JSON.parse_string(session.query_country("AAA"))
	check(first_tax.value.last_monthly_tax.known_subtotal_milli > 0 and first_tax.value.last_monthly_tax.credited_milli == null, "partial original-style income is not credited")
	check(first_tax.value.last_monthly_accruals.adm_power.income == null and first_tax.value.last_monthly_accruals.adm_power.credited == null, "unknown monthly power is not credited")
	var saved: Dictionary = JSON.parse_string(session.save_snapshot())
	check(saved.ok, "calendar world can be saved")
	check(JSON.parse_string(session.advance(345)).value.tax_settlements == 12, "one year executes twelve monthly settlements")
	check(JSON.parse_string(session.restore_snapshot(saved.value)).value.day == 20, "calendar world resumes a saved boundary")
	check(JSON.parse_string(session.advance(0)).value.tax_settlements == 1, "restoring a boundary does not repeat settlement")
	check(not JSON.parse_string(session.restore_snapshot("{}")).ok and JSON.parse_string(session.simulation_view()).value.day == 20, "invalid restore preserves the running world")
	check(north.value.administration.area == "north_area" and north.value.administration.area_status == "state", "capital area reaches administration query")
	check(north.value.administration.history_autonomy_delta_micros == 0 and north.value.administration.raw_autonomy_micros == 300000, "initial autonomy includes ordered history effects")
	check(north.value.administration.effective_autonomy_micros == 0 and north.value.administration.owner_core == "full" and north.value.administration.minimum_autonomy_micros == 0, "baseline full core and capital ceiling reach the query")
	check(north.value.attributes.tax_after_autonomy.steps.size() == 1 and north.value.attributes.tax_after_autonomy.value_micros == null, "tax efficiency and autonomy compose with explicit missing sources")
	check(not JSON.parse_string(session.query_province(-1)).ok, "negative province query fails")
	check(not JSON.parse_string(session.query_province(999)).ok, "unknown province query fails")
	var before_failed_load: Dictionary = JSON.parse_string(session.query_country("AAA"))
	check(not JSON.parse_string(session.load_world(fixture, "1444.13.1")).ok, "invalid replacement fails")
	check(JSON.parse_string(session.query_country("AAA")) == before_failed_load, "failed replacement preserves loaded state")
	var province_query_before: Dictionary = JSON.parse_string(session.query_province(1))
	var province_query_after: Dictionary = JSON.parse_string(session.query_province(1))
	check(province_query_after == province_query_before, "repeated province queries do not mutate state")
	var baseline_save_path := write_baseline_save()
	check(not baseline_save_path.is_empty(), "create a self-authored EU4 baseline save")
	if not baseline_save_path.is_empty():
		var imported: Dictionary = JSON.parse_string(session.load_world_with_baseline_save(
			fixture, "1444.11.11", no_mods, "", "", baseline_save_path
		))
		check(imported.ok, "load HRE membership and office preferences from a baseline save: %s" % imported.get("error", ""))
		if imported.ok:
			check(imported.value.saved_imperial_baseline.provinces_checked == 6, "report imported HRE province count")
			check(imported.value.saved_imperial_baseline.preferences_imported == 1, "report imported preference count")
			var modded_directories: Array[String] = ["unused-mod"]
			var rejected: Dictionary = JSON.parse_string(session.load_world_with_baseline_save(
				fixture, "1444.11.11", modded_directories, "", "", baseline_save_path
			))
			check(not rejected.ok and rejected.error.contains("without Mods"), "reject baseline import over Mod content")
			var voter: Dictionary = JSON.parse_string(session.query_country("AAA"))
			check(voter.value.politics.office_preferences.has("imperial_emperor"), "saved preference reaches the country view")
			var preference_snapshot: Dictionary = JSON.parse_string(session.save_snapshot())
			check(preference_snapshot.ok, "save imported office preference")
			check(JSON.parse_string(session.restore_snapshot(preference_snapshot.value)).ok, "restore imported office preference")
			voter = JSON.parse_string(session.query_country("AAA"))
			check(voter.value.politics.office_preferences.has("imperial_emperor"), "restored preference remains visible")
		DirAccess.remove_absolute(baseline_save_path)
	var ui = load("res://world.tscn").instantiate()
	root.add_child.call_deferred(ui)
	verify_ui.call_deferred(ui)

func write_baseline_save() -> String:
	var expansions := [
		"Conquest of Paradise", "Wealth of Nations", "Res Publica", "Art of War",
		"El Dorado", "Common Sense", "The Cossacks", "Mare Nostrum", "Rights of Man",
		"Mandate of Heaven", "Third Rome", "Cradle of Civilization", "Rule Britannia",
		"Dharma", "Golden Century", "Emperor", "Leviathan", "Origins",
		"Lions of the North", "Domination", "King of Kings", "Winds of Change",
	]
	var dlc_list := ""
	for expansion in expansions:
		if not dlc_list.is_empty():
			dlc_list += " "
		dlc_list += "\"%s\"" % expansion
	var path := ProjectSettings.globalize_path("user://augustus-baseline-save-%d.eu4" % OS.get_process_id())
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return ""
	file.store_string("EU4txt\ndate=1444.11.11\nstart_date=1444.11.11\ncampaign_length=0\nsavegame_version={ first=1 second=37 third=5 forth=0 }\ndlc_enabled={ %s }\ncountries={ AAA={ preferred_emperor=BBB } BBB={} }\nprovinces={ -1={ owner=BBB history={hre=yes} } -2={ owner=AAA history={hre=no} } -3={ history={hre=no} } -4={ history={hre=no} } -5={ history={hre=no} } -6={ history={hre=no} } }\n" % dlc_list)
	var error := file.get_error()
	file.close()
	if error != OK:
		DirAccess.remove_absolute(path)
		return ""
	return path

func verify_ui(ui: Control) -> void:
	await process_frame
	await process_frame
	if ui.overview.is_empty():
		ui.game_path.text = fixture
		var baseline_save_path := write_baseline_save()
		ui.baseline_save.text = baseline_save_path
		var loaded_ok: bool = ui.load_world()
		check(loaded_ok, "UI loads shared fixture: %s" % ui.status.text)
		if loaded_ok:
			check(ui.status.text.contains("6 个 HRE 省份归属和 1 条帝位偏好"), "UI reports imported baseline state")
		if not baseline_save_path.is_empty():
			DirAccess.remove_absolute(baseline_save_path)
	await process_frame
	await process_frame
	check(not ui.overview.is_empty(), "UI has a loaded world")
	if ui.overview.is_empty():
		finish()
		return
	check_layout(ui)
	check(ui.country_choice.item_count == ui.overview.report.countries, "country selector uses loaded definitions")
	var image_size := Vector2(ui.province_image.get_width(), ui.province_image.get_height())
	var factor := minf(ui.map_view.size.x / image_size.x, ui.map_view.size.y / image_size.y)
	var origin: Vector2 = (ui.map_view.size - image_size * factor) / 2.0
	var event := InputEventMouseButton.new()
	event.button_index = MOUSE_BUTTON_LEFT
	event.pressed = true
	event.position = origin + Vector2(0.5, 0.5) * factor
	ui.map_view.gui_input.emit(event)
	var is_fixture: bool = ui.overview.report.map_width == 4 and ui.overview.report.map_height == 2
	if is_fixture:
		check(ui.politics_details.text.contains("Iris"), "country panel displays the loaded ruler")
		check(ui.country_details.text.contains("test_court") and ui.country_details.text.contains("合法性（历史初值）：75.125"), "country panel displays selected reforms and precise legitimacy")
		check(ui.country_details.text.contains("国家理念 test_alpha · 已解锁 0 项"), "country panel displays selected ideas and progress")
		check(ui.country_details.text.contains("3 / 3 / 3") and ui.country_details.text.contains("18.500000"), "country panel displays technology and declared resources")
		var infantry_choice: OptionButton = ui.unit_option_controls["infantry"]
		check(infantry_choice.item_count == 2 and infantry_choice.get_item_metadata(1) == "test_infantry", "country panel lists unlocked unit templates")
		ui.choose_unit_template(1, "infantry")
		var selected_units: Dictionary = JSON.parse_string(ui.session.query_unit_templates("AAA")).value
		check(selected_units.selected.infantry == "test_infantry", "unit selection updates the Rust world state")
		var unit_snapshot: Dictionary = JSON.parse_string(ui.session.save_snapshot())
		if unit_snapshot.ok:
			ui.session.restore_snapshot(unit_snapshot.value)
			selected_units = JSON.parse_string(ui.session.query_unit_templates("AAA")).value
			check(selected_units.selected.infantry == "test_infantry", "unit selection survives save and restore")
		ui.choose_unit_template(0, "infantry")
		selected_units = JSON.parse_string(ui.session.query_unit_templates("AAA")).value
		check(selected_units.selected.infantry == null, "unit selector can clear a country selection")
		check(ui.diplomacy_details.text.contains("贸易联盟成员 BBB"), "country panel preserves trade league meaning")
		check(ui.province_details.text.contains("North · 1"), "map click follows BMP orientation and letterboxing")
		check(ui.province_details.text.contains("BBB"), "clicked province displays current owner")
		check(ui.province_details.text.contains("test_fort") and ui.province_details.text.contains("2.125"), "province panel displays buildings and declared prices")
		check(ui.province_details.text.contains("river") and ui.province_details.text.contains("harbor"), "province panel displays trade routes")
		check(ui.province_details.text.contains("test_growth") and ui.province_details.text.contains("-0.002500"), "province panel displays signed modifier declarations")
		check(ui.province_details.text.contains("0.212000") and ui.province_details.text.contains("831.250000") and ui.province_details.text.contains("同组未接受文化：-15.0000%") and ui.province_details.text.contains("城市：25.0000%"), "province panel explains calculated contributions")
		check(ui.province_details.text.contains("行政状态") and ui.province_details.text.contains("north_area") and ui.province_details.text.contains("30.0000%") and ui.province_details.text.contains("完整核心"), "province panel displays initialized autonomy and cores")
		check(ui.province_details.text.contains("原始自治度：30.0000%") and not ui.province_details.text.contains("历史自治追加"), "applied history is displayed as part of raw autonomy")
		ui.select_country(0)
		await process_frame
		await process_frame
		check(ui.details_scroll.scroll_vertical == 0, "country selection cancels a pending province scroll")
		if DisplayServer.get_name() != "headless":
			await RenderingServer.frame_post_draw
			var rendered := root.get_texture().get_image()
			var position: Vector2 = ui.map_view.global_position + event.position
			var actual := rendered.get_pixel(roundi(position.x), roundi(position.y))
			check(actual.is_equal_approx(Color8(50, 180, 140)), "shader displays political owner color")
		check(ui.advance_world(20), "UI advances the original-style world")
		check(ui.calendar_label.text.contains("1444.12.01") and ui.calendar_label.text.contains("月结 1 次"), "UI displays the authoritative calendar and settlement count")
		check(ui.country_details.text.contains("最近税收月结") and ui.country_details.text.contains("余额尚未确定"), "UI explains incomplete monthly receipts")
		ui.toggle_running()
		check(ui.running and ui.run_button.text == "暂停", "UI starts continuous day requests")
		await create_timer(0.3).timeout
		ui.set_running(false)
		var paused_day: int = int(JSON.parse_string(ui.session.simulation_view()).value.day)
		check(paused_day > 20, "continuous mode actually advances the core")
		await create_timer(0.3).timeout
		check(int(JSON.parse_string(ui.session.simulation_view()).value.day) == paused_day, "pause stops day requests")
		var save_path := "user://augustus-calendar-test-%d.json" % OS.get_process_id()
		if not FileAccess.file_exists(save_path):
			check(ui.save_world(save_path), "UI writes its own snapshot file")
			check(ui.advance_world(365), "UI advances after saving")
			check(ui.restore_world(save_path), "UI reads its snapshot file")
			check(int(JSON.parse_string(ui.session.simulation_view()).value.day) == paused_day and not ui.running, "UI restore recovers the saved day and remains paused")
			check(DirAccess.remove_absolute(ProjectSettings.globalize_path(save_path)) == OK, "remove this process's test snapshot")
		else:
			check(false, "test snapshot path must be unused")
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		if args[i] == "--country" and i + 1 < args.size():
			for index in ui.country_choice.item_count:
				if ui.country_choice.get_item_metadata(index) == args[i + 1]:
					ui.country_choice.select(index)
					ui.select_country(index)
					var country: Dictionary = JSON.parse_string(ui.session.query_country(args[i + 1])).value
					if country.estates != null:
						check(ui.country_details.text.contains("阶层"), "country panel displays estate state")
						for name in country.estates:
							if country.estates[name].active == true:
								check(ui.country_details.text.contains(name), "country panel displays active estate")
								if country.estates[name].loyalty_milli == null:
									check(ui.country_details.text.contains("忠诚度 未载入"), "country panel preserves unknown estate loyalty")
		if args[i] == "--province" and i + 1 < args.size():
			check(ui.select_province(int(args[i + 1])), "select requested province")
		if args[i] == "--days" and i + 1 < args.size():
			check(ui.advance_world(int(args[i + 1])), "advance requested calendar days")
		if args[i] == "--capture" and i + 1 < args.size():
			await process_frame
			await process_frame
			check_layout(ui)
			await RenderingServer.frame_post_draw
			var image := root.get_texture().get_image()
			check(not image.is_empty(), "rendered world is available")
			if not image.is_empty():
				check(image.save_png(args[i + 1]) == OK, "save rendered world")
	finish()

func finish() -> void:
	print("Augustus world integration: %d failures" % failures)
	quit(1 if failures else 0)

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

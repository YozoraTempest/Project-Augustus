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
	check(not JSON.parse_string(session.query_country("AAA")).ok, "queries require a loaded world")
	check(JSON.parse_string(session.load_world(fixture, "1444.11.11")).ok, "load independent fixture")
	var alpha: Dictionary = JSON.parse_string(session.query_country("AAA"))
	check(alpha.value.bound_capital == 2, "country query uses core capital relation")
	check(alpha.value.owned_provinces.size() == 1 and int(alpha.value.owned_provinces[0]) == 2, "country query uses actual ownership")
	check(alpha.value.politics.ruler.name == "Iris", "ruler query reads authoritative state")
	check(alpha.value.politics.heir.claim_milli == 87125, "heir claim preserves precision")
	check(alpha.value.diplomacy.size() == 5, "diplomatic relationships reach the UI query")
	check(alpha.value.technology.administrative == 3 and alpha.value.technology.military == 3, "baseline technology reaches the country query")
	check(alpha.value.resources.mercantilism_milli == 18500 and alpha.value.resources.treasury_milli == null, "declared resources preserve precision and unknown stocks")
	var north: Dictionary = JSON.parse_string(session.query_province(1))
	check(north.value.owner == "BBB" and north.value.tax_milli == 2125, "dated history reaches the Rust core")
	check(north.value.buildings == ["test_fort"], "existing buildings reach the province query")
	check(north.value.trade_good_definition.base_price_milli == 2125, "base price preserves precision")
	check(north.value.trade_node == "river" and north.value.trade_node_outgoing == ["harbor"], "province query follows the trade graph")
	check(north.value.modifiers == ["test_harbor", "test_river"] and north.value.declared_modifier_totals_micros.test_growth == -2500, "permanent modifier bindings retain signed precision")
	check(north.value.attributes.tax_basis.known_subtotal_micros == 2656250 and north.value.attributes.tax_basis.value_micros == null, "attribute query separates known calculation from complete result")
	check(north.value.attributes.manpower_contribution.known_subtotal_micros == 962500000, "manpower calculation includes trade good effects")
	check(alpha.value.province_attribute_totals.tax_basis.known_subtotal_micros == 5100000, "country aggregation includes buildings and trade goods")
	check(north.value.administration.area == "north_area" and north.value.administration.area_status == "state", "capital area reaches administration query")
	check(north.value.administration.history_autonomy_delta_micros == 250000 and north.value.administration.raw_autonomy_micros == null, "history delta does not replace an unknown baseline")
	check(north.value.administration.effective_autonomy_micros == 0 and north.value.administration.owner_core == "full" and north.value.administration.minimum_autonomy_micros == 0, "baseline full core and capital ceiling reach the query")
	check(north.value.attributes.tax_after_autonomy.steps.size() == 2 and north.value.attributes.tax_after_autonomy.value_micros == null, "autonomy is an additional stage with explicit missing sources")
	check(not JSON.parse_string(session.query_province(-1)).ok, "negative province query fails")
	check(not JSON.parse_string(session.query_province(999)).ok, "unknown province query fails")
	check(not JSON.parse_string(session.load_world(fixture, "1444.13.1")).ok, "invalid replacement fails")
	check(JSON.parse_string(session.query_country("AAA")) == alpha, "failed replacement preserves loaded state")
	check(JSON.parse_string(session.query_province(1)) == north, "queries do not mutate state")
	var ui = load("res://world.tscn").instantiate()
	root.add_child.call_deferred(ui)
	verify_ui.call_deferred(ui)

func verify_ui(ui: Control) -> void:
	await process_frame
	await process_frame
	if ui.overview.is_empty():
		ui.game_path.text = fixture
		check(ui.load_world(), "UI loads shared fixture")
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
		check(ui.country_details.text.contains("3 / 3 / 3") and ui.country_details.text.contains("18.500000"), "country panel displays technology and declared resources")
		check(ui.diplomacy_details.text.contains("贸易联盟成员 BBB"), "country panel preserves trade league meaning")
		check(ui.province_details.text.contains("North · 1"), "map click follows BMP orientation and letterboxing")
		check(ui.province_details.text.contains("BBB"), "clicked province displays current owner")
		check(ui.province_details.text.contains("test_fort") and ui.province_details.text.contains("2.125"), "province panel displays buildings and declared prices")
		check(ui.province_details.text.contains("river") and ui.province_details.text.contains("harbor"), "province panel displays trade routes")
		check(ui.province_details.text.contains("test_growth") and ui.province_details.text.contains("-0.002500"), "province panel displays signed modifier declarations")
		check(ui.province_details.text.contains("2.656250") and ui.province_details.text.contains("962.500000") and ui.province_details.text.contains("城市：25.0000%"), "province panel explains calculated contributions")
		check(ui.province_details.text.contains("行政状态") and ui.province_details.text.contains("north_area") and ui.province_details.text.contains("+25.0000%") and ui.province_details.text.contains("完整核心"), "province panel explains baseline cores and unapplied delta")
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
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		if args[i] == "--country" and i + 1 < args.size():
			for index in ui.country_choice.item_count:
				if ui.country_choice.get_item_metadata(index) == args[i + 1]:
					ui.country_choice.select(index)
					ui.select_country(index)
		if args[i] == "--province" and i + 1 < args.size():
			check(ui.select_province(int(args[i + 1])), "select requested province")
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

# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends Control

var session = AugustusSession.new()
var state: Dictionary = {}
var selected_province := 0
var paused := true
var title_label: Label
var date_label: Label
var treasury_label: Label
var page_scroll: ScrollContainer
var columns: BoxContainer
var map_column: HBoxContainer
var province_panel: VBoxContainer
var province_buttons: Array[Button] = []
var province_details: Label
var building_choice: OptionButton
var system_choice: OptionButton
var input_choice: OptionButton
var pause_button: Button
var advance_one_button: Button
var save_button: Button
var restore_button: Button
var build_button: Button
var reorganise_button: Button
var status_label: Label
var events_label: Label
var clock: Timer

func _ready() -> void:
	var font := SystemFont.new()
	font.font_names = PackedStringArray(["Microsoft YaHei", "Noto Sans CJK SC"])
	var ui_theme := Theme.new()
	ui_theme.default_font = font
	ui_theme.default_font_size = 17
	theme = ui_theme
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 24)
	add_child(margin)
	page_scroll = ScrollContainer.new()
	page_scroll.name = "ConstructionPageScroll"
	page_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	page_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	page_scroll.follow_focus = true
	page_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	page_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	margin.add_child(page_scroll)
	var page := VBoxContainer.new()
	page.add_theme_constant_override("separation", 12)
	page.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	page.size_flags_vertical = Control.SIZE_EXPAND_FILL
	page_scroll.add_child(page)
	var header := hflow()
	page.add_child(header)
	title_label = label("PROJECT AUGUSTUS", 28)
	header.add_child(title_label)
	date_label = label("第 0 天", 22)
	date_label.custom_minimum_size.x = 120
	date_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	date_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	header.add_child(date_label)
	treasury_label = label("国库", 22)
	treasury_label.custom_minimum_size.x = 160
	treasury_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	treasury_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	header.add_child(treasury_label)
	var subtitle := label("建造演示 · 两个省份，一套世界状态", 16)
	subtitle.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	page.add_child(subtitle)
	var settings := hflow()
	page.add_child(settings)
	input_choice = OptionButton.new()
	input_choice.add_item("EU4 写法的回调")
	input_choice.add_item("原生规则")
	input_choice.fit_to_longest_item = false
	input_choice.clip_text = true
	input_choice.custom_minimum_size.x = 170
	settings.add_child(input_choice)
	system_choice = OptionButton.new()
	system_choice.add_item("固定工期")
	system_choice.add_item("按建设能力推进")
	system_choice.fit_to_longest_item = false
	system_choice.clip_text = true
	system_choice.custom_minimum_size.x = 170
	settings.add_child(system_choice)
	settings.add_child(button("重新开始", new_session))
	settings.add_child(button("原版开局", func(): get_tree().change_scene_to_file("res://world.tscn")))
	var settings_hint := label("切换规则后重新开始", 15)
	settings_hint.custom_minimum_size.x = 180
	settings_hint.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	settings_hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	settings.add_child(settings_hint)
	var controls := hflow()
	page.add_child(controls)
	pause_button = button("继续", toggle_pause)
	controls.add_child(pause_button)
	advance_one_button = button("推进 1 天", func(): apply_packet(session.advance(1)))
	controls.add_child(advance_one_button)
	controls.add_child(button("推进 15 天", func(): apply_packet(session.advance(15))))
	save_button = button("保存", save_game)
	controls.add_child(save_button)
	restore_button = button("恢复", restore_game)
	controls.add_child(restore_button)
	columns = BoxContainer.new()
	columns.name = "ConstructionColumns"
	columns.vertical = false
	columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_theme_constant_override("separation", 24)
	page.add_child(columns)
	map_column = HBoxContainer.new()
	map_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(map_column)
	for index in 2:
		var region := button("", func(): select_province(index))
		region.custom_minimum_size = Vector2(240, 260)
		region.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var style := StyleBoxFlat.new()
		style.bg_color = Color("294e45") if index == 0 else Color("29485e")
		style.set_corner_radius_all(12)
		style.set_border_width_all(2)
		style.border_color = Color("668c84") if index == 0 else Color("7294ad")
		region.add_theme_stylebox_override("normal", style)
		province_buttons.append(region)
		map_column.add_child(region)
	province_panel = VBoxContainer.new()
	province_panel.custom_minimum_size.x = 310
	province_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	province_panel.add_theme_constant_override("separation", 10)
	columns.add_child(province_panel)
	province_panel.add_child(label("省份建设", 22))
	province_details = label("", 17)
	province_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	province_panel.add_child(province_details)
	building_choice = OptionButton.new()
	building_choice.fit_to_longest_item = false
	building_choice.clip_text = true
	building_choice.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	province_panel.add_child(building_choice)
	building_choice.item_selected.connect(func(_index): refresh())
	build_button = button("开始建造", build_selected)
	province_panel.add_child(build_button)
	reorganise_button = button("调整施工安排", func(): apply_packet(session.reorganise(selected_province)))
	reorganise_button.tooltip_text = "演示行动：随机将建设能力设为 1 或 2。结果随存档保存。"
	province_panel.add_child(reorganise_button)
	status_label = label("", 15)
	status_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	province_panel.add_child(status_label)
	events_label = label("", 15)
	events_label.custom_minimum_size.y = 100
	events_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	events_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	page.add_child(events_label)
	clock = Timer.new()
	clock.wait_time = 0.5
	clock.timeout.connect(func():
		if not paused:
			apply_packet(session.advance(1)))
	add_child(clock)
	clock.start()
	get_viewport().size_changed.connect(_update_columns_layout)
	_update_columns_layout.call_deferred()
	new_session()

func hflow() -> HFlowContainer:
	var flow := HFlowContainer.new()
	flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	flow.add_theme_constant_override("h_separation", 12)
	flow.add_theme_constant_override("v_separation", 6)
	return flow

func _update_columns_layout() -> void:
	if page_scroll == null or columns == null or map_column == null or province_panel == null:
		return
	var scrollbar_width: float = page_scroll.get_v_scroll_bar().get_combined_minimum_size().x
	var available_width := get_viewport_rect().size.x - 48.0 - scrollbar_width
	var required_width := (
		map_column.get_combined_minimum_size().x
		+ province_panel.get_combined_minimum_size().x
		+ float(columns.get_theme_constant("separation"))
	)
	columns.vertical = available_width < required_width

func label(text: String, size: int) -> Label:
	var node := Label.new()
	node.text = text
	node.add_theme_font_size_override("font_size", size)
	return node

func button(text: String, action: Callable) -> Button:
	var node := Button.new()
	node.text = text
	node.custom_minimum_size.y = 42
	node.pressed.connect(action)
	return node

func apply_packet(json: String) -> bool:
	var packet: Dictionary = JSON.parse_string(json)
	if not packet.ok:
		status_label.text = packet.error
		return false
	state = packet.value
	refresh()
	return true

func new_session() -> void:
	paused = true
	pause_button.text = "继续"
	selected_province = 0
	status_label.text = "选择省份，再下达建造操作。"
	apply_packet(session.start_session(system_choice.selected == 1, input_choice.selected == 1))
	building_choice.clear()
	for building in state.buildings:
		building_choice.add_item("%s · %.2f 金币" % [building.name, building.cost_milli / 1000.0])
	refresh()

func select_province(index: int) -> void:
	selected_province = index
	refresh()

func build_selected() -> void:
	if apply_packet(session.build(selected_province, building_choice.selected)):
		status_label.text = "已扣除费用，建造进入队列。"

func toggle_pause() -> void:
	paused = not paused
	pause_button.text = "继续" if paused else "暂停"

func save_game(path: String = "user://construction-save.json") -> bool:
	var packet: Dictionary = JSON.parse_string(session.save_snapshot_to_file(ProjectSettings.globalize_path(path)))
	if not packet.ok:
		status_label.text = "保存失败：%s" % packet.error
		return false
	status_label.text = "已保存到本地。"
	return true

func restore_game(path: String = "user://construction-save.json") -> bool:
	if not FileAccess.file_exists(path):
		status_label.text = "还没有存档。"
		return false
	var snapshot := FileAccess.get_file_as_string(path)
	if apply_packet(session.restore_snapshot(snapshot)):
		paused = true
		pause_button.text = "继续"
		status_label.text = "已恢复，当前暂停。"
		return true
	return false

func refresh() -> void:
	if state.is_empty():
		return
	date_label.text = "第 %d 天   " % state.day
	treasury_label.text = "国库 %.2f" % (state.treasury_milli / 1000.0)
	for index in state.provinces.size():
		var province: Dictionary = state.provinces[index]
		province_buttons[index].text = "%s\n\n建设能力 %.1f / 天\n已建成 %d 座%s" % [province.name, province.capacity_milli_per_day / 1000.0, province.buildings.size(), "\n\n已选中" if index == selected_province else ""]
	var province: Dictionary = state.provinces[selected_province]
	var queue := "空闲"
	if province.queued_building != null:
		var building: Dictionary = state.buildings[int(province.queued_building)]
		queue = "正在建造 %s" % building.name
		if state.construction_system.begins_with("capacity"):
			queue += "\n进度 %.1f / %.1f" % [province.progress_milli / 1000.0, building.work_milli / 1000.0]
		else:
			queue += "\n固定工期 %d 天" % building.standard_days
	province_details.text = "%s\n\n%s\n\n已建建筑：%s" % [province.name, queue, ", ".join(province.buildings) if not province.buildings.is_empty() else "无"]
	build_button.disabled = province.queued_building != null
	reorganise_button.disabled = not state.construction_system.begins_with("capacity")
	if building_choice.item_count > 0:
		var building: Dictionary = state.buildings[building_choice.selected]
		build_button.disabled = build_button.disabled or province.buildings.has(building.id) or state.treasury_milli < building.cost_milli
	var lines := PackedStringArray(["事件记录"])
	var start := maxi(0, state.events.size() - 4)
	for index in range(start, state.events.size()):
		var event: Array = state.events[index]
		lines.append("第 %d 天 · %s" % [event[0], event_text(event[2])])
	events_label.text = "\n".join(lines)

func event_text(message: String) -> String:
	var parts := message.split(":", false, 1)
	if parts.size() == 2:
		if parts[0] == "capacity":
			return "施工安排已调整，建设能力为 %s / 天" % parts[1]
		for building in state.buildings:
			if building.id == parts[1]:
				if parts[0] == "queued":
					return "已开始建造 %s" % building.name
				if parts[0] == "completed":
					return "%s 建造完成" % building.name
	return message

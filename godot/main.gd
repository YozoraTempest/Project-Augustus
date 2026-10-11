# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends Control

var session = AugustusSession.new()
var state: Dictionary = {}
var selected_province := 0
var saved_snapshot := ""
var paused := true
var title_label: Label
var date_label: Label
var treasury_label: Label
var province_buttons: Array[Button] = []
var province_details: Label
var building_choice: OptionButton
var system_choice: OptionButton
var input_choice: OptionButton
var pause_button: Button
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
	var page := VBoxContainer.new()
	page.add_theme_constant_override("separation", 12)
	margin.add_child(page)
	var header := HBoxContainer.new()
	page.add_child(header)
	title_label = label("PROJECT AUGUSTUS", 28)
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(title_label)
	date_label = label("第 0 天", 22)
	header.add_child(date_label)
	treasury_label = label("国库", 22)
	header.add_child(treasury_label)
	page.add_child(label("建造演示 · 两个省份，一套世界状态", 16))
	var settings := HBoxContainer.new()
	page.add_child(settings)
	input_choice = OptionButton.new()
	input_choice.add_item("EU4 写法的回调")
	input_choice.add_item("原生规则")
	settings.add_child(input_choice)
	system_choice = OptionButton.new()
	system_choice.add_item("固定工期")
	system_choice.add_item("按建设能力推进")
	settings.add_child(system_choice)
	settings.add_child(button("重新开始", new_session))
	settings.add_child(button("原版开局", func(): get_tree().change_scene_to_file("res://world.tscn")))
	settings.add_child(label("切换规则后重新开始", 15))
	var controls := HBoxContainer.new()
	page.add_child(controls)
	pause_button = button("继续", toggle_pause)
	controls.add_child(pause_button)
	controls.add_child(button("推进 1 天", func(): apply_packet(session.advance(1))))
	controls.add_child(button("推进 15 天", func(): apply_packet(session.advance(15))))
	controls.add_child(button("保存", save_game))
	controls.add_child(button("恢复", restore_game))
	var middle := HBoxContainer.new()
	middle.size_flags_vertical = Control.SIZE_EXPAND_FILL
	middle.add_theme_constant_override("separation", 24)
	page.add_child(middle)
	var map := HBoxContainer.new()
	map.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	middle.add_child(map)
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
		map.add_child(region)
	var panel := VBoxContainer.new()
	panel.custom_minimum_size.x = 310
	panel.add_theme_constant_override("separation", 10)
	middle.add_child(panel)
	panel.add_child(label("省份建设", 22))
	province_details = label("", 17)
	province_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(province_details)
	building_choice = OptionButton.new()
	panel.add_child(building_choice)
	building_choice.item_selected.connect(func(_index): refresh())
	build_button = button("开始建造", build_selected)
	panel.add_child(build_button)
	reorganise_button = button("调整施工安排", func(): apply_packet(session.reorganise(selected_province)))
	reorganise_button.tooltip_text = "演示行动：随机将建设能力设为 1 或 2。结果随存档保存。"
	panel.add_child(reorganise_button)
	status_label = label("", 15)
	status_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	status_label.max_lines_visible = 2
	panel.add_child(status_label)
	events_label = label("", 15)
	events_label.custom_minimum_size.y = 100
	page.add_child(events_label)
	clock = Timer.new()
	clock.wait_time = 0.5
	clock.timeout.connect(func():
		if not paused:
			apply_packet(session.advance(1)))
	add_child(clock)
	clock.start()
	new_session()

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

func save_game() -> void:
	var packet: Dictionary = JSON.parse_string(session.save_snapshot())
	if not packet.ok:
		status_label.text = packet.error
		return
	var file := FileAccess.open("user://construction-save.json", FileAccess.WRITE)
	if file == null:
		status_label.text = "无法写入存档。"
		return
	saved_snapshot = packet.value
	file.store_string(saved_snapshot)
	status_label.text = "已保存到本地。"

func restore_game() -> void:
	if not FileAccess.file_exists("user://construction-save.json"):
		status_label.text = "还没有存档。"
		return
	var snapshot := FileAccess.get_file_as_string("user://construction-save.json")
	if apply_packet(session.restore_snapshot(snapshot)):
		paused = true
		pause_button.text = "继续"
		status_label.text = "已恢复，当前暂停。"

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

# SPDX-License-Identifier: GPL-3.0-only
class_name CountryMilitaryOverview
extends PanelContainer

signal army_selected(army_id: int)
signal recruitment_selected(province_id: int)

@onready var country_label: Label = %Country
@onready var summary_label: Label = %Summary
@onready var tabs: TabContainer = %Lists
@onready var armies_box: VBoxContainer = %Armies
@onready var recruitments_box: VBoxContainer = %Recruitments
@onready var no_armies: Label = %NoArmies
@onready var no_recruitments: Label = %NoRecruitments

var _army_rows: Dictionary[int, Button] = {}
var _recruitment_rows: Dictionary[int, Button] = {}


func _ready() -> void:
	GameSession.clock_changed.connect(refresh)
	GameSession.world_changed.connect(refresh)
	visibility_changed.connect(refresh)


func refresh() -> void:
	if not is_visible_in_tree() or not GameSession.is_started():
		return
	var info := GameSession.backend.get_player_military_info()
	country_label.text = "%s · 军事总览" % info.country_name
	summary_label.text = "军队：%d 支 · %d 团 · %d 人\n可用人力：%d / %d\n每月恢复：+%d 人\n月度维护：%s 金币" % [info.army_count, info.regiment_count, info.strength, info.manpower.available, info.manpower.capacity, info.manpower.monthly_recovery, info.monthly_maintenance]
	tabs.set_tab_title(0, "军队 (%d)" % info.army_count)
	tabs.set_tab_title(1, "招募 (%d)" % info.recruitment_count)
	no_armies.visible = info.armies.is_empty()
	no_recruitments.visible = info.recruitments.is_empty()
	var army_ids: Array[int] = []
	for army: Dictionary in info.armies:
		var id: int = army.id
		army_ids.append(id)
		var button := _row(armies_box, _army_rows, id, func(): army_selected.emit(id))
		var location := "驻扎：" + str(army.province_name)
		if army.movement != null:
			location = "%s → %s\n剩余 %d 天 · %s 抵达" % [army.province_name, army.movement.destination_name, army.movement.days_remaining, army.movement.arrives_on]
		button.text = "%s\n%d 团 · %d 人\n%s" % [army.name, army.regiment_count, army.strength, location]
		button.tooltip_text = button.text
		armies_box.move_child(button, army_ids.size() - 1)
	_remove_missing(armies_box, _army_rows, army_ids)
	var order_ids: Array[int] = []
	for project: Dictionary in info.recruitments:
		var order_id: int = project.id
		var province: int = project.province
		order_ids.append(order_id)
		var button := _row(recruitments_box, _recruitment_rows, order_id, func(): recruitment_selected.emit(province))
		var phase := "排队" if project.queued else "正在招募"
		button.text = "%s · %s #%d · %s\n进度：%d%% · 剩余 %d 天\n完成：%s" % [project.province_name, project.kind, order_id, phase, roundi(project.progress), project.days_remaining, project.completes_on]
		button.tooltip_text = button.text
		recruitments_box.move_child(button, order_ids.size() - 1)
	_remove_missing(recruitments_box, _recruitment_rows, order_ids)


func _row(container: VBoxContainer, rows: Dictionary[int, Button], id: int, activate: Callable) -> Button:
	# 按编号复用列表项，日期刷新时保留滚动位置和键盘焦点。
	if not rows.has(id):
		var button := Button.new()
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		button.pressed.connect(activate)
		container.add_child(button)
		rows[id] = button
	return rows[id]


func _remove_missing(container: VBoxContainer, rows: Dictionary[int, Button], present: Array[int]) -> void:
	for id: int in rows.keys():
		if not present.has(id):
			container.remove_child(rows[id])
			rows[id].queue_free()
			rows.erase(id)

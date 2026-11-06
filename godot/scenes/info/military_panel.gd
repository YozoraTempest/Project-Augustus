# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceMilitaryPanel
extends VBoxContainer

signal recruitment_requested(province_id: int)
signal recruitment_cancel_requested(province_id: int)
signal army_selected(army_id: int)
signal army_stop_requested(army_id: int)
signal armies_merge_requested(target: int, sources: PackedInt64Array)
signal army_split_requested(army_id: int, count: int)
signal army_disband_requested(army_id: int)

@onready var province_label: Label = %MilitaryProvince
@onready var recruitment_label: Label = %RecruitmentDetails
@onready var recruit_button: Button = %RecruitInfantry
@onready var reason_label: Label = %RecruitmentReason
@onready var project_box: VBoxContainer = %RecruitmentProject
@onready var project_label: Label = %RecruitmentDates
@onready var progress_bar: ProgressBar = %RecruitmentProgress
@onready var cancel_button: Button = %CancelRecruitment
@onready var refund_label: Label = %RecruitmentRefund
@onready var garrison_label: Label = %GarrisonTitle
@onready var armies_box: VBoxContainer = %Armies
@onready var army_details: Label = %ArmyDetails
@onready var movement_details: Label = %MovementDetails
@onready var movement_progress: ProgressBar = %MovementProgress
@onready var stop_button: Button = %StopArmy
@onready var movement_feedback: Label = %MovementFeedback
@onready var organization_panel: ArmyOrganizationPanel = %OrganizationPanel
@onready var disband_button: Button = %DisbandArmy

var _province_id := 0
var _selected_army := 0


func _ready() -> void:
	recruit_button.pressed.connect(func(): recruitment_requested.emit(_province_id))
	cancel_button.pressed.connect(func(): recruitment_cancel_requested.emit(_province_id))
	stop_button.pressed.connect(func(): army_stop_requested.emit(_selected_army))
	disband_button.pressed.connect(func(): army_disband_requested.emit(_selected_army))
	organization_panel.armies_merge_requested.connect(func(target: int, sources: PackedInt64Array): armies_merge_requested.emit(target, sources))
	organization_panel.army_split_requested.connect(func(id: int, count: int): army_split_requested.emit(id, count))
	hide()


func show_province(province_id: int, info: Dictionary, selected_army: int, organization: Dictionary = {}) -> void:
	_province_id = province_id
	_selected_army = selected_army
	organization_panel.show_info(organization)
	visible = not info.is_empty()
	if not visible:
		return
	province_label.text = info.province_name + " · 军事"
	recruitment_label.text = "步兵 · 1 团 · %d 人\n基础费用：%s 金币\n人力消耗：%d 人\n招募时间：%d 天" % [info.strength, info.cost, info.manpower_cost, info.days]
	if info.manpower != null:
		recruitment_label.text += "\n可用人力：%d / %d\n每月恢复：+%d 人" % [info.manpower.available, info.manpower.capacity, info.manpower.monthly_recovery]
	recruit_button.disabled = not info.can_recruit
	recruit_button.tooltip_text = "" if info.completes_on == null else "预计完成：%s" % info.completes_on
	reason_label.text = info.reason
	reason_label.visible = not info.reason.is_empty()
	project_box.visible = info.project != null
	if info.project != null:
		var project: Dictionary = info.project
		project_label.text = "正在招募：步兵 (%s)\n开始：%s\n完成：%s · 剩余 %d 天" % [project.country, project.started_on, project.completes_on, project.days_remaining]
		progress_bar.value = project.progress
		cancel_button.visible = project.can_cancel
		refund_label.visible = project.can_cancel
		if project.can_cancel:
			refund_label.text = "取消返还：%s 金币 · %d 人力" % [project.refund_cost, project.refund_manpower]
			refund_label.tooltip_text = project.refund_rules
			cancel_button.tooltip_text = project.refund_rules
	garrison_label.text = "军队：无" if info.armies.is_empty() else "军队"
	for child in armies_box.get_children():
		armies_box.remove_child(child)
		child.queue_free()
	army_details.hide()
	movement_details.hide()
	movement_progress.hide()
	stop_button.hide()
	disband_button.hide()
	movement_feedback.hide()
	for army: Dictionary in info.armies:
		var id: int = army.id
		var button := Button.new()
		button.text = "%s · %d 人%s" % [army.name, army.strength, " · 行军中" if army.movement != null else ""]
		button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		button.toggle_mode = true
		button.set_pressed_no_signal(id == selected_army)
		button.pressed.connect(func(): army_selected.emit(id))
		armies_box.add_child(button)
		if id == selected_army:
			disband_button.visible = army.can_command
			disband_button.disabled = army.movement != null
			disband_button.tooltip_text = "请先让军队驻扎" if disband_button.disabled else ""
			army_details.text = "%s\n编号：#%d\n所属国家：%s (%s)\n驻扎：%s (#%d)\n部队：%d 团 · 总兵力 %d 人" % [army.name, id, army.country_name, army.country, army.province_name, army.province, army.regiment_count, army.strength]
			if army.movement != null:
				army_details.text = army_details.text.replace("驻扎：", "当前省份：")
				var movement: Dictionary = army.movement
				var route_names := PackedStringArray([movement.route[0].from_name])
				for leg: Dictionary in movement.route:
					route_names.append(leg.to_name)
				movement_details.text = "目的地：%s\n路线：%s\n当前路段：%s → %s\n本段抵达：%s\n出发：%s\n预计抵达：%s · 剩余 %d 天" % [movement.destination_name, " → ".join(route_names), movement.from_name, movement.to_name, movement.leg_arrives_on, movement.started_on, movement.arrives_on, movement.days_remaining]
				movement_progress.value = movement.progress
				movement_details.show()
				movement_progress.show()
				stop_button.visible = army.can_command
				stop_button.disabled = movement.leg_index + 1 == movement.route.size()
				stop_button.text = "抵达后驻扎" if stop_button.disabled else "停止行军"
				stop_button.tooltip_text = "" if stop_button.disabled else "停止于：" + str(movement.to_name)
			else:
				movement_details.text = "可达邻省：" + ("无" if army.move_targets.is_empty() else "、".join(army.move_targets))
				movement_details.show()
			for index in army.regiments.size():
				var regiment: Dictionary = army.regiments[index]
				army_details.text += "\n第 %d 团：%s · %d 人" % [index + 1, regiment.kind, regiment.strength]
			army_details.show()


func show_error(message: String) -> void:
	reason_label.text = message
	reason_label.show()


func show_movement_error(message: String) -> void:
	movement_feedback.text = "无法行军：" + message
	movement_feedback.modulate = Color(1.0, 0.55, 0.48)
	movement_feedback.show()


func show_organization_error(message: String) -> void:
	organization_panel.show_error(message)


func show_disband_error(message: String) -> void:
	movement_feedback.text = "无法解散：" + message
	movement_feedback.show()

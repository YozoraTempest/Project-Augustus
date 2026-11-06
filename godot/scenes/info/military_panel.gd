# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceMilitaryPanel
extends VBoxContainer

signal recruitment_requested(province_id: int, kind: String)
signal recruitment_cancel_requested(province_id: int, order_id: int)
signal army_selected(army_id: int)
signal army_stop_requested(army_id: int)
signal armies_merge_requested(target: int, sources: PackedInt64Array)
signal army_split_requested(army_id: int, count: int)
signal army_disband_requested(army_id: int)

@onready var province_label: Label = %MilitaryProvince
@onready var manpower_label: Label = %RecruitmentManpower
@onready var infantry_label: Label = %InfantryDetails
@onready var recruit_infantry: Button = %RecruitInfantry
@onready var infantry_reason: Label = %InfantryReason
@onready var cavalry_label: Label = %CavalryDetails
@onready var recruit_cavalry: Button = %RecruitCavalry
@onready var cavalry_reason: Label = %CavalryReason
@onready var reason_label: Label = %RecruitmentReason
@onready var project_box: VBoxContainer = %RecruitmentProject
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
	recruit_infantry.pressed.connect(func(): recruitment_requested.emit(_province_id, "infantry"))
	recruit_cavalry.pressed.connect(func(): recruitment_requested.emit(_province_id, "cavalry"))
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
	if info.manpower != null:
		manpower_label.text = "可用人力：%d / %d\n每月恢复：+%d 人" % [info.manpower.available, info.manpower.capacity, info.manpower.monthly_recovery]
		manpower_label.show()
	else:
		manpower_label.hide()
	_show_offer(_offer(info, "infantry"), infantry_label, recruit_infantry, infantry_reason)
	_show_offer(_offer(info, "cavalry"), cavalry_label, recruit_cavalry, cavalry_reason)
	reason_label.hide()
	for child in project_box.get_children():
		project_box.remove_child(child)
		child.queue_free()
	var orders: Array = info.orders
	project_box.visible = not orders.is_empty()
	for order: Dictionary in orders:
		var order_id: int = order.id
		var row := VBoxContainer.new()
		var title := Label.new()
		title.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		var phase := "排队" if order.queued else "正在招募"
		title.text = "%s：%s #%d (%s)\n开始：%s\n完成：%s · 剩余 %d 天" % [phase, order.kind, order_id, order.country, order.started_on, order.completes_on, order.days_remaining]
		row.add_child(title)
		var progress := ProgressBar.new()
		progress.custom_minimum_size = Vector2(0, 28)
		progress.max_value = 100
		progress.value = order.progress
		row.add_child(progress)
		if order.can_cancel:
			var refund := Label.new()
			refund.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			refund.text = "取消返还：%s 金币 · %d 人力" % [order.refund_cost, order.refund_manpower]
			refund.tooltip_text = order.refund_rules
			row.add_child(refund)
			var cancel := Button.new()
			cancel.text = "取消招募"
			cancel.tooltip_text = order.refund_rules
			cancel.pressed.connect(func(): recruitment_cancel_requested.emit(_province_id, order_id))
			row.add_child(cancel)
		project_box.add_child(row)
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
			army_details.text = "%s\n编号：#%d\n所属国家：%s (%s)\n驻扎：%s (#%d)\n部队：%d 团 · 总兵力 %d 人\n%s" % [army.name, id, army.country_name, army.country, army.province_name, army.province, army.regiment_count, army.strength, _composition_text(army)]
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
			army_details.text += _supply_text(army)
			if army.has("reinforcement"):
				army_details.text += "\n" + str(army.reinforcement)
			army_details.show()


func show_error(message: String) -> void:
	reason_label.text = message
	reason_label.show()


func _offer(info: Dictionary, kind_id: String) -> Dictionary:
	for offer: Dictionary in info.offers:
		if offer.kind_id == kind_id:
			return offer
	return {}


func _show_offer(offer: Dictionary, label: Label, button: Button, reason: Label) -> void:
	label.text = "%s · 1 团 · %d 人\n费用：%s 金币\n人力消耗：%d 人\n招募时间：%d 天" % [offer.kind, offer.strength, offer.cost, offer.manpower_cost, offer.days]
	button.disabled = not offer.can_recruit
	button.tooltip_text = "" if offer.completes_on == null else "预计完成：%s" % offer.completes_on
	reason.text = offer.reason
	reason.visible = not str(offer.reason).is_empty()


func _supply_text(army: Dictionary) -> String:
	var supply = army.get("supply")
	if supply == null:
		return ""
	var reason := "未超出补给，预计不减员"
	if int(supply.regiment_count) > int(supply.supply_limit):
		reason = "超出补给 %d 团，预计减少 %d 人" % [int(supply.regiment_count) - int(supply.supply_limit), int(supply.loss)]
	return "\n下次补给：%s · %s\n补给上限 %d · 同省 %d 团\n%s" % [str(supply.date), str(supply.province_name), int(supply.supply_limit), int(supply.regiment_count), reason]


func _composition_text(army: Dictionary) -> String:
	var lines := PackedStringArray()
	for unit: Dictionary in army.composition:
		lines.append("%s：%d 团 · %d 人" % [unit.kind, unit.regiment_count, unit.strength])
	return "\n".join(lines)


func show_movement_error(message: String) -> void:
	movement_feedback.text = "无法行军：" + message
	movement_feedback.modulate = Color(1.0, 0.55, 0.48)
	movement_feedback.show()


func show_organization_error(message: String) -> void:
	organization_panel.show_error(message)


func show_disband_error(message: String) -> void:
	movement_feedback.text = "无法解散：" + message
	movement_feedback.show()

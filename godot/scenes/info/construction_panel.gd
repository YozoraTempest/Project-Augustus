# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceConstructionPanel
extends VBoxContainer

signal construction_requested(province_id: int, building: String)

@onready var province_label: Label = %BuildingProvinceName
@onready var completed_label: Label = %CompletedBuildings
@onready var project_box: VBoxContainer = %Project
@onready var project_label: Label = %ProjectDetails
@onready var progress_bar: ProgressBar = %ConstructionProgress
@onready var build_box: VBoxContainer = %Build
@onready var details_label: Label = %BuildingDetails
@onready var build_button: Button = %BuildButton
@onready var reason_label: Label = %BuildReason

var _province_id := 0
var _building := ""


func _ready() -> void:
	build_button.pressed.connect(_build)
	hide()


func show_province(province_id: int, info: Dictionary) -> void:
	_province_id = province_id
	visible = not info.is_empty()
	if not visible:
		return
	province_label.text = info.province_name + " · 建筑"
	completed_label.text = "已建建筑：" + ("无" if info.buildings.is_empty() else "、".join(info.buildings))
	project_box.visible = info.project != null
	if info.project != null:
		var project: Dictionary = info.project
		project_label.text = "正在建设：%s\n开工：%s\n完工：%s · 剩余 %d 天" % [project.name, project.started_on, project.completes_on, project.days_remaining]
		progress_bar.value = project.progress
	build_box.visible = not info.options.is_empty()
	if info.options.is_empty():
		return
	# 首版只有一种可新建税收建筑；报价与可建造原因均由 Rust 返回。
	var option: Dictionary = info.options[0]
	_building = option.id
	details_label.text = "%s · 税收加成 +%s\n费用：%s 金币 · 工期 %d 个月\n月税：%s → %s 金币\n增加：+%s 金币／月" % [option.name, option.tax_bonus, option.cost, option.months, option.tax_before, option.tax_after, option.tax_gain]
	build_button.text = "建造 " + option.name
	build_button.disabled = not option.can_build
	reason_label.text = option.reason
	reason_label.visible = not option.reason.is_empty()
	if option.completes_on != null:
		build_button.tooltip_text = "立即扣款，预计 %s 完工；完工后计入后续月度税收。" % option.completes_on


func _build() -> void:
	construction_requested.emit(_province_id, _building)


func show_error(error: String) -> void:
	reason_label.text = error
	reason_label.show()

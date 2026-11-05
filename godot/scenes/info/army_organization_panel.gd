# SPDX-License-Identifier: GPL-3.0-only
class_name ArmyOrganizationPanel
extends VBoxContainer

signal armies_merge_requested(target: int, sources: PackedInt64Array)
signal army_split_requested(army_id: int, count: int)

@onready var candidates_box: VBoxContainer = %MergeCandidates
@onready var empty_label: Label = %NoMergeCandidates
@onready var merge_button: Button = %MergeArmies
@onready var split_count: SpinBox = %SplitCount
@onready var split_button: Button = %SplitArmy
@onready var feedback: Label = %OrganizationFeedback

var _info: Dictionary = {}
var _checked: Dictionary = {}


func _ready() -> void:
	merge_button.pressed.connect(_merge_selected)
	split_button.pressed.connect(_split_selected)
	hide()


func show_info(info: Dictionary) -> void:
	# 日期刷新不重建相同的编组表单，保留勾选和正在输入的拆分数量。
	if info == _info:
		return
	var target_changed: bool = info.get("id", 0) != _info.get("id", 0)
	_info = info
	visible = not info.is_empty()
	feedback.hide()
	if target_changed:
		_checked.clear()
		split_count.value = 1
	if not visible:
		return
	var eligible := PackedInt64Array()
	for candidate: Dictionary in info.candidates:
		eligible.append(candidate.id)
	for id: int in _checked.keys():
		if not eligible.has(id):
			_checked.erase(id)
	for child in candidates_box.get_children():
		candidates_box.remove_child(child)
		child.queue_free()
	for candidate: Dictionary in info.candidates:
		var id: int = candidate.id
		var checkbox := CheckBox.new()
		checkbox.text = "第 %d 军队 · %d 团 · %d 人" % [id, candidate.regiment_count, candidate.strength]
		checkbox.tooltip_text = candidate.name
		checkbox.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		checkbox.set_pressed_no_signal(_checked.has(id))
		checkbox.toggled.connect(func(pressed: bool): _toggle_source(id, pressed))
		candidates_box.add_child(checkbox)
	empty_label.visible = eligible.is_empty()
	merge_button.disabled = _checked.is_empty()
	split_count.max_value = maxi(1, int(info.regiment_count) - 1)
	split_count.editable = info.can_split
	split_button.disabled = not info.can_split
	split_button.tooltip_text = info.split_reason
	split_count.tooltip_text = info.split_reason


func _toggle_source(id: int, pressed: bool) -> void:
	if pressed:
		_checked[id] = true
	else:
		_checked.erase(id)
	merge_button.disabled = _checked.is_empty()


func _merge_selected() -> void:
	feedback.hide()
	var sources := PackedInt64Array()
	for candidate: Dictionary in _info.candidates:
		if _checked.has(candidate.id):
			sources.append(candidate.id)
	armies_merge_requested.emit(_info.id, sources)


func _split_selected() -> void:
	feedback.hide()
	# 提交尚未失焦的输入，再使用 SpinBox 约束后的整数值。
	split_count.apply()
	army_split_requested.emit(_info.id, int(split_count.value))


func show_error(message: String) -> void:
	feedback.text = "编组失败：" + message
	feedback.show()

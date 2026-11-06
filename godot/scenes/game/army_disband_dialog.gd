# SPDX-License-Identifier: GPL-3.0-only
extends ConfirmationDialog

signal army_disband_requested(army_id: int)

var _army_id := 0
var _opened := false
var _was_paused := false


func _ready() -> void:
	confirmed.connect(_confirm)
	visibility_changed.connect(func():
		if not visible:
			_restore_processing())


func open_for_army(backend: AugustusMap, army_id: int) -> String:
	var info := backend.get_army_disband_info(army_id)
	if not info.error.is_empty():
		return info.error
	# 固定本次确认的军队编号，防止界面刷新后误操作另一支军队。
	_army_id = army_id
	dialog_text = "%s\n驻扎：%s\n解散：%d 团 · %d 人\n全国月度维护费减少：%s 金币\n返还人力：%d 人\n招募费用不返还" % [info.name, info.province_name, info.regiment_count, info.strength, info.monthly_maintenance, info.manpower_returned]
	_was_paused = get_tree().paused
	_opened = true
	get_tree().paused = true
	popup_centered()
	return ""


func _confirm() -> void:
	# 保持场景暂停直到命令完成，再关闭窗口并清除本次目标。
	army_disband_requested.emit(_army_id)
	hide()


func _restore_processing() -> void:
	if _opened:
		get_tree().paused = _was_paused
		_opened = false
		_army_id = 0


func _exit_tree() -> void:
	_restore_processing()

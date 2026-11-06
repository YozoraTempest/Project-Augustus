# SPDX-License-Identifier: GPL-3.0-only
extends ConfirmationDialog

signal confirmed_war(tag: String)

var _tag := ""
var _opened := false
var _was_paused := false


func _ready() -> void:
	confirmed.connect(_confirm)
	visibility_changed.connect(func():
		if not visible:
			_restore_processing())


func open_for_country(backend: AugustusMap, tag: String) -> String:
	var info := backend.get_country_info(tag)
	if info.is_empty():
		return "请选择有效的国家"
	if not info.can_declare_war:
		return str(info.declare_war_reason)
	# 固定本次确认的国家，避免界面刷新后向另一个国家宣战。
	_tag = tag
	var date: String = backend.get_clock_state().date
	dialog_text = "向 %s（%s）宣战？\n开始日期：%s\n此后可以进入对方领土，不能穿过未参战国家。" % [info.name, info.tag, date]
	_was_paused = get_tree().paused
	_opened = true
	get_tree().paused = true
	popup_centered()
	return ""


func _confirm() -> void:
	confirmed_war.emit(_tag)
	hide()


func _restore_processing() -> void:
	if _opened:
		get_tree().paused = _was_paused
		_opened = false
		_tag = ""


func _exit_tree() -> void:
	_restore_processing()

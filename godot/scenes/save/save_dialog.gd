# SPDX-License-Identifier: GPL-3.0-only
extends FileDialog
class_name GameSaveDialog

const SAVE_DIRECTORY := "user://saves"

var _was_paused := false
var _opened := false


func _ready() -> void:
	visibility_changed.connect(func():
		if not visible:
			_restore_processing())


func open_for_saving(suggested_name: String) -> String:
	return _open(FileDialog.FILE_MODE_SAVE_FILE, "保存游戏", suggested_name)


func open_for_loading() -> String:
	return _open(FileDialog.FILE_MODE_OPEN_FILE, "读取存档", "")


func _open(mode: FileDialog.FileMode, caption: String, suggested_name: String) -> String:
	var directory := ProjectSettings.globalize_path(SAVE_DIRECTORY)
	var error := DirAccess.make_dir_recursive_absolute(directory)
	if error != OK:
		return "无法打开存档目录：" + error_string(error)
	file_mode = mode
	title = caption
	current_dir = directory
	current_file = suggested_name
	# 选择文件时停止模拟；取消或关闭窗口后恢复之前的运行状态。
	_was_paused = get_tree().paused
	_opened = true
	get_tree().paused = true
	popup_centered_ratio(0.75)
	return ""


func _restore_processing() -> void:
	if _opened:
		get_tree().paused = _was_paused
		_opened = false


func _exit_tree() -> void:
	_restore_processing()

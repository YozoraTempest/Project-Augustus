# SPDX-License-Identifier: GPL-3.0-only
extends Node

# 显示偏好独立于游戏会话，返回主菜单或重新开局不会清除设置。
var config_path := "user://settings.cfg"
var window_mode: Window.Mode = Window.MODE_WINDOWED
var vsync_enabled := true


func _ready() -> void:
	var error := load_settings()
	if error != OK and error != ERR_FILE_NOT_FOUND:
		push_warning("无法读取设置：" + error_string(error))
	_apply_display()


func load_settings() -> Error:
	var config := ConfigFile.new()
	var error := config.load(config_path)
	if error != OK:
		return error
	window_mode = int(config.get_value("display", "window_mode", Window.MODE_WINDOWED)) as Window.Mode
	vsync_enabled = bool(config.get_value("display", "vsync", true))
	return OK


func save_and_apply(mode: Window.Mode, enable_vsync: bool) -> Error:
	var config := ConfigFile.new()
	config.set_value("display", "window_mode", mode)
	config.set_value("display", "vsync", enable_vsync)
	var error := config.save(config_path)
	if error != OK:
		return error
	# 保存成功后再应用，写入失败时仍保留当前显示状态。
	window_mode = mode
	vsync_enabled = enable_vsync
	_apply_display()
	return OK


func _apply_display() -> void:
	var window := get_window()
	# 窗口化偏好保留用户通过标题栏选择的最大化状态。
	if not (window_mode == Window.MODE_WINDOWED and window.mode == Window.MODE_MAXIMIZED):
		window.mode = window_mode
	var sync := DisplayServer.VSYNC_ENABLED if vsync_enabled else DisplayServer.VSYNC_DISABLED
	DisplayServer.window_set_vsync_mode(sync, window.get_window_id())

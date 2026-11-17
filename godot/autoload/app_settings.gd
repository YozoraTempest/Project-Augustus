# SPDX-License-Identifier: GPL-3.0-only
extends Node

# 显示偏好和 EU4 安装目录共用 user://settings.cfg，不另建文件。
# 返回主菜单或重新开局不会清除设置。
var config_path := "user://settings.cfg"
var window_mode: Window.Mode = Window.MODE_WINDOWED
var vsync_enabled := true
var eu4_directory := ""
var _config_error := OK


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
	eu4_directory = _normalize_directory(str(config.get_value("game", "eu4_directory", "")))
	return OK


func save_and_apply(mode: Window.Mode, enable_vsync: bool) -> Error:
	var config := _load_config()
	if config == null:
		return _config_error
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


func set_eu4_directory(path: String) -> Error:
	var config := _load_config()
	if config == null:
		return _config_error
	var cleaned := _normalize_directory(path)
	if cleaned.is_empty():
		config.erase_section_key("game", "eu4_directory")
	else:
		config.set_value("game", "eu4_directory", cleaned)
	var error := config.save(config_path)
	if error != OK:
		return error
	eu4_directory = cleaned
	# 开局页读取会话里的目录。没有正在进行的对局时，让它跟上刚保存的路径。
	if not GameSession.has_game():
		GameSession.game_directory = cleaned
	return OK


func _load_config() -> ConfigFile:
	var config := ConfigFile.new()
	var error := config.load(config_path)
	if error != OK and error != ERR_FILE_NOT_FOUND:
		_config_error = error
		return null
	_config_error = OK
	return config


func _normalize_directory(path: String) -> String:
	var cleaned := path.strip_edges().replace("\\", "/")
	while cleaned.ends_with("/"):
		cleaned = cleaned.trim_suffix("/")
	return cleaned


func _apply_display() -> void:
	var window := get_window()
	# 窗口化偏好保留用户通过标题栏选择的最大化状态。
	if not (window_mode == Window.MODE_WINDOWED and window.mode == Window.MODE_MAXIMIZED):
		window.mode = window_mode
	var sync := DisplayServer.VSYNC_ENABLED if vsync_enabled else DisplayServer.VSYNC_DISABLED
	DisplayServer.window_set_vsync_mode(sync, window.get_window_id())

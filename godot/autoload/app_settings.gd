# SPDX-License-Identifier: GPL-3.0-only
extends Node

# 显示偏好和 EU4 安装目录共用 user://settings.cfg，不另建文件。
# 返回主菜单或重新开局不会清除设置。
const LAUNCHER_SIZE := Vector2i(960, 600)
# 日常游戏窗口和界面调试都按这个尺寸。没单独保存分辨率时用它，不跟屏幕走。
const GAME_SIZE := Vector2i(1920, 1080)

var config_path := "user://settings.cfg"
var window_mode: Window.Mode = Window.MODE_WINDOWED
var vsync_enabled := true
var has_custom_resolution := false
var game_resolution := Vector2i.ZERO
var eu4_directory := ""
var _config_error := OK


func _ready() -> void:
	var error := load_settings()
	if error != OK and error != ERR_FILE_NOT_FOUND:
		push_warning("无法读取设置：" + error_string(error))
	# 启动器自己决定窗口大小。这里只恢复垂直同步，避免一启动就铺成游戏分辨率。
	_apply_vsync()


func load_settings() -> Error:
	var config := ConfigFile.new()
	var error := config.load(config_path)
	if error != OK:
		return error
	window_mode = int(config.get_value("display", "window_mode", Window.MODE_WINDOWED)) as Window.Mode
	vsync_enabled = bool(config.get_value("display", "vsync", true))
	var width := int(config.get_value("display", "width", 0))
	var height := int(config.get_value("display", "height", 0))
	has_custom_resolution = width > 0 and height > 0
	game_resolution = Vector2i(width, height) if has_custom_resolution else Vector2i.ZERO
	eu4_directory = _normalize_directory(str(config.get_value("game", "eu4_directory", "")))
	return OK


func save_and_apply(mode: Window.Mode, enable_vsync: bool, resolution: Vector2i) -> Error:
	var config := _load_config()
	if config == null:
		return _config_error
	config.set_value("display", "window_mode", mode)
	config.set_value("display", "vsync", enable_vsync)
	if resolution.x > 0 and resolution.y > 0:
		config.set_value("display", "width", resolution.x)
		config.set_value("display", "height", resolution.y)
	else:
		config.erase_section_key("display", "width")
		config.erase_section_key("display", "height")
	var error := config.save(config_path)
	if error != OK:
		return error
	# 保存成功后再应用到游戏窗口，写入失败时仍保留当前显示状态。
	window_mode = mode
	vsync_enabled = enable_vsync
	has_custom_resolution = resolution.x > 0 and resolution.y > 0
	game_resolution = resolution if has_custom_resolution else Vector2i.ZERO
	apply_game_window()
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


func current_screen_size() -> Vector2i:
	var window := get_window()
	var screen := window.current_screen if window != null else DisplayServer.window_get_current_screen()
	return DisplayServer.screen_get_size(screen)


func resolved_game_size() -> Vector2i:
	return game_resolution if has_custom_resolution else GAME_SIZE


func apply_launcher_window() -> void:
	var window := get_window()
	if window == null:
		return
	_apply_vsync()
	window.mode = Window.MODE_WINDOWED
	window.size = LAUNCHER_SIZE
	_center_on_current_screen(window, LAUNCHER_SIZE)


func apply_game_window() -> void:
	var window := get_window()
	if window == null:
		return
	_apply_vsync()
	if window_mode == Window.MODE_FULLSCREEN:
		window.mode = Window.MODE_FULLSCREEN
		return
	window.mode = Window.MODE_WINDOWED
	var resolution := resolved_game_size()
	window.size = resolution
	_center_on_current_screen(window, resolution)


func _apply_vsync() -> void:
	var window := get_window()
	if window == null:
		return
	var sync := DisplayServer.VSYNC_ENABLED if vsync_enabled else DisplayServer.VSYNC_DISABLED
	DisplayServer.window_set_vsync_mode(sync, window.get_window_id())


func _center_on_current_screen(window: Window, resolution: Vector2i) -> void:
	var screen := window.current_screen
	var origin := DisplayServer.screen_get_position(screen)
	var screen_size := DisplayServer.screen_get_size(screen)
	window.position = origin + (screen_size - resolution) / 2

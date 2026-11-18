# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 局内欧陆菜单。安装目录和扫描留在启动器。画面从已配置的欧陆目录读取，不把贴图放进仓库。
const COUNTRY_SELECT := "res://scenes/country_select/country_select.tscn"
const LOAD_SCENE := preload("res://scenes/launcher/opening_load.tscn")
const SETTINGS_SCENE := preload("res://scenes/settings/settings_dialog.tscn")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const FONT_SCRIPT := preload("res://ui/eu4/eu4_bitmap_font.gd")
const GAME_SCENE := "res://scenes/game/game.tscn"

@onready var background: TextureRect = %Background
@onready var logo: TextureRect = %Logo
@onready var studio_logo: TextureRect = %StudioLogo
@onready var bar: TextureRect = %Bar
@onready var new_game_button: Button = %NewGame
@onready var quit_button: Button = %Quit
@onready var settings_button: Button = %Settings
@onready var load_button: Button = %LoadGame
@onready var status_label: Label = %Status
@onready var save_dialog: GameSaveDialog = $SaveDialog

var _loading := false


func _ready() -> void:
	AppSettings.apply_game_window()
	new_game_button.pressed.connect(_start_game)
	quit_button.pressed.connect(func(): get_tree().quit())
	settings_button.pressed.connect(_open_settings)
	load_button.pressed.connect(_open_load_dialog)
	save_dialog.file_selected.connect(_load_saved_game)
	_dress_from_eu4()
	new_game_button.grab_focus()


func _start_game() -> void:
	if _loading:
		return
	if GameSession.has_game() and not GameSession.is_started():
		get_tree().change_scene_to_file(COUNTRY_SELECT)
		return
	var directory := GameSession.game_directory.strip_edges()
	if directory.is_empty():
		directory = AppSettings.eu4_directory.strip_edges()
	if directory.is_empty():
		_show_error("请先在启动器选择安装目录。")
		return
	_loading = true
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = true
	status_label.hide()
	var loader := LOAD_SCENE.instantiate()
	add_child(loader)
	var error: String = await loader.run(directory)
	if not is_inside_tree():
		return
	loader.queue_free()
	_loading = false
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = false
	if not error.is_empty():
		_show_error(error)
		return
	get_tree().change_scene_to_file(COUNTRY_SELECT)


func _open_settings() -> void:
	var dialog := SETTINGS_SCENE.instantiate()
	add_child(dialog)
	dialog.popup_centered()


func _open_load_dialog() -> void:
	var error: String = save_dialog.open_for_loading()
	if not error.is_empty():
		_show_error(error)


func _load_saved_game(path: String) -> void:
	if _loading:
		return
	_loading = true
	for button in [new_game_button, load_button, settings_button, quit_button]:
		button.disabled = true
	status_label.modulate = Color.WHITE
	status_label.text = "正在读取存档…"
	status_label.show()
	# 先显示载入状态，再读取本地内容并创建地图纹理。
	await get_tree().process_frame
	await get_tree().process_frame
	var error := GameSession.load_saved_game(path)
	if not error.is_empty():
		_loading = false
		for button in [new_game_button, load_button, settings_button, quit_button]:
			button.disabled = false
		_show_error("读取失败：" + error)
		return
	get_tree().change_scene_to_file(GAME_SCENE)


func _show_error(message: String) -> void:
	status_label.text = message
	status_label.modulate = Color(1.0, 0.55, 0.48)
	status_label.show()


func _dress_from_eu4() -> void:
	var directory := GameSession.game_directory.strip_edges()
	if directory.is_empty():
		directory = AppSettings.eu4_directory.strip_edges()
	if directory.is_empty() or not DirAccess.dir_exists_absolute(directory):
		_clear_art()
		return
	var session := AugustusMap.new()
	var loaded: Dictionary = session.load_eu4_ui_document(directory, "interface/frontend.gui")
	if loaded.get("ok", false) != true:
		_clear_art()
		return
	var assets = ASSETS_SCRIPT.new()
	assets.backend = session
	_show_texture(background, _loading_screen(directory, assets), true)
	_show_texture(logo, assets.texture_for("GFX_frontend_game_logo"), false)
	_show_texture(studio_logo, assets.texture_for("GFX_frontend_tinto_logo"), false)
	_show_texture(bar, assets.texture_for("GFX_frontend_bottom_small_bg"), false)
	var font = FONT_SCRIPT.new()
	var has_font: bool = font.load_vic22(directory)
	_paint_button(new_game_button, assets.texture_for("GFX_frontend_sp_button"), font if has_font else null)
	_paint_button(load_button, assets.texture_for("GFX_frontend_panel_button_left"), font if has_font else null)
	_paint_button(settings_button, assets.texture_for("GFX_frontend_panel_button_center"), font if has_font else null)
	_paint_button(quit_button, assets.texture_for("GFX_frontend_panel_button_right"), font if has_font else null)


func _clear_art() -> void:
	for rect in [background, logo, studio_logo, bar]:
		rect.texture = null
		rect.hide()


func _show_texture(rect: TextureRect, texture: Texture2D, cover: bool) -> void:
	if texture == null:
		rect.texture = null
		rect.hide()
		return
	rect.texture = texture
	rect.show()
	if cover:
		rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
		return
	var size := texture.get_size()
	if size.y > 140.0:
		size *= 140.0 / size.y
	rect.custom_minimum_size = size
	rect.size = size


func _paint_button(button: Button, texture: Texture2D, font) -> void:
	button.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	button.add_theme_color_override("font_hover_color", Color(1, 0.97, 0.88))
	if font != null and font.covers(button.text):
		var rendered: Texture2D = font.render(button.text)
		if rendered:
			button.icon = rendered
			button.expand_icon = false
			button.text = ""
	if texture == null:
		return
	var style := StyleBoxTexture.new()
	style.texture = texture
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		button.add_theme_stylebox_override(state, style)
	button.custom_minimum_size = texture.get_size()


func _loading_screen(directory: String, assets) -> Texture2D:
	var folder := directory.path_join("gfx/loadingscreens")
	var listing := DirAccess.open(folder)
	if listing == null:
		return null
	var paths: PackedStringArray = PackedStringArray()
	for file_name in listing.get_files():
		var lower := file_name.to_lower()
		if not lower.begins_with("load_") or not lower.ends_with(".dds"):
			continue
		var path := folder.path_join(file_name)
		# 原版从这组图里随机挑一张当主菜单背景。过大的和 DXT 先跳过，避免卡死。
		if _readable_background(path):
			paths.append(path)
	if paths.is_empty():
		return null
	return assets.texture_from_file(paths[randi() % paths.size()])


func _readable_background(path: String) -> bool:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return false
	var header := file.get_buffer(128)
	if header.size() < 128:
		return false
	var height := header.decode_u32(12)
	var width := header.decode_u32(16)
	var fourcc := header.slice(84, 88).get_string_from_ascii()
	if fourcc.contains("DXT"):
		return false
	return width > 0 and height > 0 and width <= 2560 and height <= 1600

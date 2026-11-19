# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 选国家挂 frontend.gui 这一个组件。lobby 里的控件按父子坐标留在组件内，不再逐个摆。
const LAUNCHER := "res://scenes/launcher/launcher.tscn"
const MAIN_MENU := "res://scenes/main_menu/main_menu.tscn"
const GAME_SCENE := "res://scenes/game/game.tscn"
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const Eu4Type := preload("res://ui/eu4/eu4_type.gd")

@onready var map_view: ProvinceMapView = %MapView

var _backend: AugustusMap
var _gui
var _chosen_country := ""
var _chrome: Control
var _flag: TextureRect
var _nation_labels: Array[Control] = []
var _detail_labels: Array[Control] = []
var _start: Button


func _ready() -> void:
	if not GameSession.has_game() or GameSession.is_started():
		var target := GAME_SCENE if GameSession.is_started() else LAUNCHER
		get_tree().change_scene_to_file.call_deferred(target)
		return
	AppSettings.apply_game_window()
	_backend = GameSession.backend
	_build_chrome()
	map_view.selection_changed.connect(_refresh_selection)
	_clear_country()
	await get_tree().process_frame
	if not is_inside_tree() or _backend == null:
		return
	map_view.present_for_country_pick()
	map_view.show_map(_backend)
	var pending := GameSession.startup_country
	GameSession.startup_country = ""
	if pending.is_empty():
		return
	map_view.select_country(pending)
	if not _chosen_country.is_empty():
		_enter_game()


func _build_chrome() -> void:
	var directory := str(_backend.get_game_directory())
	var gui = GUI_FILE.new()
	add_child(gui)
	if not gui.open(directory, "interface/frontend.gui", _backend):
		return
	gui.show_window("lobby")
	_gui = gui
	# 单人选国家：聊天、设计器、读档和联机面板整棵不画。还没选国家时只留 no_player。
	for hidden in ["bottom", "nation_designer_bottom_panel", "random_new_world_loading_window", "save_games", "multiplayer", "singleplayer"]:
		gui.set_named_visible(hidden, false)
	gui.action_requested.connect(_on_gui_action)
	_chrome = gui.focus as Control
	if _chrome == null:
		return
	_start = _find_named(_chrome, "play_button") as Button
	if _start:
		_start.disabled = true
	_nation_labels = _collect_named(_chrome, "selected_nation_label")
	_detail_labels = _collect_named(_chrome, "selected_fog")
	var parts := str(_backend.get_start_date()).split(".")
	var year_box := _find_named(_chrome, "year")
	if year_box is LineEdit and not parts.is_empty():
		year_box.text = parts[0]
	var date_label := _find_named(_chrome, "daymonth")
	if date_label is RichTextLabel and parts.size() >= 3:
		date_label.text = "%s.%s" % [parts[1], parts[2]]
	_attach_flag()
	_paint_loaded_text(_chrome)


func _on_gui_action(eu4_name: String, _properties: Dictionary) -> void:
	match eu4_name:
		"play_button":
			_enter_game()
		"back_button":
			_return_to_menu()
		"mapmode_terrain":
			map_view.set_surface_terrain(true)
		"mapmode_political":
			map_view.set_surface_terrain(false)


func _attach_flag() -> void:
	var shield := _find_named(_chrome, "player_shield") as Control
	if shield == null:
		return
	_flag = TextureRect.new()
	_flag.set_anchors_preset(Control.PRESET_FULL_RECT)
	_flag.offset_left = 0.0
	_flag.offset_top = 0.0
	_flag.offset_right = 0.0
	_flag.offset_bottom = 0.0
	_flag.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_flag.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_flag.mouse_filter = Control.MOUSE_FILTER_IGNORE
	shield.add_child(_flag)


func _paint_loaded_text(node: Node) -> void:
	if node is RichTextLabel:
		var size: int = node.get_theme_font_size("normal_font_size")
		Eu4Type.paint_rich_text(node, size)
	elif node is LineEdit:
		var size: int = node.get_theme_font_size("font_size")
		node.add_theme_font_override("font", Eu4Type.face_of(node))
		node.add_theme_font_size_override("font_size", size)
	elif node is Button:
		var size: int = node.get_theme_font_size("font_size")
		node.add_theme_font_override("font", Eu4Type.face_of(node))
		node.add_theme_font_size_override("font_size", size)
	for child in node.get_children():
		_paint_loaded_text(child)


func _find_named(node: Node, eu4_name: String) -> Node:
	if node != null and str(node.get_meta("eu4_name", "")) == eu4_name:
		return node
	if node == null:
		return null
	for child in node.get_children():
		var found := _find_named(child, eu4_name)
		if found:
			return found
	return null


func _collect_named(node: Node, eu4_name: String) -> Array[Control]:
	var found: Array[Control] = []
	_collect_into(node, eu4_name, found)
	return found


func _collect_into(node: Node, eu4_name: String, found: Array[Control]) -> void:
	if node is Control and str(node.get_meta("eu4_name", "")) == eu4_name:
		found.append(node)
	if node == null:
		return
	for child in node.get_children():
		_collect_into(child, eu4_name, found)


func _set_texts(labels: Array[Control], text: String) -> void:
	for label in labels:
		if label is RichTextLabel:
			label.text = text


func _refresh_selection() -> void:
	if _backend == null:
		return
	var tag := map_view.selected_country
	if tag.is_empty():
		_clear_country()
		return
	var info := _backend.get_country_info(tag)
	if info.is_empty():
		_clear_country()
		return
	if _flag:
		_flag.texture = _flag_texture(tag)
		_flag.visible = _flag.texture != null
	_set_texts(_nation_labels, str(info.name))
	var provinces := str(_backend.localise_eu4("PROVINCES"))
	if provinces.is_empty():
		provinces = "PROVINCES"
	_set_texts(_detail_labels, "%s %d" % [provinces, int(info.province_count)])
	if int(info.province_count) == 0:
		_chosen_country = ""
		_show_country_panel(false)
		if _start:
			_start.disabled = true
		var empty := str(_backend.localise_eu4("FE_NO_COUNTRY_SELECTED"))
		_set_texts(_nation_labels, empty if not empty.is_empty() else "FE_NO_COUNTRY_SELECTED")
		_set_texts(_detail_labels, "")
		return
	_chosen_country = tag
	_show_country_panel(true)
	if _start:
		_start.disabled = false


func _show_country_panel(selected: bool) -> void:
	if _gui == null:
		return
	_gui.set_named_visible("no_player", not selected)
	_gui.set_named_visible("singleplayer", selected)


func _flag_texture(tag: String) -> Texture2D:
	var directory := str(_backend.get_game_directory())
	var path := directory.path_join("gfx/flags").path_join(tag + ".tga")
	if not FileAccess.file_exists(path):
		return null
	var image := Image.new()
	if image.load(path) != OK:
		return null
	return _compose_shield(image)


func _compose_shield(flag: Image) -> Texture2D:
	var info: Dictionary = _backend.resolve_eu4_sprite("GFX_shield_thin")
	var assets = ASSETS_SCRIPT.new()
	var mask: Image
	var overlay: Image
	for key in ["absolute_path", "absolute_path_1", "absolute_path_2", "absolute_path_3"]:
		var texture_path := str(info.get(key, ""))
		if texture_path.is_empty():
			continue
		var loaded := _load_interface_image(assets, texture_path)
		if loaded == null:
			continue
		var file_name := texture_path.get_file().to_lower()
		if file_name.contains("mask"):
			mask = loaded
		elif file_name.contains("overlay") or file_name.contains("frame"):
			overlay = loaded
	if mask == null:
		return ImageTexture.create_from_image(flag)
	var width: int = mask.get_width()
	var height: int = mask.get_height()
	var source := flag.duplicate()
	source.resize(width, height, Image.INTERPOLATE_NEAREST)
	var image := Image.create(width, height, false, Image.FORMAT_RGBA8)
	for y in height:
		for x in width:
			var pixel: Color = source.get_pixel(x, y)
			var cover: Color = mask.get_pixel(x, y)
			image.set_pixel(x, y, Color(pixel.r, pixel.g, pixel.b, pixel.a * cover.a))
	if overlay != null:
		var frame := overlay
		if frame.get_width() != width or frame.get_height() != height:
			frame = overlay.duplicate()
			frame.resize(width, height, Image.INTERPOLATE_NEAREST)
		image.blend_rect(frame, Rect2i(0, 0, width, height), Vector2i.ZERO)
	return ImageTexture.create_from_image(image)


func _load_interface_image(assets, path: String) -> Image:
	var texture: Texture2D = assets.texture_from_file(path)
	if texture == null:
		return null
	return texture.get_image()


func _clear_country() -> void:
	_chosen_country = ""
	_show_country_panel(false)
	if _start:
		_start.disabled = true
	if _flag:
		_flag.texture = null
	var empty := str(_backend.localise_eu4("FE_NO_COUNTRY_SELECTED")) if _backend else ""
	_set_texts(_nation_labels, empty)
	_set_texts(_detail_labels, "")


func _enter_game() -> void:
	if _backend == null or _backend != GameSession.backend or _chosen_country.is_empty():
		return
	var error: String = GameSession.start_game(_chosen_country)
	if not error.is_empty():
		return
	GameSession.map_projection = MapMode.ViewType.FLAT
	get_tree().change_scene_to_file(GAME_SCENE)


func _return_to_menu() -> void:
	get_tree().change_scene_to_file(MAIN_MENU)

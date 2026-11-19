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
var _bookmarks: Array = []
var _bookmark_rows: Array[Button] = []
var _shield_row: Control


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
	# 单人：说明板要留着。聊天、设计器提示、联机页签、观察模式和调试地图按钮不画。
	for hidden in ["nation_designer_bottom_panel", "random_new_world_loading_window", "save_games", "multiplayer", "singleplayer", "open_music_player", "nation_designer_hint_bg", "nation_designer_hint", "country_tab", "players_tab", "country_tab_title", "players_tab_title", "mapmode_expansion", "mapmode_content", "mapmode_unit", "mapmode_music", "observe_mode_button", "observe_mode_title", "lobby_chat_edit", "largefonts_lobby_chat_edit", "speech_enabled_lobby_chat_edit", "speech_enabled_largefonts_lobby_chat_edit", "speech_enabled_lobby_chat_button", "mic_button", "lobby_chat_button", "chatlog", "enabled_dlc_list_0", "enabled_dlc_list_1"]:
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
		_seat_year(year_box)
	var date_label := _find_named(_chrome, "daymonth")
	if date_label is RichTextLabel and parts.size() >= 3:
		date_label.text = "%s.%s" % [parts[1], parts[2]]
	_attach_flag()
	_paint_loaded_text(_chrome)
	_fill_lobby()


# 年份数字按 vic_22s 的基线落进编辑框上沿的字盒，不压住框下沿的箭头。
func _seat_year(edit: LineEdit) -> void:
	var info: Dictionary = _backend.resolve_eu4_sprite("vic_22s")
	var fnt_path := str(info.get("absolute_path", ""))
	var page_path := str(info.get("absolute_path_1", ""))
	if fnt_path.is_empty() or page_path.is_empty():
		return
	var assets = ASSETS_SCRIPT.new()
	var texture: Texture2D = assets.texture_from_file(page_path)
	if texture == null:
		return
	var page := texture.get_image()
	var font: Font = preload("res://ui/eu4/eu4_bitmap_font.gd").font_from_page(fnt_path, page)
	if font == null:
		return
	var face := int(font.get_meta("eu4_face", 22))
	var line_height := int(font.get_meta("eu4_line_height", face))
	edit.add_theme_font_override("font", font)
	edit.add_theme_font_size_override("font_size", face)
	edit.add_theme_color_override("font_color", Color(0.96, 0.91, 0.78))
	edit.alignment = HORIZONTAL_ALIGNMENT_LEFT
	edit.clip_contents = true
	var box_h := edit.size.y
	if box_h < 1.0:
		box_h = 30.0
	# 编辑框没有欧陆底图，不铺纯黑。边距只用来把字行留在框的上沿。
	var style := StyleBoxEmpty.new()
	style.content_margin_left = 0.0
	style.content_margin_top = 0.0
	style.content_margin_right = 0.0
	style.content_margin_bottom = maxf(box_h - float(line_height), 0.0)
	for state in ["normal", "focus", "read_only"]:
		edit.add_theme_stylebox_override(state, style)


func _fill_lobby() -> void:
	_fill_start_tabs()
	_fill_bookmarks()
	_fill_dlc_icons()


func _fill_start_tabs() -> void:
	# OverlappingElementsBoxType 的大小写和解析器不一致，控件没有建出来。
	# 页签仍放在文件坐标 (32, 49)、250×50 的那一块里，不挪左栏。
	var left := _find_named(_chrome, "left") as Control
	if left == null:
		return
	var selected := _sprite_frame("GFX_start_type_option_bg", 1)
	var idle := _sprite_frame("GFX_start_type_option_bg", 2)
	var bookmarks_icon := _sprite_frame("GFX_start_type_option_icon", 1)
	var saves_icon := _sprite_frame("GFX_start_type_option_icon", 2)
	var tab_w := 80.0
	var gap := 2.0
	var origin := Vector2(32, 49) + Vector2((250.0 - tab_w * 2.0 - gap) * 0.5, 7)
	var bookmarks_tab := _tab_button(selected, bookmarks_icon, origin)
	var saves_tab := _tab_button(idle, saves_icon, origin + Vector2(tab_w + gap, 0))
	left.add_child(bookmarks_tab)
	left.add_child(saves_tab)
	bookmarks_tab.pressed.connect(func() -> void:
		_gui.set_named_visible("bookmarks", true)
		_gui.set_named_visible("save_games", false)
		_set_tab_style(bookmarks_tab, selected)
		_set_tab_style(saves_tab, idle)
	)
	saves_tab.pressed.connect(func() -> void:
		_gui.set_named_visible("bookmarks", false)
		_gui.set_named_visible("save_games", true)
		_set_tab_style(bookmarks_tab, idle)
		_set_tab_style(saves_tab, selected)
	)


func _tab_button(background: Texture2D, icon: Texture2D, at: Vector2) -> Button:
	var button := Button.new()
	button.position = at
	button.size = Vector2(80, 35)
	button.custom_minimum_size = Vector2(80, 35)
	button.focus_mode = Control.FOCUS_NONE
	button.flat = false
	button.z_index = 2
	_set_tab_style(button, background)
	if icon:
		var mark := TextureRect.new()
		mark.texture = icon
		mark.mouse_filter = Control.MOUSE_FILTER_IGNORE
		mark.set_anchors_preset(Control.PRESET_FULL_RECT)
		mark.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		mark.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		button.add_child(mark)
	return button


func _set_tab_style(button: Button, texture: Texture2D) -> void:
	if texture == null:
		return
	var style := StyleBoxTexture.new()
	style.texture = texture
	for state in ["normal", "hover", "pressed", "focus"]:
		button.add_theme_stylebox_override(state, style)


func _fill_bookmarks() -> void:
	var list := _find_named(_chrome, "bookmarks_list")
	if list == null:
		return
	var items := list.get_node_or_null("Items") as VBoxContainer
	if items == null:
		return
	if list is Control:
		list.clip_contents = true
	items.add_theme_constant_override("separation", 0)
	_bookmarks = _read_bookmarks()
	_bookmark_rows.clear()
	var background := _sprite_frame("GFX_lobby_list_entry", 1)
	var face := _ui_font("vic_18")
	var chosen := 0
	for index in _bookmarks.size():
		var entry: Dictionary = _bookmarks[index]
		if bool(entry.get("default", false)):
			chosen = index
		var row := _bookmark_row(entry, background, face)
		var captured := index
		row.pressed.connect(func() -> void: _select_bookmark(captured))
		items.add_child(row)
		_bookmark_rows.append(row)
	if not _bookmarks.is_empty():
		_select_bookmark(chosen)


func _bookmark_row(entry: Dictionary, background: Texture2D, face: Font) -> Button:
	var row := Button.new()
	row.flat = false
	row.focus_mode = Control.FOCUS_NONE
	row.custom_minimum_size = Vector2(240, 40)
	row.alignment = HORIZONTAL_ALIGNMENT_LEFT
	if background:
		var style := StyleBoxTexture.new()
		style.texture = background
		for state in ["normal", "hover", "pressed", "focus"]:
			row.add_theme_stylebox_override(state, style)
	var title := _bookmark_label(str(entry.get("title", "")), face, Vector2(8, 1))
	var when := _bookmark_label(str(entry.get("when", "")), face, Vector2(8, 18))
	row.add_child(title)
	row.add_child(when)
	return row


func _bookmark_label(text: String, face: Font, at: Vector2) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", Color(0.93, 0.9, 0.8))
	if face:
		label.add_theme_font_override("font", face)
		var size := int(face.get_meta("eu4_face", 18))
		label.add_theme_font_size_override("font_size", size)
	return label


func _select_bookmark(index: int) -> void:
	if index < 0 or index >= _bookmarks.size():
		return
	for row in _bookmark_rows:
		row.modulate = Color(0.78, 0.78, 0.78)
	if index < _bookmark_rows.size():
		_bookmark_rows[index].modulate = Color.WHITE
	var entry: Dictionary = _bookmarks[index]
	var description := _find_named(_chrome, "scenario_description") as RichTextLabel
	if description:
		description.text = str(entry.get("body", ""))
	_show_bookmark_shields(entry.get("countries", []))


func _show_bookmark_shields(countries: Array) -> void:
	var bottom := _find_named(_chrome, "bottom") as Control
	if bottom == null:
		return
	if _shield_row == null:
		_shield_row = Control.new()
		_shield_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
		bottom.get_parent().add_child(_shield_row)
	for child in _shield_row.get_children():
		child.queue_free()
	var shield := 64.0
	var gap := 6.0
	var count := countries.size()
	if count == 0:
		return
	var row_w := count * shield + float(count - 1) * gap
	var panel_w := bottom.size.x if bottom.size.x > 1.0 else 760.0
	_shield_row.position = Vector2(bottom.position.x + (panel_w - row_w) * 0.5, bottom.position.y - shield - 8.0)
	for index in count:
		var mark := TextureRect.new()
		mark.texture = _flag_texture(str(countries[index]), "GFX_shield_medium")
		mark.position = Vector2(index * (shield + gap), 0)
		mark.size = Vector2(shield, shield)
		mark.custom_minimum_size = Vector2(shield, shield)
		mark.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		mark.stretch_mode = TextureRect.STRETCH_SCALE
		mark.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_shield_row.add_child(mark)


func _fill_dlc_icons() -> void:
	var first := _list_items("enabled_dlc_list_0")
	var second := _list_items("enabled_dlc_list_1")
	if first == null or second == null:
		return
	first.add_theme_constant_override("separation", 2)
	second.add_theme_constant_override("separation", 2)
	var first_list := _find_named(_chrome, "enabled_dlc_list_0") as Control
	var second_list := _find_named(_chrome, "enabled_dlc_list_1") as Control
	if first_list:
		first_list.clip_contents = true
	if second_list:
		second_list.clip_contents = true
	var packs := _read_dlcs()
	var columns: Array[VBoxContainer] = [first, second]
	for index in packs.size():
		var icon := TextureRect.new()
		icon.texture = _sprite_frame(str(packs[index]), 1)
		icon.custom_minimum_size = Vector2(36, 40)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		columns[index % 2].add_child(icon)


func _list_items(eu4_name: String) -> VBoxContainer:
	var list := _find_named(_chrome, eu4_name)
	if list == null:
		return null
	return list.get_node_or_null("Items") as VBoxContainer


func _read_bookmarks() -> Array:
	var folder := str(_backend.get_game_directory()).path_join("common/bookmarks")
	var names := DirAccess.get_files_at(folder)
	var found: Array = []
	for file_name in names:
		if not str(file_name).ends_with(".txt"):
			continue
		var text := FileAccess.get_file_as_string(folder.path_join(file_name))
		if text.is_empty():
			continue
		var date := _plain_field(text, "date")
		var parts := date.split(".")
		var stamp := 0
		if parts.size() >= 3:
			stamp = int(parts[0]) * 10000 + int(parts[1]) * 100 + int(parts[2])
		var title_key := _quoted_field(text, "name")
		var desc_key := _quoted_field(text, "desc")
		var title := str(_backend.localise_eu4(title_key))
		if title.is_empty():
			title = title_key
		var body := str(_backend.localise_eu4(desc_key))
		if body.is_empty():
			body = desc_key
		found.append({
			"title": title,
			"when": _english_date(date),
			"body": body,
			"default": text.contains("default = yes"),
			"countries": _bookmark_countries(text),
			"stamp": stamp,
		})
	found.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return int(a.get("stamp", 0)) < int(b.get("stamp", 0)))
	return found


func _english_date(date: String) -> String:
	var parts := date.split(".")
	if parts.size() < 3:
		return date
	var months: PackedStringArray = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
	var month := int(parts[1])
	var month_name := parts[1]
	if month >= 1 and month <= 12:
		month_name = months[month - 1]
	var localised := str(_backend.localise_eu4(month_name))
	if not localised.is_empty():
		month_name = localised
	return "%02d %s %s" % [int(parts[2]), month_name, parts[0]]


func _bookmark_countries(text: String) -> Array:
	var tags: Array = []
	for raw_line in text.split("\n"):
		var line := raw_line.strip_edges()
		if line.begins_with("country") and not line.begins_with("easy_country"):
			var tag := _plain_field(line, "country")
			if not tag.is_empty():
				tags.append(tag)
	return tags


func _quoted_field(text: String, key: String) -> String:
	for raw_line in text.split("\n"):
		var line := raw_line.strip_edges()
		if not line.begins_with(key):
			continue
		var rest := line.substr(key.length()).strip_edges()
		if rest.begins_with("="):
			rest = rest.substr(1).strip_edges()
		var open := rest.find("\"")
		if open < 0:
			return ""
		var close := rest.find("\"", open + 1)
		if close < 0:
			return ""
		return rest.substr(open + 1, close - open - 1)
	return ""


func _plain_field(text: String, key: String) -> String:
	for raw_line in text.split("\n"):
		var line := raw_line.strip_edges()
		if not line.begins_with(key):
			continue
		var rest := line.substr(key.length()).strip_edges()
		if rest.begins_with("="):
			rest = rest.substr(1).strip_edges()
		return rest.get_slice(" ", 0).get_slice("#", 0).strip_edges()
	return ""


func _read_dlcs() -> PackedStringArray:
	var expansions := PackedStringArray([
		"GFX_ConquestofParadise", "GFX_WealthofNations", "GFX_ResPublica", "GFX_ArtofWar",
		"GFX_ElDorado", "GFX_CommonSense", "GFX_TheCossacks", "GFX_MareNostrum", "GFX_RightsofMan",
		"GFX_MandateofHeaven", "GFX_ThirdRome", "GFX_CradleofCivilization", "GFX_RuleBritannia",
		"GFX_Dharma", "GFX_GoldenCentury", "GFX_Emperor", "GFX_Leviathan", "GFX_Origins",
		"GFX_Lions_of_the_north", "GFX_Domination", "GFX_Kingofkings", "GFX_WindsOfChange",
	])
	var known := {}
	for sprite in expansions:
		known[_sprite_key(sprite.trim_prefix("GFX_"))] = sprite
	var folder := str(_backend.get_game_directory()).path_join("dlc")
	var major: Array[String] = []
	var other: Array[String] = []
	for directory in DirAccess.get_directories_at(folder):
		var pack := folder.path_join(directory)
		for file_name in DirAccess.get_files_at(pack):
			if not str(file_name).ends_with(".dlc"):
				continue
			var text := FileAccess.get_file_as_string(pack.path_join(file_name))
			var display := _quoted_field(text, "name")
			var category := _plain_field(text, "category")
			var key := _sprite_key(display)
			if known.has(key):
				major.append(str(known[key]))
			elif category == "music":
				other.append("GFX_music_pack_icon")
			elif category.contains("unit"):
				other.append("GFX_unit_pack_icon")
			else:
				other.append("GFX_content_pack_icon")
	var ordered := PackedStringArray()
	for sprite in expansions:
		if major.has(sprite):
			ordered.append(sprite)
	for sprite in other:
		ordered.append(sprite)
	return ordered


func _sprite_key(text: String) -> String:
	return text.to_lower().replace(" ", "").replace("_", "").replace("-", "").replace("of", "of")


func _sprite_frame(sprite_name: String, frame_number: int) -> Texture2D:
	var assets = ASSETS_SCRIPT.new()
	assets.backend = _backend
	var full: Texture2D = assets.texture_for(sprite_name)
	if full == null:
		return null
	var info: Dictionary = _backend.resolve_eu4_sprite(sprite_name)
	var raw: Variant = info.get("frames")
	var frames := 1
	if raw is int or raw is float:
		frames = maxi(int(raw), 1)
	if frames <= 1:
		return full
	var index := clampi(frame_number, 1, frames) - 1
	var frame_width := full.get_width() / frames
	var region := AtlasTexture.new()
	region.atlas = full
	region.region = Rect2(index * frame_width, 0, frame_width, full.get_height())
	return region


func _ui_font(sprite_name: String) -> Font:
	var info: Dictionary = _backend.resolve_eu4_sprite(sprite_name)
	var fnt_path := str(info.get("absolute_path", ""))
	var page_path := str(info.get("absolute_path_1", ""))
	if fnt_path.is_empty() or page_path.is_empty():
		return null
	var script := preload("res://ui/eu4/eu4_bitmap_font.gd")
	if page_path.to_lower().ends_with(".tga"):
		return script.atlas_font(fnt_path, page_path)
	var assets = ASSETS_SCRIPT.new()
	var texture: Texture2D = assets.texture_from_file(page_path)
	if texture == null:
		return null
	return script.font_from_page(fnt_path, texture.get_image())


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


func _flag_texture(tag: String, shield_sprite: String = "GFX_shield_thin") -> Texture2D:
	var directory := str(_backend.get_game_directory())
	var path := directory.path_join("gfx/flags").path_join(tag + ".tga")
	if not FileAccess.file_exists(path):
		return null
	var image := Image.new()
	if image.load(path) != OK:
		return null
	return _compose_shield(image, shield_sprite)


func _compose_shield(flag: Image, shield_sprite: String = "GFX_shield_thin") -> Texture2D:
	var info: Dictionary = _backend.resolve_eu4_sprite(shield_sprite)
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
	# 旗帜只铺在遮罩上，用遮罩 alpha 裁成盾形。框保持自己的像素尺寸，居中盖住旗帜，不放大去包住溢出。
	var canvas_w := mask.get_width()
	var canvas_h := mask.get_height()
	var origin := Vector2i.ZERO
	if overlay != null:
		canvas_w = overlay.get_width()
		canvas_h = overlay.get_height()
		origin = Vector2i((canvas_w - mask.get_width()) / 2, (canvas_h - mask.get_height()) / 2)
	var source := flag.duplicate()
	source.resize(mask.get_width(), mask.get_height(), Image.INTERPOLATE_NEAREST)
	var image := Image.create(canvas_w, canvas_h, false, Image.FORMAT_RGBA8)
	image.fill(Color(0, 0, 0, 0))
	for y in mask.get_height():
		for x in mask.get_width():
			var cover: Color = mask.get_pixel(x, y)
			if cover.a <= 0.0:
				continue
			var pixel: Color = source.get_pixel(x, y)
			var dx := origin.x + x
			var dy := origin.y + y
			if dx < 0 or dy < 0 or dx >= canvas_w or dy >= canvas_h:
				continue
			image.set_pixel(dx, dy, Color(pixel.r, pixel.g, pixel.b, pixel.a * cover.a))
	if overlay != null:
		image.blend_rect(overlay, Rect2i(0, 0, overlay.get_width(), overlay.get_height()), Vector2i.ZERO)
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

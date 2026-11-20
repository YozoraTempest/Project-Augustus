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
var _idea_tip: Control


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
	_raise_lobby_over_map()


# 大厅画在地图之上。整屏的空窗口仍把点击让给地图，面板、列表和图标自己接住。
func _raise_lobby_over_map() -> void:
	if _gui == null or not _gui is Control:
		return
	var chrome := _gui as Control
	chrome.z_index = 20
	_block_lobby_clicks(chrome)


func _block_lobby_clicks(node: Node) -> void:
	if node is Control and node != _gui:
		var control := node as Control
		var box: Vector2 = control.get_meta("eu4_box", Vector2.ZERO)
		var covers_screen := box.x >= 1919.0 and box.y >= 1079.0
		# top 没有底图，声明高 500，下半截盖住黑海北岸。横幅、按钮和那行字自己接点击。
		var banner_window := str(control.get_meta("eu4_name", "")) == "top"
		if banner_window or covers_screen:
			control.mouse_filter = Control.MOUSE_FILTER_IGNORE
		else:
			control.mouse_filter = Control.MOUSE_FILTER_STOP
	for child in node.get_children():
		_block_lobby_clicks(child)


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
	if list is ScrollContainer:
		_apply_bookmark_scrollbar(list)


# standardlistbox_slider：按钮和滑块用原图像素，轨道按九宫格拉高。
# 贴图像素：箭头和滑块 16×16，轨道 12×12、边 4。条自己的 borderSize 是 12。
class BookmarkScrollbar extends Control:
	const BUTTON := 16.0
	const TRACK_WIDTH := 12.0
	const END_INSET := 12.0
	const TRACK_BORDER := 4.0

	var host: ScrollContainer
	var up_texture: Texture2D
	var down_texture: Texture2D
	var knob_texture: Texture2D
	var track_texture: Texture2D
	var _dragging := false
	var _drag_offset := 0.0
	var _repeat_direction := 0.0
	var _repeat_wait := 0.0

	func _ready() -> void:
		mouse_filter = Control.MOUSE_FILTER_STOP
		focus_mode = Control.FOCUS_NONE
		set_process(false)

	func _limit() -> float:
		if host == null:
			return 0.0
		var bar := host.get_v_scroll_bar()
		return maxf(bar.max_value - bar.page, 0.0)

	func _line() -> float:
		var items := host.get_node_or_null("Items") as VBoxContainer
		if items != null and items.get_child_count() > 0:
			var row := items.get_child(0) as Control
			if row != null and row.size.y >= 1.0:
				return row.size.y
		return BUTTON

	func _knob_top() -> float:
		var span := maxf(size.y - BUTTON * 3.0, 0.0)
		var limit := _limit()
		var along := 0.0 if limit <= 0.0 else clampf(host.scroll_vertical / limit, 0.0, 1.0)
		return BUTTON + span * along

	func _set_scroll(pixels: float) -> void:
		host.scroll_vertical = clampf(pixels, 0.0, _limit())

	func _nudge(direction: float) -> void:
		_set_scroll(host.scroll_vertical + direction * _line())

	func _draw() -> void:
		var column_x := floorf((size.x - BUTTON) * 0.5)
		var track := Rect2(column_x + (BUTTON - TRACK_WIDTH) * 0.5, END_INSET, TRACK_WIDTH, maxf(size.y - END_INSET * 2.0, 0.0))
		_draw_sliced(track_texture, track, TRACK_BORDER)
		if up_texture:
			draw_texture(up_texture, Vector2(column_x, 0.0))
		if down_texture:
			draw_texture(down_texture, Vector2(column_x, size.y - BUTTON))
		if knob_texture:
			draw_texture(knob_texture, Vector2(column_x, _knob_top()))

	func _draw_sliced(texture: Texture2D, rect: Rect2, border: float) -> void:
		if texture == null or rect.size.x < 1.0 or rect.size.y <= border * 2.0:
			return
		var tex_w := float(texture.get_width())
		var tex_h := float(texture.get_height())
		var mid_src_w := tex_w - border * 2.0
		var mid_src_h := tex_h - border * 2.0
		var mid_dst_w := rect.size.x - border * 2.0
		var mid_dst_h := rect.size.y - border * 2.0
		var origin := rect.position
		_blit(texture, Rect2(origin, Vector2(border, border)), Rect2(0.0, 0.0, border, border))
		_blit(texture, Rect2(origin + Vector2(border, 0.0), Vector2(mid_dst_w, border)), Rect2(border, 0.0, mid_src_w, border))
		_blit(texture, Rect2(origin + Vector2(rect.size.x - border, 0.0), Vector2(border, border)), Rect2(tex_w - border, 0.0, border, border))
		_blit(texture, Rect2(origin + Vector2(0.0, border), Vector2(border, mid_dst_h)), Rect2(0.0, border, border, mid_src_h))
		_blit(texture, Rect2(origin + Vector2(border, border), Vector2(mid_dst_w, mid_dst_h)), Rect2(border, border, mid_src_w, mid_src_h))
		_blit(texture, Rect2(origin + Vector2(rect.size.x - border, border), Vector2(border, mid_dst_h)), Rect2(tex_w - border, border, border, mid_src_h))
		_blit(texture, Rect2(origin + Vector2(0.0, rect.size.y - border), Vector2(border, border)), Rect2(0.0, tex_h - border, border, border))
		_blit(texture, Rect2(origin + Vector2(border, rect.size.y - border), Vector2(mid_dst_w, border)), Rect2(border, tex_h - border, mid_src_w, border))
		_blit(texture, Rect2(origin + Vector2(rect.size.x - border, rect.size.y - border), Vector2(border, border)), Rect2(tex_w - border, tex_h - border, border, border))

	func _blit(texture: Texture2D, dest: Rect2, source: Rect2) -> void:
		if dest.size.x <= 0.0 or dest.size.y <= 0.0 or source.size.x <= 0.0 or source.size.y <= 0.0:
			return
		draw_texture_rect_region(texture, dest, source)

	func _gui_input(event: InputEvent) -> void:
		var button := event as InputEventMouseButton
		if button != null and button.button_index == MOUSE_BUTTON_LEFT:
			if button.pressed:
				var knob := Rect2((size.x - BUTTON) * 0.5, _knob_top(), BUTTON, BUTTON)
				if knob.has_point(button.position):
					_dragging = true
					_drag_offset = button.position.y - knob.position.y
					_stop_repeat()
				elif button.position.y < BUTTON:
					_begin_repeat(-1.0)
				elif button.position.y > size.y - BUTTON:
					_begin_repeat(1.0)
				elif button.position.y < knob.position.y:
					_nudge(-1.0)
				else:
					_nudge(1.0)
			else:
				_dragging = false
				_stop_repeat()
			accept_event()
			return
		if button != null and button.pressed and (button.button_index == MOUSE_BUTTON_WHEEL_UP or button.button_index == MOUSE_BUTTON_WHEEL_DOWN):
			var direction := -1.0 if button.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0
			_nudge(direction * maxf(button.factor, 1.0))
			accept_event()

	func _input(event: InputEvent) -> void:
		var button := event as InputEventMouseButton
		if button != null and button.button_index == MOUSE_BUTTON_LEFT and not button.pressed:
			_dragging = false
			_stop_repeat()
			return
		if _dragging and event is InputEventMouseMotion:
			var span := maxf(size.y - BUTTON * 3.0, 1.0)
			var top := get_local_mouse_position().y - _drag_offset
			var along := clampf((top - BUTTON) / span, 0.0, 1.0)
			_set_scroll(along * _limit())
			get_viewport().set_input_as_handled()

	func _process(delta: float) -> void:
		if _repeat_direction == 0.0:
			set_process(false)
			return
		_repeat_wait -= delta
		if _repeat_wait > 0.0:
			return
		_nudge(_repeat_direction)
		_repeat_wait = 0.05

	func _begin_repeat(direction: float) -> void:
		_repeat_direction = direction
		_repeat_wait = 0.35
		_nudge(direction)
		set_process(true)

	func _stop_repeat() -> void:
		_repeat_direction = 0.0
		set_process(false)


func _apply_bookmark_scrollbar(list: ScrollContainer) -> void:
	var bar := list.get_v_scroll_bar()
	var empty := StyleBoxEmpty.new()
	var styles := PackedStringArray(["scroll", "scroll_focus", "grabber", "grabber_highlight", "grabber_pressed"])
	for style_name in styles:
		bar.add_theme_stylebox_override(style_name, empty)
	bar.custom_minimum_size = Vector2(BookmarkScrollbar.BUTTON, 0.0)
	bar.modulate = Color(1, 1, 1, 0)
	bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	list.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_SHOW_ALWAYS
	var parent := list.get_parent()
	if parent == null:
		return
	var previous := parent.get_node_or_null("bookmark_scrollbar")
	if previous:
		previous.queue_free()
	var overlay := BookmarkScrollbar.new()
	overlay.name = "bookmark_scrollbar"
	overlay.host = list
	overlay.up_texture = _sprite_frame("GFX_scroll_up", 1)
	overlay.down_texture = _sprite_frame("GFX_scroll_down", 1)
	overlay.knob_texture = _sprite_frame("GFX_scroll_drager", 1)
	overlay.track_texture = _sprite_frame("GFX_scroll_track", 1)
	overlay.z_index = 2
	parent.add_child(overlay)
	_sync_bookmark_scrollbar(list, overlay)
	list.resized.connect(func() -> void: _sync_bookmark_scrollbar(list, overlay))
	bar.resized.connect(func() -> void: _sync_bookmark_scrollbar(list, overlay))
	bar.value_changed.connect(func(_value: float) -> void: overlay.queue_redraw())
	bar.changed.connect(overlay.queue_redraw)
	_sync_bookmark_scrollbar.call_deferred(list, overlay)


func _sync_bookmark_scrollbar(list: ScrollContainer, overlay: Control) -> void:
	if list.size.x < 1.0 or list.size.y < 1.0:
		return
	# 欧陆这列右缘在金框内侧，比列表框右缘再往右 16 像素。尺寸不变。
	overlay.position = list.position + Vector2(list.size.x - BookmarkScrollbar.BUTTON + 16.0, 0.0)
	overlay.size = Vector2(BookmarkScrollbar.BUTTON, list.size.y)
	overlay.queue_redraw()


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
	var panel := _find_named(_chrome, "singleplayer")
	var shield := _find_named(panel, "player_shield") as Control
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
	_show_panel_flag(tag)
	_set_texts(_nation_labels, str(info.name))
	var provinces := str(_backend.localise_eu4("PROVINCES"))
	if provinces.is_empty():
		provinces = "PROVINCES"
	_set_texts(_detail_labels, "%s %d" % [provinces, int(info.province_count)])
	if int(info.province_count) == 0:
		_clear_country_stats()
		_chosen_country = ""
		_show_country_panel(false)
		_hide_religion_icons()
		_clear_tech_government()
		_clear_diplomacy()
		_clear_opening_figures()
		_clear_national_ideas()
		if _start:
			_start.disabled = true
		var empty := str(_backend.localise_eu4("FE_NO_COUNTRY_SELECTED"))
		_set_texts(_nation_labels, empty if not empty.is_empty() else "FE_NO_COUNTRY_SELECTED")
		_set_texts(_detail_labels, "")
		return
	_chosen_country = tag
	_fill_country_stats(info)
	_apply_religion_icons(info)
	_apply_tech_government(info)
	_fill_diplomacy(info)
	_fill_opening_figures(info)
	_fill_national_ideas(info)
	_show_country_panel(true)
	if _start:
		_start.disabled = false


func _fill_national_ideas(info: Dictionary) -> void:
	_set_box("national_ideagroup_name", _text_field(info, "idea_group"))
	var paths := _idea_paths(info)
	var tips: Variant = info.get("national_idea_tips", null)
	var tip_list: Array = tips if tips is Array else []
	_set_box("ideas_value", str(paths.size()) if paths.size() > 0 else "")
	var empty := _sprite_frame("GFX_idea_empty", 1)
	for index in 7:
		var slot := _find_named(_chrome, "national_idea%d" % [index + 1]) as TextureRect
		if slot == null:
			continue
		if index < paths.size():
			var texture := _idea_texture(paths[index])
			if texture != null:
				slot.texture = texture
			elif empty != null:
				slot.texture = empty
		elif empty != null:
			slot.texture = empty
		_set_idea_tip(slot, tip_list[index] if index < tip_list.size() else null)


func _set_idea_tip(slot: TextureRect, tip: Variant) -> void:
	var title := ""
	var body := ""
	var effects := ""
	if tip is Dictionary:
		title = str(tip.get("name", ""))
		body = str(tip.get("desc", ""))
		effects = str(tip.get("effects", ""))
		if body == "<null>":
			body = ""
		if effects == "<null>":
			effects = ""
	if title.is_empty():
		slot.set_meta("idea_tip_title", "")
		slot.set_meta("idea_tip_body", "")
		slot.set_meta("idea_tip_effects", "")
		slot.tooltip_text = ""
		slot.mouse_filter = Control.MOUSE_FILTER_IGNORE
		return
	slot.set_meta("idea_tip_title", title)
	slot.set_meta("idea_tip_body", body)
	slot.set_meta("idea_tip_effects", effects)
	slot.tooltip_text = ""
	slot.mouse_filter = Control.MOUSE_FILTER_STOP
	if not slot.has_meta("idea_tip_bound"):
		slot.mouse_entered.connect(_show_idea_tip.bind(slot))
		slot.mouse_exited.connect(_hide_idea_tip)
		slot.set_meta("idea_tip_bound", true)


func _show_idea_tip(slot: Control) -> void:
	var title := str(slot.get_meta("idea_tip_title", ""))
	var effects := str(slot.get_meta("idea_tip_effects", ""))
	var body := str(slot.get_meta("idea_tip_body", ""))
	if title.is_empty():
		_hide_idea_tip()
		return
	var text := title
	if not effects.is_empty():
		text += "\n" + effects
	if not body.is_empty():
		text += "\n" + body
	var tip := _idea_tip_panel()
	var label := tip.get_node("Text") as RichTextLabel
	label.text = text
	var face := _ui_font("vic_18")
	var font_size := 18
	if face:
		font_size = int(face.get_meta("eu4_face", 18))
		label.add_theme_font_override("normal_font", face)
		label.add_theme_font_size_override("normal_font_size", font_size)
	# core.gui 的 ToolTip：宽最多 400，边 18。frontend 的 tooltip_offset_small 是 (21, 24)。
	var text_width := 364.0
	var measured := Vector2(text_width, float(font_size))
	if face:
		measured = face.get_multiline_string_size(label.text, HORIZONTAL_ALIGNMENT_LEFT, text_width, font_size)
	var content_w := clampf(measured.x, 32.0, text_width)
	var content_h := maxf(measured.y, float(font_size))
	label.position = Vector2(18, 18)
	label.size = Vector2(content_w, content_h)
	tip.size = Vector2(content_w + 36.0, content_h + 36.0)
	var mouse := get_global_mouse_position()
	var pos := mouse + Vector2(21, 24)
	var view := get_viewport_rect().size
	if pos.x + tip.size.x > view.x:
		pos.x = maxf(0.0, mouse.x - tip.size.x - 21.0)
	if pos.y + tip.size.y > view.y:
		pos.y = maxf(0.0, mouse.y - tip.size.y - 24.0)
	tip.position = pos
	tip.show()


func _hide_idea_tip() -> void:
	if _idea_tip:
		_idea_tip.hide()


func _idea_tip_panel() -> Control:
	if _idea_tip:
		return _idea_tip
	var tip := Control.new()
	tip.name = "IdeaToolTip"
	tip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	tip.z_index = 100
	tip.hide()
	var patch := NinePatchRect.new()
	patch.set_anchors_preset(Control.PRESET_FULL_RECT)
	patch.mouse_filter = Control.MOUSE_FILTER_IGNORE
	patch.patch_margin_left = 18
	patch.patch_margin_top = 18
	patch.patch_margin_right = 18
	patch.patch_margin_bottom = 18
	var path := str(_backend.get_game_directory()).path_join("gfx/interface/tiles_dialog.dds")
	var assets = ASSETS_SCRIPT.new()
	patch.texture = assets.texture_from_file(path)
	tip.add_child(patch)
	var label := RichTextLabel.new()
	label.name = "Text"
	label.bbcode_enabled = false
	label.scroll_active = false
	label.fit_content = false
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	tip.add_child(label)
	add_child(tip)
	_idea_tip = tip
	return tip


func _idea_paths(info: Dictionary) -> PackedStringArray:
	var paths := PackedStringArray()
	var icons: Variant = info.get("national_idea_icons", null)
	if icons is PackedStringArray:
		return icons
	if icons is Array:
		for item in icons:
			paths.append(str(item))
	return paths


func _clear_national_ideas() -> void:
	_set_box("national_ideagroup_name", "")
	_set_box("ideas_value", "")
	var empty := _sprite_frame("GFX_idea_empty", 1)
	if empty == null:
		return
	for index in 7:
		var slot := _find_named(_chrome, "national_idea%d" % [index + 1]) as TextureRect
		if slot:
			slot.texture = empty
			slot.tooltip_text = ""
			slot.set_meta("idea_tip_title", "")
			slot.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_hide_idea_tip()


func _idea_texture(relative: String) -> Texture2D:
	if _backend == null or relative.is_empty():
		return null
	var path := str(_backend.get_game_directory()).path_join(relative)
	var assets = ASSETS_SCRIPT.new()
	return assets.texture_from_file(path)


func _apply_tech_government(info: Dictionary) -> void:
	var frame := int(info.get("tech_group_icon", 0))
	if frame < 1:
		frame = 1
	_paint_icon_frame("techgroup_icon", "GFX_tech_group_strip", frame)
	_bind_named_tip("techgroup_icon", _text_field(info, "tech_group_name"))
	_set_box("selected_nation_status_label", _text_field(info, "government_name"))


func _clear_tech_government() -> void:
	_paint_icon_frame("techgroup_icon", "GFX_tech_group_strip", 1)
	_bind_named_tip("techgroup_icon", "")
	_set_box("selected_nation_status_label", "")
	_hide_idea_tip()


func _paint_icon_frame(eu4_name: String, sprite_name: String, frame: int) -> void:
	var icon := _find_named(_chrome, eu4_name) as TextureRect
	if icon == null:
		return
	var texture := _sprite_frame(sprite_name, frame)
	if texture == null:
		return
	icon.texture = texture
	icon.visible = true


func _bind_named_tip(eu4_name: String, title: String) -> void:
	var slot := _find_named(_chrome, eu4_name) as TextureRect
	if slot == null:
		return
	slot.tooltip_text = ""
	if title.is_empty():
		slot.set_meta("idea_tip_title", "")
		slot.set_meta("idea_tip_body", "")
		slot.set_meta("idea_tip_effects", "")
		return
	slot.set_meta("idea_tip_title", title)
	slot.set_meta("idea_tip_body", "")
	slot.set_meta("idea_tip_effects", "")
	slot.mouse_filter = Control.MOUSE_FILTER_STOP
	if not slot.has_meta("idea_tip_bound"):
		slot.mouse_entered.connect(_show_idea_tip.bind(slot))
		slot.mouse_exited.connect(_hide_idea_tip)
		slot.set_meta("idea_tip_bound", true)


func _fill_diplomacy(info: Dictionary) -> void:
	var items := _list_items("diplomatic_listbox")
	if items == null:
		return
	_clear_list(items)
	var lines: Variant = info.get("diplomacy", [])
	if not lines is Array:
		return
	items.add_theme_constant_override("separation", 1)
	var document := _diplomacy_item_document()
	for entry in lines:
		if entry is Dictionary:
			var row := _diplomacy_row(document, entry)
			if row:
				items.add_child(row)
	var list := _find_named(_chrome, "diplomatic_listbox") as ScrollContainer
	if list:
		list.clip_contents = true
		_apply_diplomacy_scrollbar(list)


class DiplomacyBar extends BookmarkScrollbar:
	var image: Image

	func _draw() -> void:
		if image == null:
			return
		for y in image.get_height():
			for x in image.get_width():
				var color := image.get_pixel(x, y)
				if color.a < 0.75:
					continue
				draw_rect(Rect2(x, y, 1, 1), Color(color.r, color.g, color.b, 1.0), true)


func _sprite_image(sprite_name: String) -> Image:
	var texture := _sprite_frame(sprite_name, 1)
	if texture == null:
		return null
	var image := texture.get_image()
	if image == null or image.is_empty():
		return null
	return image


func _bake_diplomacy_scrollbar(list: ScrollContainer, height: int) -> Image:
	var image := Image.create(int(BookmarkScrollbar.BUTTON), height, false, Image.FORMAT_RGBA8)
	image.fill(Color(0, 0, 0, 0))
	var track := _sprite_image("GFX_scroll_track")
	var border := int(BookmarkScrollbar.TRACK_BORDER)
	var track_left := int((BookmarkScrollbar.BUTTON - BookmarkScrollbar.TRACK_WIDTH) * 0.5)
	var track_top := int(BookmarkScrollbar.END_INSET)
	var track_height := height - int(BookmarkScrollbar.END_INSET * 2.0)
	if track != null and track_height > border * 2:
		var mid := track.get_region(Rect2i(0, border, track.get_width(), track.get_height() - border * 2))
		mid.resize(int(BookmarkScrollbar.TRACK_WIDTH), track_height - border * 2, Image.INTERPOLATE_NEAREST)
		image.blend_rect(track, Rect2i(0, 0, track.get_width(), border), Vector2i(track_left, track_top))
		image.blend_rect(mid, Rect2i(0, 0, mid.get_width(), mid.get_height()), Vector2i(track_left, track_top + border))
		image.blend_rect(track, Rect2i(0, track.get_height() - border, track.get_width(), border), Vector2i(track_left, track_top + track_height - border))
	var up := _sprite_image("GFX_scroll_up")
	if up != null:
		image.blend_rect(up, Rect2i(0, 0, up.get_width(), up.get_height()), Vector2i(0, 0))
	var down := _sprite_image("GFX_scroll_down")
	if down != null:
		image.blend_rect(down, Rect2i(0, 0, down.get_width(), down.get_height()), Vector2i(0, height - down.get_height()))
	var knob := _sprite_image("GFX_scroll_drager")
	if knob != null:
		var bar := list.get_v_scroll_bar()
		var limit := maxf(bar.max_value - bar.page, 0.0)
		var span := maxf(float(height) - BookmarkScrollbar.BUTTON * 3.0, 0.0)
		var along := 0.0 if limit <= 0.0 else clampf(list.scroll_vertical / limit, 0.0, 1.0)
		var knob_y := int(BookmarkScrollbar.BUTTON + span * along)
		image.blend_rect(knob, Rect2i(0, 0, knob.get_width(), knob.get_height()), Vector2i(0, knob_y))
	return image


func _apply_diplomacy_scrollbar(list: ScrollContainer) -> void:
	var bar := list.get_v_scroll_bar()
	var empty := StyleBoxEmpty.new()
	var styles := PackedStringArray(["scroll", "scroll_focus", "grabber", "grabber_highlight", "grabber_pressed"])
	for style_name in styles:
		bar.add_theme_stylebox_override(style_name, empty)
	bar.custom_minimum_size = Vector2(BookmarkScrollbar.BUTTON, 0.0)
	bar.modulate = Color(1, 1, 1, 0)
	bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	list.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_SHOW_ALWAYS
	list.clip_contents = true
	# 挂到选国家根上，盖过后面画的界面。放不下的行留在名单里，靠这条滚动。
	var column := get_node_or_null("diplomacy_scrollbar") as DiplomacyBar
	if column == null:
		column = DiplomacyBar.new()
		column.name = "diplomacy_scrollbar"
		column.mouse_filter = Control.MOUSE_FILTER_STOP
		column.z_as_relative = false
		column.z_index = 4095
		add_child(column)
		list.resized.connect(_sync_diplomacy_scrollbar.bind(list, column))
		bar.value_changed.connect(_redraw_diplomacy_scrollbar.bind(list, column))
	column.visible = true
	column.host = list
	column.move_to_front()
	_sync_diplomacy_scrollbar(list, column)
	_sync_diplomacy_scrollbar.call_deferred(list, column)


func _redraw_diplomacy_scrollbar(_value: float, list: ScrollContainer, column: Control) -> void:
	_sync_diplomacy_scrollbar(list, column)


func _diplomacy_list_screen_rect(list: ScrollContainer) -> Rect2:
	var rect := list.get_global_rect()
	var origin := rect.position
	var width := rect.size.x
	var height := rect.size.y
	if width < 8.0:
		width = list.offset_right - list.offset_left
	if height < 8.0:
		height = list.offset_bottom - list.offset_top
	if width < 8.0 or height < 8.0:
		var items := list.get_node_or_null("Items") as Control
		if items != null and items.get_child_count() > 0 and items.get_child(0) is Control:
			var row := items.get_child(0) as Control
			var row_rect := row.get_global_rect()
			if row_rect.size.y >= 1.0:
				origin = row_rect.position
				if width < 8.0:
					width = 200.0
				if height < 8.0:
					height = 120.0
	if width < 8.0:
		width = 200.0
	if height < 8.0:
		height = 120.0
	return Rect2(origin, Vector2(width, height))


func _sync_diplomacy_scrollbar(list: ScrollContainer, column: Control) -> void:
	if not is_instance_valid(list) or not is_instance_valid(column):
		return
	list.clip_contents = true
	# 内容放得下也画整列：上箭头、12 像素轨道、16×16 滑块、下箭头。滑块不拉伸。
	var rect := _diplomacy_list_screen_rect(list)
	var host := column.get_parent() as Control
	column.visible = true
	column.z_as_relative = false
	column.z_index = 4095
	if host == null:
		column.global_position = rect.position + Vector2(rect.size.x - BookmarkScrollbar.BUTTON, 0.0)
	else:
		var local: Vector2 = host.get_global_transform().affine_inverse() * rect.position
		column.position = local + Vector2(rect.size.x - BookmarkScrollbar.BUTTON, 0.0)
	column.size = Vector2(BookmarkScrollbar.BUTTON, rect.size.y)
	var bar := column as DiplomacyBar
	if bar != null:
		bar.image = _bake_diplomacy_scrollbar(list, int(rect.size.y))
		bar.queue_redraw()


func _diplomacy_item_document() -> Dictionary:
	if _backend == null:
		return {}
	var loaded: Dictionary = _backend.load_eu4_ui_document(str(_backend.get_game_directory()), "interface/countrydiplomacyview.gui")
	if loaded.get("ok", false) != true:
		return {}
	var parsed: Variant = JSON.parse_string(str(loaded.get("json", "")))
	return parsed if parsed is Dictionary else {}


func _diplomacy_row(document: Dictionary, entry: Dictionary) -> Control:
	if document.is_empty():
		return null
	var assets = ASSETS_SCRIPT.new()
	assets.backend = _backend
	var builder = preload("res://ui/eu4/eu4_ui_builder.gd").new()
	builder.assets = assets
	builder.plain_windows = true
	var host := Control.new()
	var created: Array = builder.build_named(document, "countrydiplomacyview_diplistitem", host)
	if created.is_empty() or not created[0] is Control:
		host.free()
		return null
	var row := created[0] as Control
	host.remove_child(row)
	host.free()
	row.custom_minimum_size = Vector2(200, 38)
	row.mouse_filter = Control.MOUSE_FILTER_STOP
	var icon := _find_in(row, "icon") as TextureRect
	if icon:
		var texture := _sprite_or_dds(str(entry.get("icon", "")))
		if texture:
			icon.texture = texture
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var box := _find_in(row, "box") as Control
	if box:
		box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var shield := TextureRect.new()
	shield.name = "CountryShield"
	shield.texture = _flag_texture(str(entry.get("tag", "")), "GFX_shield_small")
	shield.mouse_filter = Control.MOUSE_FILTER_IGNORE
	shield.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	shield.stretch_mode = TextureRect.STRETCH_SCALE
	shield.z_index = 4
	var icon_right := 34.0
	if icon:
		icon_right = icon.position.x + maxf(icon.size.x, maxf(icon.custom_minimum_size.x, 33.0))
	shield.position = Vector2(icon_right + 2.0, 3.0)
	shield.size = Vector2(32, 32)
	shield.custom_minimum_size = Vector2(32, 32)
	shield.visible = shield.texture != null
	row.add_child(shield)
	var text := _find_in(row, "infotext") as RichTextLabel
	if text:
		text.text = str(entry.get("name", ""))
		text.mouse_filter = Control.MOUSE_FILTER_IGNORE
		text.z_index = 3
		text.position.x = shield.position.x + shield.size.x + 4.0
		text.size.x = maxf(200.0 - text.position.x - 18.0, 40.0)
	row.set_meta("idea_tip_title", str(entry.get("relation", "")))
	row.set_meta("idea_tip_body", str(entry.get("name", "")))
	row.set_meta("idea_tip_effects", "")
	row.mouse_entered.connect(_show_idea_tip.bind(row))
	row.mouse_exited.connect(_hide_idea_tip)
	return row


func _sprite_or_dds(sprite_name: String) -> Texture2D:
	if sprite_name.is_empty() or _backend == null:
		return null
	var assets = ASSETS_SCRIPT.new()
	assets.backend = _backend
	var texture := assets.texture_for(sprite_name)
	if texture:
		return texture
	var info: Dictionary = _backend.resolve_eu4_sprite(sprite_name)
	var path := str(info.get("absolute_path", ""))
	if path.to_lower().ends_with(".tga"):
		return assets.texture_from_file(path.substr(0, path.length() - 4) + ".dds")
	return null


func _find_in(node: Node, eu4_name: String) -> Node:
	if node != null and str(node.get_meta("eu4_name", "")) == eu4_name:
		return node
	if node == null:
		return null
	for child in node.get_children():
		var found := _find_in(child, eu4_name)
		if found:
			return found
	return null


func _clear_diplomacy() -> void:
	var overlay := get_node_or_null("diplomacy_scrollbar") as CanvasItem
	if overlay:
		overlay.visible = false
	var items := _list_items("diplomatic_listbox")
	if items == null:
		return
	_clear_list(items)


func _clear_list(items: VBoxContainer) -> void:
	for child in items.get_children():
		items.remove_child(child)
		child.free()


func _fill_country_stats(info: Dictionary) -> void:
	_set_box("provinces_value", str(int(info.province_count)))
	_set_box("economy_value", _number_field(info, "development"))
	_set_box("fort_value", _number_field(info, "fort_level"))


func _clear_country_stats() -> void:
	for eu4_name in ["provinces_value", "economy_value", "fort_value"]:
		_set_box(eu4_name, "")


func _fill_opening_figures(info: Dictionary) -> void:
	_set_box("admtech_value", _number_field(info, "adm_tech"))
	_set_box("diptech_value", _number_field(info, "dip_tech"))
	_set_box("miltech_value", _number_field(info, "mil_tech"))
	_set_box("selected_ruler", _text_field(info, "ruler_name"))
	_set_box("ruler_adm_value", _number_field(info, "ruler_adm"))
	_set_box("ruler_dip_value", _number_field(info, "ruler_dip"))
	_set_box("ruler_mil_value", _number_field(info, "ruler_mil"))


func _clear_opening_figures() -> void:
	for eu4_name in ["admtech_value", "diptech_value", "miltech_value", "selected_ruler", "ruler_adm_value", "ruler_dip_value", "ruler_mil_value"]:
		_set_box(eu4_name, "")


func _set_box(eu4_name: String, text: String) -> void:
	var node := _find_named(_chrome, eu4_name)
	if node is RichTextLabel:
		node.text = text


func _number_field(info: Dictionary, key: String) -> String:
	var value: Variant = info.get(key, null)
	if value == null:
		return ""
	return str(int(value))


func _text_field(info: Dictionary, key: String) -> String:
	var value: Variant = info.get(key, null)
	if value == null:
		return ""
	return str(value)


func _apply_religion_icons(info: Dictionary) -> void:
	_paint_religion_icon("religion_icon", int(info.get("religion_icon", 0)))
	var secondary_name := str(info.get("secondary_religion", ""))
	var secondary_frame := int(info.get("secondary_religion_icon", 0))
	if secondary_name.is_empty() or secondary_frame < 1:
		var secondary := _find_named(_chrome, "secondary_religion_icon") as TextureRect
		if secondary:
			secondary.visible = false
		return
	_paint_religion_icon("secondary_religion_icon", secondary_frame)


func _paint_religion_icon(eu4_name: String, frame: int) -> void:
	var icon := _find_named(_chrome, eu4_name) as TextureRect
	if icon == null:
		return
	if frame < 1:
		icon.visible = false
		return
	var texture := _sprite_frame("GFX_icon_religion", frame)
	if texture == null:
		icon.visible = false
		return
	icon.texture = texture
	icon.visible = true


func _hide_religion_icons() -> void:
	if _chrome == null:
		return
	for eu4_name in ["religion_icon", "secondary_religion_icon"]:
		var icon := _find_named(_chrome, eu4_name) as TextureRect
		if icon:
			icon.visible = false


func _show_country_panel(selected: bool) -> void:
	if _gui == null:
		return
	_gui.set_named_visible("no_player", not selected)
	_gui.set_named_visible("singleplayer", selected)


func _show_panel_flag(tag: String) -> void:
	if _flag == null:
		return
	# 右栏这面旗用 gfx/flags/<标签>.tga，再按 player_shield 的薄盾框裁。底部书签盾徽不走这里。
	var texture := _flag_texture(tag)
	_flag.texture = texture
	_flag.visible = texture != null
	var shield := _flag.get_parent() as Control
	if shield == null or texture == null:
		return
	if shield.size.x < texture.get_width() or shield.size.y < texture.get_height():
		shield.offset_right = shield.offset_left + texture.get_width()
		shield.offset_bottom = shield.offset_top + texture.get_height()


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
	_hide_religion_icons()
	_clear_tech_government()
	_clear_diplomacy()
	_clear_opening_figures()
	_clear_national_ideas()
	_clear_country_stats()
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

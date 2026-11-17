# SPDX-License-Identifier: GPL-3.0-only
extends Control

const BUILDER_SCRIPT := preload("res://ui/eu4/eu4_ui_builder.gd")
const ASSETS_SCRIPT := preload("res://ui/eu4/eu4_ui_assets.gd")

const UNIMPLEMENTED := {
	"text_ADM": "行政点尚未实现",
	"text_DIP": "外交点尚未实现",
	"text_MIL": "军事点尚未实现",
	"text_stability": "稳定度尚未实现",
	"text_prestige": "威望尚未实现",
	"text_corruption": "腐化尚未实现",
	"text_sailors": "水手尚未实现",
	"text_dev": "尚未实现",
	"text_meritocracy": "尚未实现",
	"text_leg": "尚未实现",
	"text_rep": "尚未实现",
	"text_unity": "尚未实现",
	"text_merchants": "尚未实现",
	"text_settlers": "尚未实现",
	"text_diplomats": "尚未实现",
	"text_missionaries": "尚未实现",
	"text_powerprojection": "尚未实现",
	"topbar_liberty_value": "尚未实现",
}

@onready var host: Control = %Host
@onready var map_view: ProvinceMapView = $MapView

var _ui: AugustusMap
var _builder
var _assets
var _status: Label
var _feedback: Label
var _province_spin: SpinBox
var _province_id := 1
var _shell_shifted := false
var _map_ready := false
var _syncing_province := false


func _ready() -> void:
	_build_chrome()
	_ui = AugustusMap.new()
	_assets = ASSETS_SCRIPT.new()
	_assets.backend = _ui
	_builder = BUILDER_SCRIPT.new()
	_builder.assets = _assets
	_builder.action_requested.connect(_on_action)
	_load_shell()
	_load_real_window()
	_place_map()
	resized.connect(_place_map)
	_place_map.call_deferred()
	map_view.selection_changed.connect(_on_map_selection)
	_maybe_start_session()
	if GameSession.is_started() and GameSession.backend != null:
		map_view.show_map(GameSession.backend)
		map_view.set_projection(GameSession.map_projection)
		_map_ready = true
		if _province_id > 0:
			map_view.focus_province(_province_id)
	GameSession.clock_changed.connect(_refresh)
	GameSession.world_changed.connect(_refresh)
	_refresh()


func _build_chrome() -> void:
	var background := ColorRect.new()
	background.color = Color(0.035, 0.047, 0.063)
	background.set_anchors_preset(Control.PRESET_FULL_RECT)
	background.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(background)
	move_child(background, 0)
	var chrome := PanelContainer.new()
	chrome.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	chrome.offset_top = -112.0
	add_child(chrome)
	var box := VBoxContainer.new()
	chrome.add_child(box)
	var row := HBoxContainer.new()
	box.add_child(row)
	var province_label := Label.new()
	province_label.text = "省份编号"
	row.add_child(province_label)
	_province_spin = SpinBox.new()
	_province_spin.min_value = 1
	_province_spin.max_value = 20000
	_province_spin.value = 1
	_province_spin.rounded = true
	var requested := _argument("--province")
	if requested.is_valid_int():
		_province_spin.value = int(requested)
		_province_id = int(requested)
	_province_spin.value_changed.connect(func(value: float) -> void:
		if _syncing_province:
			return
		_province_id = int(value)
		_refresh_province(GameSession.is_started())
		if _map_ready:
			map_view.focus_province(_province_id)
	)
	row.add_child(_province_spin)
	_feedback = Label.new()
	_feedback.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(_feedback)
	_status = Label.new()
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	box.add_child(_status)
	var note := Label.new()
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.text = "行政点、外交点、军事点尚未实现，不显示占位数值。点选地图上的省份会更新右侧面板。当前游戏场景仍使用原有界面。"
	box.add_child(note)


func _maybe_start_session() -> void:
	var directory := GameSession.game_directory
	var country := GameSession.startup_country
	if directory.is_empty() or country.is_empty():
		return
	var error := GameSession.load_game(directory)
	if not error.is_empty():
		_feedback.text = error
		return
	error = GameSession.start_game(country)
	if not error.is_empty():
		_feedback.text = error


func _load_shell() -> void:
	var root := ProjectSettings.globalize_path("res://ui/eu4/shell")
	for file: String in ["interface/topbar.gui", "interface/province.gui"]:
		var loaded: Dictionary = _ui.load_eu4_ui_document(root, file)
		if loaded.get("ok", false) != true:
			_set_status(str(loaded.get("error", "无法读取合成界面")))
			continue
		var document: Variant = JSON.parse_string(str(loaded.get("json", "")))
		if typeof(document) != TYPE_DICTIONARY:
			_set_status("合成界面 JSON 无法解析")
			continue
		_builder.build_document(document, host)


func _load_real_window() -> void:
	var directory := GameSession.game_directory
	if directory.is_empty():
		_set_status("未提供游戏目录，只显示合成界面")
		return
	var file := _argument("--eu4-ui-file")
	var window_name := _argument("--eu4-ui-window")
	if file.is_empty():
		file = "interface/topbar.gui"
	if window_name.is_empty():
		window_name = "topbar"
	var loaded: Dictionary = _ui.load_eu4_ui_document(directory, file)
	if loaded.get("ok", false) != true:
		_set_status(str(loaded.get("error", "无法读取 EU4 窗口")))
		return
	var document: Variant = JSON.parse_string(str(loaded.get("json", "")))
	if typeof(document) != TYPE_DICTIONARY:
		_set_status("EU4 窗口 JSON 无法解析")
		return
	var clip := Control.new()
	clip.name = "RealWindowClip"
	clip.clip_contents = true
	clip.set_anchors_preset(Control.PRESET_TOP_WIDE)
	clip.offset_bottom = 126.0
	host.add_child(clip)
	var created: Array = _builder.build_named(document, window_name, clip)
	if created.is_empty():
		_set_status("窗口 %s 不在 %s 中" % [window_name, file])
		return
	_shift_shell()
	var gfx_errors := int(loaded.get("gfx_errors", 0))
	var suffix := "" if gfx_errors == 0 else "，%d 个 gfx 未解析" % gfx_errors
	_set_status("已载入 %s / %s%s" % [file, window_name, suffix])


func _shift_shell() -> void:
	if _shell_shifted:
		return
	_shell_shifted = true
	# 真实顶栏占上面 126 像素。顶栏整体下移；省份面板只让出重叠部分，避免压到底部说明。
	_offset_window("augustus_topbar", 136.0)
	_offset_window("augustus_province", 40.0)


func _place_map() -> void:
	# 地图只放在顶栏、省份面板和底部说明之间，避免挡住 EU4 控件。
	var top := 8.0
	var bar := _find("augustus_topbar")
	if bar:
		top = maxf(top, bar.offset_bottom + 8.0)
	var right := size.x - 8.0
	var panel := _find("augustus_province")
	if panel:
		right = minf(right, panel.offset_left - 8.0)
	var bottom := size.y - 120.0
	map_view.set_anchors_preset(Control.PRESET_TOP_LEFT)
	map_view.offset_left = 8.0
	map_view.offset_top = top
	map_view.offset_right = maxf(right, 208.0)
	map_view.offset_bottom = maxf(bottom, top + 160.0)


func _on_map_selection() -> void:
	var province_id := map_view.selected_province
	if province_id <= 0 and map_view.selected_army > 0 and GameSession.backend != null:
		var army: Dictionary = GameSession.backend.get_army_info(map_view.selected_army)
		if not army.is_empty():
			province_id = int(army.province)
	if province_id <= 0 or province_id == _province_id:
		return
	_province_id = province_id
	_syncing_province = true
	_province_spin.value = province_id
	_syncing_province = false
	_refresh_province(GameSession.is_started())


func _offset_window(node_name: String, amount: float) -> void:
	var node := _find(node_name)
	if node:
		node.offset_top += amount
		node.offset_bottom += amount


func _refresh() -> void:
	var started := GameSession.is_started()
	var country := GameSession.get_player_country()
	var finances: Dictionary = {}
	var manpower: Dictionary = {}
	var clock: Dictionary = GameSession.get_clock_state()
	if started:
		finances = GameSession.backend.get_country_finances(country)
		manpower = GameSession.backend.get_country_manpower(country)
	_set_named("topbar_title", _keep_localised("topbar_title"))
	_set_named("province_heading", _keep_localised("province_heading"))
	if finances.is_empty():
		_set_named("treasury", "国库：—")
		_set_named("monthly_tax", "月度税收：—")
		_set_named("army_maintenance", "陆军维护：—")
		_set_named("monthly_balance", "月度净收支：—")
		_set_named("text_gold", "—")
	else:
		_set_named("treasury", "国库：%s" % finances.treasury)
		_set_named("monthly_tax", "月度税收：+%s" % finances.monthly_tax)
		_set_named("army_maintenance", "陆军维护：-%s（基础 %s，超额 %s）" % [finances.army_maintenance, finances.base_army_maintenance, finances.excess_army_maintenance])
		_set_named("monthly_balance", "月度净收支：%s" % finances.monthly_balance)
		_set_named("text_gold", str(finances.treasury))
		_tint("monthly_balance", finances.in_deficit == true)
		_tint("text_gold", finances.treasury_negative == true)
	if manpower.is_empty():
		_set_named("manpower", "人力：—")
		_set_named("text_manpower", "—")
	else:
		_set_named("manpower", "人力：%d / %d，每月 +%d" % [manpower.available, manpower.capacity, manpower.monthly_recovery])
		_set_named("text_manpower", "%d/%d" % [manpower.available, manpower.capacity])
	if clock.is_empty():
		_set_named("date", "日期：—")
		_set_named("speed_state", "尚未开局")
		_set_button("pause", "暂停")
	else:
		_set_named("date", "日期：" + str(clock.date))
		_set_named("speed_state", "已暂停" if clock.paused else "%d×" % int(clock.speed))
		_set_button("pause", "继续" if clock.paused else "暂停")
		_set_named("pause_banner_text", "已暂停" if clock.paused else "")
	if started:
		var info: Dictionary = GameSession.backend.get_country_info(country)
		_set_named("PlayerNationText", "" if info.is_empty() else str(info.name))
	for node_name: String in UNIMPLEMENTED:
		_set_named(node_name, "未实现")
		_set_tooltip(node_name, str(UNIMPLEMENTED[node_name]))
	_refresh_province(started)


func _refresh_province(started: bool) -> void:
	if not started or GameSession.backend == null:
		_set_named("province_title", "未选择省份")
		for node_name: String in ["province_id", "owner", "controller", "culture", "religion", "trade_good", "development", "base_tax", "base_production", "base_manpower", "province_monthly_tax", "construction"]:
			_set_named(node_name, "—")
		_fill_list("buildings", PackedStringArray())
		_fill_list("recruitment", PackedStringArray())
		_fill_list("armies", PackedStringArray())
		return
	var province: Dictionary = GameSession.backend.get_province_info(_province_id)
	var construction: Dictionary = GameSession.backend.get_construction_info(_province_id)
	var recruitment: Dictionary = GameSession.backend.get_recruitment_info(_province_id)
	if province.is_empty():
		_set_named("province_title", "没有省份 #%d" % _province_id)
		return
	_set_named("province_title", str(province.name))
	_set_named("province_id", "编号：#%d" % int(province.id))
	_set_named("owner", "所属国家：%s" % _country_name(province, "owner"))
	_set_named("controller", "控制者：%s" % _country_name(province, "controller"))
	_set_named("culture", "文化：%s" % _value(province.get("culture")))
	_set_named("religion", "宗教：%s" % _value(province.get("religion")))
	_set_named("trade_good", "商品：%s" % _value(province.get("trade_good")))
	_set_named("development", "总发展度：%s" % _value(province.get("development")))
	_set_named("base_tax", "税收 %s" % _value(province.get("base_tax")))
	_set_named("base_production", "生产 %s" % _value(province.get("base_production")))
	_set_named("base_manpower", "人力 %s" % _value(province.get("base_manpower")))
	_set_named("province_monthly_tax", "当前月度税收：%s" % _value(province.get("monthly_tax")))
	_set_named("construction", _construction_text(construction))
	_fill_list("buildings", _building_lines(construction))
	_fill_list("recruitment", _recruitment_lines(recruitment))
	_fill_list("armies", _army_lines(recruitment))


func _construction_text(info: Dictionary) -> String:
	if info.is_empty():
		return "建造：—"
	if info.project == null:
		return "建造：当前没有工程"
	var project: Dictionary = info.project
	return "正在建设：%s，剩余 %d 天" % [project.name, project.days_remaining]


func _building_lines(info: Dictionary) -> PackedStringArray:
	var lines := PackedStringArray()
	if info.is_empty():
		return lines
	if info.buildings.is_empty():
		lines.append("已建建筑：无")
	else:
		lines.append("已建建筑：" + "、".join(info.buildings))
	return lines


func _recruitment_lines(info: Dictionary) -> PackedStringArray:
	var lines := PackedStringArray()
	if info.is_empty():
		return lines
	for offer: Dictionary in info.offers:
		var state := "可招募" if offer.can_recruit else str(offer.reason)
		lines.append("%s · %s 金币 · %d 人力 · %s" % [offer.kind, offer.cost, offer.manpower_cost, state])
	for order: Dictionary in info.orders:
		var phase := "排队" if order.queued else "正在招募"
		lines.append("%s：%s，剩余 %d 天" % [phase, order.kind, order.days_remaining])
	return lines


func _army_lines(info: Dictionary) -> PackedStringArray:
	var lines := PackedStringArray()
	if info.is_empty() or info.armies.is_empty():
		lines.append("军队：无")
		return lines
	for army: Dictionary in info.armies:
		lines.append("%s · %d 人 · %d 团" % [army.name, army.strength, army.regiment_count])
	return lines


func _on_action(eu4_name: String, _properties: Dictionary) -> void:
	match eu4_name:
		"pause":
			GameSession.toggle_pause()
		"speed_1", "speed_2", "speed_4", "speed_8":
			GameSession.set_game_speed(int(eu4_name.trim_prefix("speed_")))
		"build_tax", "open_build_interface_button":
			_build_tax()
		_:
			_feedback.text = "未映射的按钮：" + eu4_name


func _build_tax() -> void:
	if not GameSession.is_started():
		_feedback.text = "请先开始游戏"
		return
	var info: Dictionary = GameSession.backend.get_construction_info(_province_id)
	if info.is_empty() or info.options.is_empty():
		_feedback.text = "当前省份没有可建造项目"
		return
	var option: Dictionary = info.options[0]
	if not option.can_build:
		_feedback.text = str(option.reason)
		return
	var error := GameSession.start_construction(_province_id, str(option.id))
	_feedback.text = "已开始建造 %s" % option.name if error.is_empty() else error
	_refresh()


func _keep_localised(node_name: String) -> String:
	var node := _find(node_name)
	if node is RichTextLabel and not node.text.is_empty():
		return node.text
	return ""


func _set_named(node_name: String, text: String) -> void:
	_set_in(host, node_name, text)


func _set_in(node: Node, node_name: String, text: String) -> void:
	if str(node.get_meta("eu4_name", "")) == node_name:
		if node is RichTextLabel or node is Label:
			node.text = text
	for child: Node in node.get_children():
		_set_in(child, node_name, text)


func _set_button(node_name: String, text: String) -> void:
	var node := _find(node_name)
	if node is Button:
		node.text = text


func _set_tooltip(node_name: String, text: String) -> void:
	var node := _find(node_name)
	if node is Control:
		node.tooltip_text = text


func _tint(node_name: String, negative: bool) -> void:
	var node := _find(node_name)
	if node is CanvasItem:
		node.modulate = Color(1.0, 0.55, 0.48) if negative else Color(0.86, 0.93, 0.86)


func _fill_list(node_name: String, lines: PackedStringArray) -> void:
	var node := _find(node_name)
	if node == null:
		return
	var box := node.get_node_or_null("Items")
	if box == null:
		return
	for child: Node in box.get_children():
		box.remove_child(child)
		child.free()
	for line: String in lines:
		var label := Label.new()
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.text = line
		box.add_child(label)


func _find(node_name: String) -> Control:
	return _find_in(host, node_name)


func _find_in(node: Node, node_name: String) -> Control:
	if node is Control and str(node.get_meta("eu4_name", "")) == node_name:
		return node
	for child: Node in node.get_children():
		var found := _find_in(child, node_name)
		if found:
			return found
	return null


func _country_name(province: Dictionary, field: String) -> String:
	if province.get(field) == null:
		return "—"
	var display_name := str(province.get(field + "_name", ""))
	if not display_name.is_empty():
		return display_name
	return str(province[field])


func _value(value: Variant) -> String:
	return "—" if value == null else str(value)


func _argument(name: String) -> String:
	var arguments := OS.get_cmdline_user_args()
	var index := arguments.find(name)
	if index >= 0 and index + 1 < arguments.size():
		return str(arguments[index + 1])
	return ""


func _set_status(text: String) -> void:
	if _status:
		_status.text = text
		var diagnostics: PackedStringArray = _builder.diagnostics if _builder else PackedStringArray()
		var asset_failures: PackedStringArray = _assets.failures if _assets else PackedStringArray()
		if not diagnostics.is_empty():
			_status.text += "\n" + "\n".join(diagnostics)
		if not asset_failures.is_empty():
			var shown := asset_failures.slice(0, 4)
			_status.text += "\n" + "\n".join(shown)
			if asset_failures.size() > shown.size():
				_status.text += "\n另外 %d 个纹理没有读入" % (asset_failures.size() - shown.size())

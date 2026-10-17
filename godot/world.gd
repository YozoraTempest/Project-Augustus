# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends Control

var session = AugustusWorldSession.new()
var overview: Dictionary = {}
var province_image: Image
var color_ids: Dictionary = {}
var game_path: LineEdit
var baseline_save: LineEdit
var date_input: LineEdit
var player_countries_input: LineEdit
var mod_directories: Array[String] = []
var user_directory := ""
var launcher_config := ""
var status: Label
var country_choice: OptionButton
var country_details: Label
var politics_details: Label
var diplomacy_details: Label
var province_details: Label
var map_view: TextureRect
var directory_picker: FileDialog
var save_picker: FileDialog
var details_scroll: ScrollContainer
var selection_revision := 0
var unit_country_id := ""
var unit_option_cache: Dictionary = {}
var unit_option_controls: Dictionary = {}
var unit_selection_status: Label
var calendar_label: Label
var run_button: Button
var simulation_timer: Timer
var time_controls: Array[Button] = []
var running := false

func _ready() -> void:
	var font := SystemFont.new()
	font.font_names = PackedStringArray(["Microsoft YaHei", "Noto Sans CJK SC"])
	var ui_theme := Theme.new()
	ui_theme.default_font = font
	ui_theme.default_font_size = 16
	theme = ui_theme
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 24)
	add_child(margin)
	var page := VBoxContainer.new()
	page.add_theme_constant_override("separation", 12)
	margin.add_child(page)
	var header := HBoxContainer.new()
	page.add_child(header)
	var heading := label("PROJECT AUGUSTUS · 原版开局", 26)
	heading.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(heading)
	header.add_child(button("建造演示", func(): get_tree().change_scene_to_file("res://main.tscn")))
	page.add_child(label("EU4 终版 · 全部 DLC 机制基线 · 已接入日历与税收月结，其余机制尚待实现", 15))
	var form := HBoxContainer.new()
	page.add_child(form)
	game_path = LineEdit.new()
	game_path.placeholder_text = "选择本地 Europa Universalis IV 安装目录"
	game_path.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	form.add_child(game_path)
	form.add_child(button("选择目录", func(): directory_picker.popup_centered_ratio(0.7)))
	date_input = LineEdit.new()
	date_input.text = "1444.11.11"
	date_input.custom_minimum_size.x = 130
	form.add_child(date_input)
	player_countries_input = LineEdit.new()
	player_countries_input.placeholder_text = "玩家国家标签，可用逗号分隔"
	player_countries_input.custom_minimum_size.x = 220
	form.add_child(player_countries_input)
	form.add_child(button("载入开局", load_world))
	var save_form := HBoxContainer.new()
	page.add_child(save_form)
	baseline_save = LineEdit.new()
	baseline_save.placeholder_text = "可选：1444.11.11 全 DLC、无 Mod 的明文存档，仅导入 HRE 归属与帝位偏好"
	baseline_save.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	save_form.add_child(baseline_save)
	save_form.add_child(button("选择存档", func(): save_picker.popup_centered_ratio(0.7)))
	directory_picker = FileDialog.new()
	directory_picker.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	directory_picker.access = FileDialog.ACCESS_FILESYSTEM
	directory_picker.dir_selected.connect(func(path): game_path.text = path)
	add_child(directory_picker)
	save_picker = FileDialog.new()
	save_picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	save_picker.access = FileDialog.ACCESS_FILESYSTEM
	save_picker.file_selected.connect(func(path): baseline_save.text = path)
	add_child(save_picker)
	status = label("选择安装目录和玩家国家后载入；标签留空时控制身份保持未知。", 15)
	status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	status.max_lines_visible = 3
	page.add_child(status)
	var clock_bar := HBoxContainer.new()
	page.add_child(clock_bar)
	calendar_label = label("尚未载入", 16)
	calendar_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	clock_bar.add_child(calendar_label)
	run_button = button("运行", toggle_running)
	time_controls = [run_button, button("推进一天", func(): advance_world(1)), button("至下月", advance_month), button("保存", save_world), button("读取", restore_world)]
	for control in time_controls:
		control.disabled = true
		clock_bar.add_child(control)
	simulation_timer = Timer.new()
	simulation_timer.wait_time = 0.25
	simulation_timer.timeout.connect(func(): advance_world(1))
	add_child(simulation_timer)
	var middle := HBoxContainer.new()
	middle.size_flags_vertical = Control.SIZE_EXPAND_FILL
	middle.add_theme_constant_override("separation", 20)
	page.add_child(middle)
	map_view = TextureRect.new()
	map_view.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	map_view.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	map_view.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	map_view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	map_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	map_view.custom_minimum_size = Vector2(400, 250)
	map_view.gui_input.connect(map_input)
	middle.add_child(map_view)
	var sidebar := ScrollContainer.new()
	details_scroll = sidebar
	sidebar.custom_minimum_size.x = 320
	sidebar.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	middle.add_child(sidebar)
	var panel := VBoxContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel.add_theme_constant_override("separation", 12)
	sidebar.add_child(panel)
	panel.add_child(label("国家", 20))
	country_choice = OptionButton.new()
	country_choice.custom_minimum_size.x = 300
	country_choice.item_selected.connect(select_country)
	panel.add_child(country_choice)
	country_details = label("尚未载入", 16)
	country_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(country_details)
	panel.add_child(label("兵种选择", 20))
	unit_selection_status = label("尚未载入", 15)
	unit_selection_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(unit_selection_status)
	for category in ["infantry", "cavalry", "artillery"]:
		var row := HBoxContainer.new()
		row.add_child(label({"infantry": "步兵", "cavalry": "骑兵", "artillery": "炮兵"}[category], 15))
		var option := OptionButton.new()
		option.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		option.item_selected.connect(choose_unit_template.bind(category))
		unit_option_controls[category] = option
		row.add_child(option)
		panel.add_child(row)
	panel.add_child(label("统治者", 20))
	politics_details = label("尚未载入", 16)
	politics_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(politics_details)
	panel.add_child(label("外交", 20))
	diplomacy_details = label("尚未载入", 16)
	diplomacy_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(diplomacy_details)
	panel.add_child(label("省份 · 点击地图查看", 20))
	province_details = label("尚未选择", 16)
	province_details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	panel.add_child(province_details)
	if not parse_launch_args(OS.get_cmdline_user_args()):
		return
	if not game_path.text.is_empty():
		load_world()

func parse_launch_args(args: PackedStringArray) -> bool:
	var index := 0
	while index < args.size():
		var option := args[index]
		if option not in ["--game-root", "--date", "--player", "--mod", "--user-directory", "--launcher-config", "--baseline-save"]:
			index += 1
			continue
		if index + 1 >= args.size():
			status.text = "启动参数缺少对应的值。"
			return false
		var value := args[index + 1]
		match option:
			"--game-root":
				game_path.text = value
			"--date":
				date_input.text = value
			"--player":
				if player_countries_input.text.is_empty():
					player_countries_input.text = value
				else:
					player_countries_input.text += "," + value
			"--mod":
				mod_directories.append(value)
			"--user-directory":
				user_directory = value
			"--launcher-config":
				launcher_config = value
			"--baseline-save":
				baseline_save.text = value
		index += 2
	return true

func label(text: String, size: int) -> Label:
	var node := Label.new()
	node.text = text
	node.add_theme_font_size_override("font_size", size)
	return node

func button(text: String, action: Callable) -> Button:
	var node := Button.new()
	node.text = text
	node.custom_minimum_size.y = 40
	node.pressed.connect(action)
	return node

func load_world() -> bool:
	set_running(false)
	var candidate = AugustusWorldSession.new()
	var players: Array[String] = []
	for raw_tag in player_countries_input.text.split(","):
		var tag := raw_tag.strip_edges()
		if not tag.is_empty():
			players.append(tag)
	var packet: Dictionary
	if players.is_empty():
		packet = JSON.parse_string(candidate.load_world_with_baseline_save(
			game_path.text,
			date_input.text,
			mod_directories,
			user_directory,
			launcher_config,
			baseline_save.text
		))
	else:
		packet = JSON.parse_string(candidate.load_world_with_players_and_baseline_save(
			game_path.text,
			date_input.text,
			mod_directories,
			user_directory,
			launcher_config,
			players,
			baseline_save.text
		))
	if not packet.ok:
		status.text = packet.error
		return false
	var data: Dictionary = packet.value
	var image := Image.load_from_file(data.province_bitmap)
	if image == null or image.is_empty():
		status.text = "无法读取省份地图。"
		return false
	if image.get_width() != data.report.map_width or image.get_height() != data.report.map_height:
		status.text = "省份地图尺寸与开局数据不一致。"
		return false
	session = candidate
	overview = data
	province_image = image
	map_view.texture = ImageTexture.create_from_image(image)
	color_ids.clear()
	var width := 1
	while width < data.palette.size():
		width *= 2
	var palette := Image.create(width, 2, false, Image.FORMAT_RGBA8)
	palette.fill(Color(0.12, 0.12, 0.12))
	for i in data.palette.size():
		var entry: Dictionary = data.palette[i]
		palette.set_pixel(i, 0, rgb(entry.color))
		palette.set_pixel(i, 1, rgb(entry.display_color))
		color_ids[color_key(entry.color)] = int(entry.province)
	var material := ShaderMaterial.new()
	material.shader = load("res://political_map.gdshader")
	material.set_shader_parameter("province_palette", ImageTexture.create_from_image(palette))
	material.set_shader_parameter("palette_size", data.palette.size())
	material.set_shader_parameter("palette_width", float(width))
	map_view.material = material
	country_choice.clear()
	unit_country_id = ""
	unit_option_cache.clear()
	for country in data.countries:
		country_choice.add_item("%s · %s (%d)" % [country.id, country.name, country.province_count])
		country_choice.set_item_metadata(country_choice.item_count - 1, country.id)
	var unapplied := 0
	for count in data.report.unapplied_country_fields.values():
		unapplied += int(count)
	for count in data.report.unapplied_diplomacy_fields.values():
		unapplied += int(count)
	for count in data.report.unapplied_province_fields.values():
		unapplied += int(count)
	status.text = "%d 个国家定义 · %d 个有领土国家 · %d 个省份定义\n尚未应用 %d 项历史字段；有 %d 个国家的历史文件存在歧义。" % [data.report.countries, data.report.active_countries, data.report.provinces, unapplied, data.report.ambiguous_country_histories.size()]
	if data.saved_imperial_baseline != null:
		status.text += "\n从所选存档导入 %d 个 HRE 省份归属和 %d 条帝位偏好。" % [int(data.saved_imperial_baseline.provinces_checked), int(data.saved_imperial_baseline.preferences_imported)]
	province_details.text = "点击地图查看省份。"
	select_country(0)
	for control in time_controls:
		control.disabled = false
	refresh_calendar()
	return true

func set_running(value: bool) -> void:
	running = value
	run_button.text = "暂停" if running else "运行"
	if running:
		simulation_timer.start()
	else:
		simulation_timer.stop()

func toggle_running() -> void:
	set_running(not running)

func refresh_calendar() -> void:
	var packet: Dictionary = JSON.parse_string(session.simulation_view())
	if packet.ok:
		var date: Dictionary = packet.value.date
		calendar_label.text = "%d.%02d.%02d · 月结 %d 次" % [date.year, date.month, date.day, packet.value.tax_settlements]

func advance_world(days: int) -> bool:
	var packet: Dictionary = JSON.parse_string(session.advance(days))
	if not packet.ok:
		set_running(false)
		status.text = packet.error
		return false
	refresh_calendar()
	select_country(country_choice.selected, false)
	return true

func advance_month() -> void:
	set_running(false)
	var packet: Dictionary = JSON.parse_string(session.simulation_view())
	if packet.ok:
		advance_world(int(packet.value.days_until_next_month))

func save_world(path: String = "user://augustus-world.json") -> bool:
	set_running(false)
	var packet: Dictionary = JSON.parse_string(session.save_snapshot())
	if not packet.ok:
		status.text = packet.error
		return false
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		status.text = "无法保存战役。"
		return false
	file.store_string(packet.value)
	var error := file.get_error()
	file.close()
	status.text = "已保存到用户数据目录。" if error == OK else "战役保存失败。"
	return error == OK

func restore_world(path: String = "user://augustus-world.json") -> bool:
	set_running(false)
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		status.text = "没有可读取的战役存档。"
		return false
	var snapshot := file.get_as_text()
	file.close()
	var packet: Dictionary = JSON.parse_string(session.restore_snapshot(snapshot))
	if not packet.ok:
		status.text = packet.error
		return false
	refresh_calendar()
	select_country(country_choice.selected, false)
	status.text = "已恢复战役，当前暂停。"
	return true

func rgb(values: Array) -> Color:
	return Color(float(values[0]) / 255.0, float(values[1]) / 255.0, float(values[2]) / 255.0)

func color_key(values: Array) -> int:
	return int(values[0]) * 65536 + int(values[1]) * 256 + int(values[2])

func shown(value: Variant) -> String:
	if value == null:
		return "未载入"
	if value is float or value is int:
		return str(int(value))
	return str(value)

func select_country(index: int, reset_scroll: bool = true) -> void:
	if country_choice.item_count == 0:
		return
	var id: String = country_choice.get_item_metadata(index)
	selection_revision += 1
	if reset_scroll:
		details_scroll.scroll_vertical = 0
	var packet: Dictionary = JSON.parse_string(session.query_country(id))
	if not packet.ok:
		status.text = packet.error
		return
	var country: Dictionary = packet.value.state
	refresh_unit_templates(id)
	var government: Variant = packet.value.government
	country_details.text = "%s · %s\n领土 %d 个省份 · 首都 %s\n文化 %s · 宗教 %s\n政体 %s" % [country.id, country.name, packet.value.owned_provinces.size(), shown(packet.value.bound_capital), shown(country.primary_culture), shown(country.religion), shown(null if government == null else government.kind)]
	if government != null:
		country_details.text += "\n已选改革：" + ("未载入" if government.reforms == null else ", ".join(government.reforms))
		if government.metrics == null:
			country_details.text += "\n合法性：适用状态未确定"
		elif government.metrics.has("legitimacy"):
			country_details.text += "\n合法性（历史初值）：" + ("未载入" if government.metrics.legitimacy == null else "%.3f" % (float(government.metrics.legitimacy) / 1000.0))
		else:
			country_details.text += "\n合法性：此政体不使用"
	var ideas: Variant = packet.value.ideas
	if ideas == null or ideas.groups == null:
		country_details.text += "\n国家理念：选择未确定"
	else:
		for group in ideas.groups:
			var progress: Variant = ideas.groups[group]
			country_details.text += "\n国家理念 %s · %s" % [group, "进度未确定" if progress == null else "已解锁 %d 项" % int(progress)]
	var tech: Variant = packet.value.technology
	country_details.text += "\n行政 / 外交 / 军事科技：\n" + ("未载入" if tech == null else "%s / %s / %s" % [shown(tech.get("administrative")), shown(tech.get("diplomatic")), shown(tech.get("military"))])
	var resources: Variant = packet.value.resources
	if resources != null:
		country_details.text += "\n国库 %s · 人力 %s\n重商主义 %s" % [scaled(resources.treasury_milli, 1000), scaled(resources.manpower_milli, 1000), scaled(resources.mercantilism_milli, 1000)]
	if not packet.value.modifiers.is_empty():
		country_details.text += "\n历史修正：" + ", ".join(packet.value.modifiers)
	var estates: Variant = packet.value.estates
	if estates != null:
		country_details.text += "\n阶层"
		var unknown_estates := 0
		var active_estates := 0
		for name in estates:
			var estate: Dictionary = estates[name]
			if estate.active == null:
				unknown_estates += 1
				continue
			if not estate.active:
				continue
			active_estates += 1
			country_details.text += "\n%s\n忠诚度 %s · 影响力 %s\n土地份额 %s" % [name, scaled(estate.loyalty_milli, 1000), scaled(estate.influence_milli, 1000), autonomy_percent(estate.land_share_micros)]
			if estate.privileges == null:
				country_details.text += "\n历史特权尚未确定"
			elif not estate.privileges.is_empty():
				country_details.text += "\n历史特权：" + ", ".join(estate.privileges)
		if active_estates == 0:
			country_details.text += "\n暂无已确定可用的阶层"
		if unknown_estates > 0:
			country_details.text += "\n另有 %d 个阶层的可用条件尚未确定" % unknown_estates
	var totals: Dictionary = packet.value.province_attribute_totals
	country_details.text += "\n省份贡献合计（已接入部分，自治前）\n税收基数 %s · 人力 %s\n完整属性与其他结算尚未接入" % [scaled(totals.tax_basis.known_subtotal_micros, 1000000), scaled(totals.manpower_contribution.known_subtotal_micros, 1000000)]
	var tax: Variant = packet.value.last_monthly_tax
	if tax != null:
		country_details.text += "\n最近税收月结\n已知部分 %s · 完整收入 %s\n入账 %s" % [scaled(tax.known_subtotal_milli, 1000), scaled(tax.income_milli, 1000), scaled(tax.credited_milli, 1000)]
		if not tax.unresolved.is_empty():
			country_details.text += "\n有 %d 项缺项，收入或余额尚未确定。" % tax.unresolved.size()
	var areas = packet.value.administrative_areas
	if areas != null:
		var known: Array[String] = []
		for area in areas:
			known.append("%s（%s）" % [area, "州" if areas[area] == "state" else "领地"])
		country_details.text += "\n已确定的行政地区：" + "、".join(known)
	var people: Array[String] = []
	for role in ["ruler", "heir", "consort"]:
		var title: String = {"ruler": "君主", "heir": "继承人", "consort": "配偶"}[role]
		var person: Variant = null if packet.value.politics == null else packet.value.politics[role]
		if person == null:
			people.append(title + "：未载入")
		else:
			people.append("%s：%s%s\n行政 %s / 外交 %s / 军事 %s" % [title, shown(person.name), "（摄政）" if person.regent == true else "", shown(person.administration), shown(person.diplomacy), shown(person.military)])
	politics_details.text = "\n".join(people)
	var relations: Array[String] = []
	for office in packet.value.political_offices:
		relations.append({"imperial_emperor": "神圣罗马帝国皇帝", "celestial_emperor": "天朝皇帝"}.get(office, office))
	for relation in packet.value.diplomacy:
		relations.append(relation_text(relation, id))
	diplomacy_details.text = "没有已载入的外交关系" if relations.is_empty() else "\n".join(relations)

func refresh_unit_templates(id: String) -> void:
	var packet: Dictionary = JSON.parse_string(session.query_unit_templates(id))
	if not packet.ok:
		unit_selection_status.text = packet.error
		return
	var data: Dictionary = packet.value
	var available: Dictionary = data.available
	var selected: Dictionary = data.selected
	var unselected := 0
	var unavailable := 0
	for category in ["infantry", "cavalry", "artillery"]:
		var option: OptionButton = unit_option_controls[category]
		var choices: Array = available.get(category, [])
		var chosen: Variant = selected.get(category)
		if choices.is_empty():
			unavailable += 1
		elif chosen == null:
			unselected += 1
		var signature := JSON.stringify([choices, chosen])
		if unit_country_id == id and unit_option_cache.get(category) == signature:
			continue
		option.clear()
		option.add_item("未选择")
		option.set_item_metadata(0, "")
		var selected_index := 0
		for unit_type in choices:
			option.add_item(unit_type)
			option.set_item_metadata(option.item_count - 1, unit_type)
			if unit_type == chosen:
				selected_index = option.item_count - 1
		option.select(selected_index)
		option.disabled = choices.is_empty()
		unit_option_cache[category] = signature
	unit_country_id = id
	if unselected == 0:
		unit_selection_status.text = "已选择所有可用兵种。%s" % ("部分类别尚未解锁。" if unavailable > 0 else "")
	else:
		unit_selection_status.text = "%d 个类别尚未选择；%d 个类别当前没有解锁模板。" % [unselected, unavailable]

func choose_unit_template(index: int, category: String) -> void:
	if unit_country_id.is_empty():
		return
	var option: OptionButton = unit_option_controls[category]
	var unit_type := str(option.get_item_metadata(index))
	if unit_type.is_empty():
		var clear_packet: Dictionary = JSON.parse_string(session.clear_unit_template(unit_country_id, category))
		if not clear_packet.ok:
			unit_selection_status.text = clear_packet.error
			return
		unit_option_cache.clear()
		refresh_unit_templates(unit_country_id)
		return
	var packet: Dictionary = JSON.parse_string(session.select_unit_template(unit_country_id, category, unit_type))
	if not packet.ok:
		unit_selection_status.text = packet.error
		return
	unit_option_cache.clear()
	refresh_unit_templates(unit_country_id)

func relation_text(relation: Dictionary, id: String) -> String:
	var first: bool = relation.first == id
	var other: String = relation.second if first else relation.first
	match relation.kind:
		"alliance": return "盟友 " + other
		"royal_marriage": return "王室联姻 " + other
		"guarantee": return ("保障 " + other) if first else ("受 " + other + " 保障")
		"warning": return ("警告 " + other) if first else ("收到 " + other + " 的警告")
		"trade_league": return ("贸易联盟成员 " + other) if first else ("贸易联盟领袖 " + other)
		"subject":
			var type_name: String = {"vassal": "附庸", "march": "卫戍国", "personal_union": "共主邦联", "appanage": "分封领", "daimyo_vassal": "大名", "tributary_state": "朝贡国"}.get(relation.subject_type, relation.subject_type)
			return "%s %s（%s）" % ["属国" if first else "宗主国", other, type_name]
	return relation.kind + " " + other

func scaled(value: Variant, scale: int) -> String:
	return "未载入" if value == null else "%.6f" % (float(value) / scale)


func select_province(id: int) -> bool:
	var packet: Dictionary = JSON.parse_string(session.query_province(id))
	if not packet.ok:
		status.text = packet.error
		return false
	var province: Dictionary = packet.value
	var development := "未载入"
	if province.tax_milli != null and province.production_milli != null and province.manpower_milli != null:
		development = "%.3f / %.3f / %.3f" % [province.tax_milli / 1000.0, province.production_milli / 1000.0, province.manpower_milli / 1000.0]
	province_details.text = "%s · %d\n所有者 %s · 控制者 %s\n文化 %s · 宗教 %s\n商品 %s\n税收 / 生产 / 人力：\n%s" % [province.name, province.id, shown(province.owner), shown(province.controller), shown(province.culture), shown(province.religion), shown(province.trade_good), development]
	if province.trade_good_definition != null:
		var price: Variant = province.trade_good_definition.base_price_milli
		province_details.text += "\n商品基础价格：" + ("未声明" if price == null else "%.3f" % (price / 1000.0))
		if province.trade_good_definition.gold_type == true:
			province_details.text += "（金矿类）"
	var buildings := "未载入"
	if province.buildings != null:
		buildings = "无历史记载" if province.buildings.is_empty() else ", ".join(province.buildings)
	province_details.text += "\n历史记载建筑：" + buildings
	province_details.text += "\n贸易节点：" + shown(province.trade_node)
	if province.trade_node_outgoing != null:
		province_details.text += "\n流向：" + ("无下游" if province.trade_node_outgoing.is_empty() else ", ".join(province.trade_node_outgoing))
	if not province.modifiers.is_empty():
		province_details.text += "\n历史修正：" + ", ".join(province.modifiers)
		var labels := {"province_trade_power_value": "省份贸易力量", "local_development_cost": "本地发展成本修正", "local_tax_modifier": "本地税收修正"}
		for key in province.declared_modifier_totals_micros:
			province_details.text += "\n%s（声明值）：%s" % [labels.get(key, key), scaled(province.declared_modifier_totals_micros[key], 1000000)]
	province_details.text += attribute_text(province.attributes)
	province_details.text += administration_text(province.administration)
	selection_revision += 1
	reveal_province.call_deferred(selection_revision)
	return true

func attribute_text(attributes: Dictionary) -> String:
	if attributes.is_empty():
		return ""
	var text := "\n属性计算（已接入部分）"
	var labels := {"tax_basis": "税收基数（自治前）", "manpower_contribution": "人力贡献（自治前）", "tax_after_autonomy": "税收基数（自治折减后）", "manpower_after_autonomy": "人力贡献（自治折减后）"}
	var missing: Dictionary = {}
	for name in attributes:
		var value: Dictionary = attributes[name]
		text += "\n%s：%s" % [labels.get(name, name), scaled(value.known_subtotal_micros, 1000000)]
		for step in value.steps:
			for contribution in step.contributions:
				var source_name: String = contribution.source
				if source_name == "static:city":
					source_name = "城市"
				elif source_name == "static:occupied":
					source_name = "被占领"
				elif source_name == "static:core":
					source_name = "州内完整核心"
				elif source_name == "static:colonial_core":
					source_name = "州内领地核心"
				elif source_name == "static:non_core":
					source_name = "州内非核心"
				elif source_name == "static:territory_core":
					source_name = "领地内核心"
				elif source_name == "static:territory_non_core":
					source_name = "领地内非核心"
				elif source_name == "static:same_culture_group":
					source_name = "同组未接受文化"
				elif source_name == "static:non_accepted_culture":
					source_name = "异组未接受文化"
				elif source_name.begins_with("affiliation:religion:province:"):
					source_name = "省份宗教 " + source_name.trim_prefix("affiliation:religion:province:")
				elif source_name.begins_with("affiliation:religion:country:"):
					source_name = "国教 " + source_name.trim_prefix("affiliation:religion:country:")
				elif source_name.begins_with("building:"):
					source_name = "建筑 " + source_name.trim_prefix("building:")
				elif source_name.begins_with("ideas:traditions:"):
					source_name = "国家传统 " + source_name.trim_prefix("ideas:traditions:")
				elif source_name.begins_with("ideas:idea:"):
					source_name = "已解锁理念 " + source_name.trim_prefix("ideas:idea:")
				elif source_name.begins_with("ideas:completion:"):
					source_name = "理念完成奖励 " + source_name.trim_prefix("ideas:completion:")
				elif source_name.begins_with("government:reform:"):
					source_name = "政体改革 " + source_name.trim_prefix("government:reform:")
				elif source_name.begins_with("estates:"):
					source_name = "阶层 " + source_name.trim_prefix("estates:")
				elif source_name == "government:metric:legitimacy":
					source_name = "合法性"
				elif source_name.begins_with("trade_good:"):
					source_name = "商品 " + source_name.trim_prefix("trade_good:")
				elif source_name.begins_with("modifier:"):
					source_name = "历史修正 " + source_name.trim_prefix("modifier:")
				elif source_name == "administration:autonomy":
					source_name = "自治折减"
				var amount: String = "%.4f%%" % (float(contribution.micros) / 10000) if contribution.is_rate else scaled(contribution.micros, 1000000)
				text += "\n  %s：%s" % [source_name, amount]
		for reason in value.unresolved:
			missing[reason] = true
	var reasons := {"autonomy_and_state_rules": "完整自治与州规则", "state_and_core_effects": "州与核心的其他效果", "culture_and_religion": "文化与宗教", "culture_government_effects": "政体对文化待遇的影响", "tolerance_and_religious_mechanics": "宽容度与宗教机制", "affiliation:culture:unknown_relation": "文化待遇未知", "other_government_estate_and_country_effects": "其他政体、阶层及国家效果", "ideas:unknown_selection": "国家理念选择未知", "building_retention_conditions": "建筑保留条件", "original_runtime_comparison": "原版运行核对", "city_status": "城市状态", "administration:area_status": "开局州与领地划分", "administration:core_type": "核心类型", "administration:initial_autonomy": "初始自治度", "administration:minimum_autonomy": "最低自治", "administration:other_minimum_autonomy_effects": "其他最低自治修正", "administration:scoped_country_history": "国家历史中的自治操作"}
	var descriptions: Array[String] = []
	reasons["administration:unknown_category"] = "州、领地或所有者核心状态未知"
	reasons["administration:missing_province"] = "省份行政状态未知"
	for reason in missing:
		if reason.begins_with("estates:"):
			var estate_reason := "阶层或特权效果尚未确定"
			if reason.ends_with("unknown_presence"):
				estate_reason = "阶层可用条件尚未确定"
			elif reason.ends_with("unknown_loyalty_band"):
				estate_reason = "阶层忠诚度及适用效果尚未确定"
			elif reason.ends_with("unknown_scale"):
				estate_reason = "阶层影响力或土地份额尚未确定"
			elif reason.ends_with("unknown_privileges"):
				estate_reason = "阶层特权尚未确定"
			elif reason.ends_with("is_valid"):
				estate_reason = "特权适用条件尚未确定"
			elif ":unexecuted:" in reason:
				estate_reason = "特权授予效果尚未执行"
			if not descriptions.has(estate_reason):
				descriptions.append(estate_reason)
		else:
			descriptions.append(reasons.get(reason, reason))
	if not descriptions.is_empty():
		text += "\n缺项：" + "、".join(descriptions)
	return text

func administration_text(administration) -> String:
	if administration == null:
		return ""
	var statuses := {"state": "州", "territory": "领地"}
	var core_types := {"full": "完整核心", "territorial": "领地核心", "unspecified": "已声明核心，类型待确认"}
	var core_label: String = "无核心" if administration.cores_known else "未知"
	if administration.owner_core != null:
		core_label = core_types.get(administration.owner_core, "未知")
	var text := "\n行政状态\n地区 %s · %s\n所有者核心：%s" % [shown(administration.area), statuses.get(administration.area_status, "州与领地状态未知"), core_label]
	if administration.is_capital:
		text += "\n当前首都"
	elif administration.is_capital_area:
		text += "\n属于首都地区"
	text += "\n原始自治度：%s" % autonomy_percent(administration.raw_autonomy_micros)
	if administration.raw_autonomy_micros == null:
		text += "\n历史自治追加：%+.4f%%（尚未应用）" % (float(administration.history_autonomy_delta_micros) / 10000)
	text += "\n规则最低自治：%s\n有效自治（已接入规则）：%s" % [autonomy_percent(administration.minimum_autonomy_micros), autonomy_percent(administration.effective_autonomy_micros)]
	return text

func autonomy_percent(value) -> String:
	return "未知" if value == null else "%.4f%%" % (float(value) / 10000)

func reveal_province(revision: int) -> void:
	await get_tree().process_frame
	if revision == selection_revision:
		details_scroll.ensure_control_visible(province_details)

func map_input(event: InputEvent) -> void:
	if province_image == null or not event is InputEventMouseButton:
		return
	if not event.pressed or event.button_index != MOUSE_BUTTON_LEFT:
		return
	var image_size := Vector2(province_image.get_width(), province_image.get_height())
	var factor := minf(map_view.size.x / image_size.x, map_view.size.y / image_size.y)
	var origin := (map_view.size - image_size * factor) / 2.0
	var pixel: Vector2 = (event.position - origin) / factor
	if pixel.x < 0 or pixel.y < 0 or pixel.x >= image_size.x or pixel.y >= image_size.y:
		return
	var color := province_image.get_pixel(int(pixel.x), int(pixel.y))
	var key := color_key([roundi(color.r * 255), roundi(color.g * 255), roundi(color.b * 255)])
	if color_ids.has(key):
		select_province(color_ids[key])

# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends Control

var session = AugustusWorldSession.new()
var overview: Dictionary = {}
var province_image: Image
var color_ids: Dictionary = {}
var game_path: LineEdit
var date_input: LineEdit
var status: Label
var country_choice: OptionButton
var country_details: Label
var politics_details: Label
var diplomacy_details: Label
var province_details: Label
var map_view: TextureRect
var directory_picker: FileDialog
var details_scroll: ScrollContainer
var selection_revision := 0

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
	page.add_child(label("EU4 终版 · 全部 DLC 机制基线 · 当前可查看开局，尚未接入时间推进", 15))
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
	form.add_child(button("载入开局", load_world))
	directory_picker = FileDialog.new()
	directory_picker.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	directory_picker.access = FileDialog.ACCESS_FILESYSTEM
	directory_picker.dir_selected.connect(func(path): game_path.text = path)
	add_child(directory_picker)
	status = label("选择安装目录后载入。游戏资源从本地读取。", 15)
	status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	status.max_lines_visible = 3
	page.add_child(status)
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
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		if args[i] == "--game-root" and i + 1 < args.size():
			game_path.text = args[i + 1]
		if args[i] == "--date" and i + 1 < args.size():
			date_input.text = args[i + 1]
	if not game_path.text.is_empty():
		load_world()

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
	var candidate = AugustusWorldSession.new()
	var packet: Dictionary = JSON.parse_string(candidate.load_world(game_path.text, date_input.text))
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
	province_details.text = "点击地图查看省份。"
	select_country(0)
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

func select_country(index: int) -> void:
	if country_choice.item_count == 0:
		return
	var id: String = country_choice.get_item_metadata(index)
	selection_revision += 1
	details_scroll.scroll_vertical = 0
	var packet: Dictionary = JSON.parse_string(session.query_country(id))
	if not packet.ok:
		status.text = packet.error
		return
	var country: Dictionary = packet.value.state
	country_details.text = "%s · %s\n领土 %d 个省份 · 首都 %s\n文化 %s · 宗教 %s\n政体 %s" % [country.id, country.name, packet.value.owned_provinces.size(), shown(packet.value.bound_capital), shown(country.primary_culture), shown(country.religion), shown(country.government)]
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
	selection_revision += 1
	reveal_province.call_deferred(selection_revision)
	return true

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

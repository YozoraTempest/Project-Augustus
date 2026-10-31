# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends VBoxContainer

signal province_selected(province_id: int)

var section_button: Button
var report_status: Label
var tree: Tree
var detail_label: Label
var location_button: Button
var section_expanded := false
var selected_key := ""

var _content: VBoxContainer
var _report_data: Dictionary = {}
var _has_report := false
var _items_by_key: Dictionary = {}
var _selected_details: Dictionary = {}
var _target_province_id := 0

func _ready() -> void:
	add_theme_constant_override("separation", 6)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	section_button = Button.new()
	section_button.custom_minimum_size.y = 40
	section_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	section_button.pressed.connect(toggle_section)
	add_child(section_button)
	_content = VBoxContainer.new()
	_content.add_theme_constant_override("separation", 6)
	_content.visible = false
	add_child(_content)
	report_status = Label.new()
	_content.add_child(report_status)
	tree = Tree.new()
	tree.name = "MilitaryRoster"
	tree.custom_minimum_size.y = 190
	tree.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tree.columns = 1
	tree.hide_root = true
	tree.select_mode = Tree.SELECT_SINGLE
	tree.set_column_expand(0, true)
	tree.item_selected.connect(_on_item_selected)
	_content.add_child(tree)
	detail_label = Label.new()
	detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	detail_label.visible = false
	_content.add_child(detail_label)
	location_button = Button.new()
	location_button.text = "查看驻地"
	location_button.custom_minimum_size.y = 36
	location_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	location_button.disabled = true
	location_button.visible = false
	location_button.pressed.connect(_on_location_pressed)
	_content.add_child(location_button)
	_update_header()
	_show_empty_selection()

func toggle_section() -> void:
	section_expanded = not section_expanded
	_content.visible = section_expanded
	_update_header()

func render_report(data: Dictionary) -> void:
	if _has_report and data == _report_data:
		return
	var expanded_keys: Dictionary = {}
	if _has_report:
		_capture_expanded_keys(tree.get_root(), expanded_keys)
		var selected := tree.get_selected()
		if selected != null:
			var metadata: Variant = selected.get_metadata(0)
			if metadata is Dictionary:
				selected_key = str(metadata.get("key", ""))
	_report_data = data.duplicate(true)
	_has_report = true
	_rebuild_tree(expanded_keys)
	_update_header()
	_restore_selection()

func clear() -> void:
	_report_data.clear()
	_has_report = false
	selected_key = ""
	_items_by_key.clear()
	_selected_details.clear()
	_target_province_id = 0
	if tree != null:
		tree.clear()
	_update_header()
	_show_empty_selection()

func _update_header() -> void:
	if section_button == null:
		return
	var summary := "尚未载入"
	var formation_count := 0
	if _has_report:
		formation_count = _report_data.get("armies", []).size() + _report_data.get("fleets", []).size()
		summary = "暂无已载入编队" if formation_count == 0 else "已载入编队：%d" % formation_count
	section_button.text = "军力 · %s %s" % [summary, "▼" if section_expanded else "▶"]
	if report_status != null:
		report_status.text = summary
		report_status.visible = not _has_report or formation_count == 0
	if tree != null:
		tree.visible = _has_report and formation_count > 0

func _capture_expanded_keys(item: TreeItem, expanded_keys: Dictionary) -> void:
	if item == null:
		return
	var metadata: Variant = item.get_metadata(0)
	if metadata is Dictionary and item.get_child_count() > 0 and not item.is_collapsed():
		expanded_keys[str(metadata.get("key", ""))] = true
	for index in item.get_child_count():
		_capture_expanded_keys(item.get_child(index), expanded_keys)

func _rebuild_tree(expanded_keys: Dictionary) -> void:
	tree.clear()
	_items_by_key.clear()
	var root := tree.create_item()
	for army in _report_data.get("armies", []):
		_add_formation(root, army, "army", expanded_keys)
	for fleet in _report_data.get("fleets", []):
		_add_formation(root, fleet, "fleet", expanded_keys)

func _add_formation(root: TreeItem, formation: Dictionary, kind: String, expanded_keys: Dictionary) -> void:
	var id := str(formation.get("id", ""))
	var key := "%s:%s" % [kind, id]
	var province_id := int(formation.get("province_id", 0))
	var province_name := _shown(formation.get("province_name"))
	var raw_formation_name: Variant = formation.get("name")
	var formation_name := "" if raw_formation_name == null else str(raw_formation_name)
	if formation_name.is_empty():
		formation_name = ("陆军" if kind == "army" else "舰队") + " #%s" % id
	var title := "%s · %s · 驻地 %s (%d)" % ["陆军" if kind == "army" else "舰队", formation_name, province_name, province_id]
	var item := tree.create_item(root)
	item.set_text(0, title)
	item.set_tooltip_text(0, title)
	item.set_metadata(0, {
		"key": key,
		"kind": kind,
		"id": id,
		"name": formation_name,
		"province_id": province_id,
		"province_name": province_name,
		"details": _formation_details(kind, id, formation_name, province_name, province_id),
	})
	_items_by_key[key] = item
	if kind == "army":
		for regiment in formation.get("regiments", []):
			_add_member(item, regiment, "regiment", province_id, province_name)
	else:
		for ship in formation.get("ships", []):
			_add_member(item, ship, "ship", province_id, province_name)
	if item.get_child_count() > 0:
		item.set_collapsed(not expanded_keys.has(key))

func _add_member(parent: TreeItem, member: Dictionary, kind: String, province_id: int, province_name: String) -> void:
	var id := str(member.get("id", ""))
	var key := "%s:%s" % [kind, id]
	var unit_type := str(member.get("unit_type", ""))
	var raw_name: Variant = member.get("name")
	var display_name := unit_type if raw_name == null or str(raw_name).is_empty() else str(raw_name)
	if display_name.is_empty():
		display_name = ("团" if kind == "regiment" else "舰船") + " #%s" % id
	var title := "%s · #%s" % [display_name, id]
	if raw_name != null and not str(raw_name).is_empty():
		title = "%s · %s · #%s" % [display_name, unit_type if not unit_type.is_empty() else "类型未载入", id]
	var item := tree.create_item(parent)
	item.set_text(0, title)
	item.set_tooltip_text(0, title)
	var details := _member_details(kind, id, raw_name, unit_type, member, province_name, province_id)
	item.set_metadata(0, {
		"key": key,
		"kind": kind,
		"id": id,
		"details": details,
		"province_id": province_id,
		"province_name": province_name,
	})
	_items_by_key[key] = item

func _formation_details(kind: String, id: String, formation_name: String, province_name: String, province_id: int) -> Dictionary:
	return {
		"text": "%s · 编队 #%s\n名称：%s\n当前驻地：%s (%d)" % ["陆军" if kind == "army" else "舰队", id, formation_name, province_name, province_id],
	}

func _member_details(kind: String, id: String, raw_name: Variant, unit_type: String, member: Dictionary, province_name: String, province_id: int) -> Dictionary:
	var label := "团" if kind == "regiment" else "舰船"
	var lines: Array[String] = ["%s · %s · #%s" % [label, unit_type if not unit_type.is_empty() else "类型未载入", id]]
	lines.append("原始名称：%s" % _shown(raw_name))
	lines.append("当前驻地：%s (%d)" % [province_name, province_id])
	if kind == "regiment":
		lines.append("兵力：%s（比例） · 士气：%s" % [_scaled(member.get("strength_milli")), _scaled(member.get("morale_milli"))])
	else:
		lines.append("士气：%s" % _scaled(member.get("morale_milli")))
	lines.append("登记本籍省份（EU4编号）：%s" % _shown(member.get("home_province")))
	return {"text": "\n".join(lines)}

func _restore_selection() -> void:
	if not _items_by_key.has(selected_key):
		selected_key = ""
		_show_empty_selection()
		return
	var item: TreeItem = _items_by_key[selected_key]
	tree.set_selected(item, 0)
	_show_selection(item)

func _on_item_selected() -> void:
	var item := tree.get_selected()
	if item == null:
		selected_key = ""
		_show_empty_selection()
		return
	var metadata: Variant = item.get_metadata(0)
	if not metadata is Dictionary:
		_show_empty_selection()
		return
	selected_key = str(metadata.get("key", ""))
	_show_selection(item)

func _show_selection(item: TreeItem) -> void:
	var metadata: Variant = item.get_metadata(0)
	_selected_details = metadata.get("details", {})
	detail_label.text = str(_selected_details.get("text", ""))
	detail_label.visible = true
	_target_province_id = int(metadata.get("province_id", 0))
	location_button.disabled = _target_province_id <= 0
	location_button.visible = true

func _show_empty_selection() -> void:
	if detail_label == null:
		return
	detail_label.text = ""
	detail_label.visible = false
	location_button.disabled = true
	location_button.visible = false
	_target_province_id = 0

func _on_location_pressed() -> void:
	if _target_province_id > 0:
		province_selected.emit(_target_province_id)

func _shown(value: Variant) -> String:
	if value is int or value is float:
		return str(int(value))
	return "未载入" if value == null or str(value).is_empty() else str(value)

func _scaled(value: Variant) -> String:
	return "未载入" if value == null else "%.3f" % (float(value) / 1000.0)

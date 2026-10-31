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
	report_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_content.add_child(report_status)
	tree = Tree.new()
	tree.name = "NavalConstructionQueue"
	tree.custom_minimum_size.y = 150
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
	if _has_report:
		var selected := tree.get_selected()
		if selected != null:
			var metadata: Variant = selected.get_metadata(0)
			if metadata is Dictionary:
				selected_key = str(metadata.get("id", ""))
	_report_data = data.duplicate(true)
	_has_report = true
	_rebuild_tree()
	_update_header()
	_restore_selection()

func clear() -> void:
	_report_data.clear()
	_has_report = false
	selected_key = ""
	_items_by_key.clear()
	_target_province_id = 0
	if tree != null:
		tree.clear()
	_update_header()
	_show_empty_selection()

func _update_header() -> void:
	if section_button == null:
		return
	var orders: Array = _report_data.get("orders", []) if _has_report else []
	var summary := "暂无已接入的待造订单" if orders.is_empty() else "当前排期 %d 项" % orders.size()
	section_button.text = "造船 · %s %s" % [summary, "▼" if section_expanded else "▶"]
	if not _has_report:
		report_status.text = "暂无已接入的待造订单"
		report_status.tooltip_text = ""
		if tree != null:
			tree.visible = false
		return
	if report_status != null:
		var lines: Array[String] = []
		if orders.is_empty():
			lines.append("暂无已接入的待造订单")
		else:
			lines.append("排队中的订单日期会继续顺延。")
		var import_report: Dictionary = _report_data.get("import_report", {})
		var unresolved: Array = import_report.get("unresolved", [])
		if not unresolved.is_empty():
			lines.append("输入存档未接入记录（全局）：%d 项" % unresolved.size())
		report_status.text = "\n".join(lines)
		var diagnostic_lines: Array[String] = ["基线输入的全局导入诊断，不随项目快照恢复重计。"]
		var shown_count := mini(unresolved.size(), 8)
		for index in shown_count:
			var item: Variant = unresolved[index]
			if not item is Dictionary:
				continue
			var province_value: Variant = item.get("province_id")
			var reason := str(item.get("reason", "原因未提供"))
			if province_value == null:
				diagnostic_lines.append("省份未知：%s" % reason)
			else:
				diagnostic_lines.append("省份 %d：%s" % [int(province_value), reason])
		if unresolved.size() > shown_count:
			diagnostic_lines.append("另有 %d 项未展开。" % (unresolved.size() - shown_count))
		report_status.tooltip_text = "\n".join(diagnostic_lines)
	if tree != null:
		tree.visible = _has_report and not orders.is_empty()

func _rebuild_tree() -> void:
	tree.clear()
	_items_by_key.clear()
	var root := tree.create_item()
	var orders: Array = _report_data.get("orders", [])
	for order in orders:
		if order is Dictionary:
			_add_order(root, order)

func _add_order(root: TreeItem, order: Dictionary) -> void:
	var id := str(order.get("id", ""))
	var province_id := int(order.get("province_id", 0))
	var province_name := _shown(order.get("province_name"))
	var unit_type := _shown(order.get("unit_type"))
	var queue_value: Variant = order.get("queue_position")
	var queue_label := "独立订单" if queue_value == null else "队列第 %d 项" % int(queue_value)
	var title := "%s · %s · 港口：%s (%d)" % [queue_label, unit_type, province_name, province_id]
	var item := tree.create_item(root)
	item.set_text(0, title)
	item.set_tooltip_text(0, title)
	item.set_metadata(0, {
		"id": id,
		"province_id": province_id,
		"details": _order_details(order, queue_label, unit_type, province_name, province_id),
	})
	_items_by_key[id] = item

func _order_details(order: Dictionary, queue_label: String, unit_type: String, province_name: String, province_id: int) -> Dictionary:
	var date_value: Variant = order.get("completion_date")
	var schedule_date := "未载入"
	if date_value is Dictionary:
		schedule_date = "%04d.%02d.%02d" % [int(date_value.get("year", 0)), int(date_value.get("month", 0)), int(date_value.get("day", 0))]
	var details := "订单模板：%s\n建造港口：%s (%d)\n订单位置：%s\n当前排期日期：%s" % [unit_type, province_name, province_id, queue_label, schedule_date]
	return {"text": details}

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
	selected_key = str(metadata.get("id", ""))
	_show_selection(item)

func _show_selection(item: TreeItem) -> void:
	var metadata: Variant = item.get_metadata(0)
	var details: Dictionary = metadata.get("details", {})
	detail_label.text = str(details.get("text", ""))
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

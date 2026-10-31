# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends VBoxContainer

var section_button: Button
var report_status: Label
var tree: Tree
var detail_label: Label
var section_expanded := false
var selected_key := ""

var _content: VBoxContainer
var _report_data: Dictionary = {}
var _has_report := false
var _items_by_key: Dictionary = {}

func _ready() -> void:
	add_theme_constant_override("separation", 6)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	section_button = Button.new()
	section_button.custom_minimum_size.y = 40
	section_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	section_button.clip_text = true
	section_button.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
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
	tree.name = "EconomyHistory"
	tree.custom_minimum_size.y = 120
	tree.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tree.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
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
	if tree != null:
		tree.clear()
	_update_header()
	_show_empty_selection()

func _update_header() -> void:
	if section_button == null:
		return
	var treasury := "未载入"
	if _has_report:
		treasury = _format_milli(_report_data.get("treasury_milli"))
	section_button.text = "财政 · 国库 %s %s" % [treasury, "▼" if section_expanded else "▶"]
	section_button.tooltip_text = "当前国库：%s" % treasury
	if report_status != null:
		if not _has_report:
			report_status.text = "尚未载入"
			report_status.tooltip_text = ""
		else:
			report_status.text = "当前日期：%s · 第 %s 天\n当前国库：%s\n账本待结余额：%s（未计入国库）" % [
				_format_date(_report_data.get("date")),
				_exact_integer(_report_data.get("day")),
				treasury,
				_format_milli(_report_data.get("ledger_pending_milli")),
			]
			report_status.tooltip_text = report_status.text
	if tree != null:
		tree.visible = _has_report

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
	for key in ["tax", "production", "cashflow"]:
		_add_stage(root, key, expanded_keys)

func _add_stage(root: TreeItem, key: String, expanded_keys: Dictionary) -> void:
	var stage: Variant = _report_data.get(key)
	var title := _stage_name(key)
	var summary := ""
	if stage == null:
		summary = "未安装"
	else:
		var count := _exact_integer(stage.get("settlement_count"))
		var receipt: Variant = stage.get("last_receipt")
		if receipt is Dictionary:
			summary = "全局月结 %s 次 · 最近 %s" % [count, _receipt_date(receipt)]
		else:
			summary = "全局月结 %s 次 · 暂无该国月结记录" % count
	title += " · " + summary
	var item := tree.create_item(root)
	item.set_text(0, title)
	var details := _stage_details(key, stage)
	var tooltip := _stage_tooltip(details, stage)
	item.set_tooltip_text(0, tooltip)
	item.set_metadata(0, {"key": key, "details": details, "tooltip": tooltip})
	_items_by_key[key] = item
	if stage is Dictionary and stage.get("last_receipt") is Dictionary:
		var receipt: Dictionary = stage.get("last_receipt")
		var receipt_item := tree.create_item(item)
		var receipt_summary := "最近记录 · %s · 第 %s 天" % [_receipt_date(receipt), _exact_integer(receipt.get("day"))]
		receipt_item.set_text(0, receipt_summary)
		receipt_item.set_tooltip_text(0, tooltip)
		receipt_item.set_metadata(0, {"key": key, "details": details, "tooltip": tooltip})
	item.set_collapsed(not expanded_keys.has(key))

func _stage_details(key: String, stage: Variant) -> String:
	var lines: Array[String] = [_stage_name(key)]
	if stage == null:
		lines.append("状态：未安装")
		return "\n".join(lines)
	lines.append("全局月结次数：%s" % _exact_integer(stage.get("settlement_count")))
	var receipt: Variant = stage.get("last_receipt")
	if not receipt is Dictionary:
		lines.append("暂无该国月结记录。")
		return "\n".join(lines)
	lines.append("结算日期：%s · 第 %s 天" % [_receipt_date(receipt), _exact_integer(receipt.get("day"))])
	if key == "tax":
		lines.append("已知月税收：%s" % _format_milli(receipt.get("known_subtotal_milli")))
		lines.append("完整税收：%s" % _format_milli(receipt.get("amount_milli")))
	elif key == "production":
		lines.append("已知月产值：%s" % _format_milli(receipt.get("known_subtotal_milli")))
		lines.append("完整产值：%s" % _format_milli(receipt.get("amount_milli")))
	else:
		lines.append("现金流净额：%s" % _format_milli(receipt.get("amount_milli")))
	lines.append("阶段确认入账：%s" % _format_milli(receipt.get("credited_milli")))
	lines.append("记录时国库（历史值，不代表当前国库）")
	lines.append("结算前：%s" % _format_milli(receipt.get("treasury_before_milli")))
	lines.append("结算后：%s" % _format_milli(receipt.get("treasury_after_milli")))
	var unresolved: Variant = receipt.get("unresolved", [])
	if unresolved is Array and not unresolved.is_empty():
		lines.append("缺项原因：")
		for reason in unresolved:
			lines.append("· " + _reason_text(str(reason)))
	else:
		lines.append("缺项原因：无")
	return "\n".join(lines)

func _stage_tooltip(details: String, stage: Variant) -> String:
	if not stage is Dictionary or not stage.get("last_receipt") is Dictionary:
		return details
	var reasons: Array = stage.last_receipt.get("unresolved", [])
	if reasons.is_empty():
		return details
	return details + "\n诊断标识：\n" + "\n".join(reasons)

func _reason_text(reason: String) -> String:
	match reason:
		"unknown_treasury":
			return "国库余额尚未确定"
		"unknown_treasury_after_tax":
			return "税收阶段后的国库余额尚未确定"
		"unknown_treasury_after_production":
			return "普通生产阶段后的国库余额尚未确定"
		"unknown_cashflow":
			return "现金流金额尚未确定"
		"unknown_province_ownership":
			return "省份归属尚未确定"
		"unknown_external_country_annual_tax":
			return "国家年税收输入尚未确定"
	if reason.begins_with("province_input:") or reason in ["missing_production_input", "unknown_production_input"]:
		return "省份生产输入存在缺项（悬停查看诊断）"
	if reason.begins_with("province:"):
		return "省份年税收输入尚未确定（悬停查看诊断）"
	return "财政来源尚未完全解析（悬停查看诊断）"

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
	detail_label.text = str(metadata.get("details", ""))
	detail_label.tooltip_text = str(metadata.get("tooltip", detail_label.text))
	detail_label.visible = true

func _show_empty_selection() -> void:
	if detail_label == null:
		return
	detail_label.text = ""
	detail_label.tooltip_text = ""
	detail_label.visible = false

func _stage_name(key: String) -> String:
	match key:
		"tax":
			return "税收"
		"production":
			return "生产"
		"cashflow":
			return "现金流"
	return key

func _receipt_date(receipt: Dictionary) -> String:
	return _format_date(receipt.get("date"))

func _format_date(value: Variant) -> String:
	if not value is Dictionary:
		return "未载入"
	return "%04d.%02d.%02d" % [
		int(value.get("year", 0)),
		int(value.get("month", 0)),
		int(value.get("day", 0)),
	]

func _exact_integer(value: Variant) -> String:
	return "未载入" if value == null else str(value)

func _format_milli(value: Variant) -> String:
	if value == null:
		return "未载入"
	var raw := str(value)
	var negative := raw.begins_with("-")
	var digits := raw.substr(1) if negative else raw
	if digits.is_empty():
		return "未载入"
	for index in digits.length():
		var code := digits.unicode_at(index)
		if code < 48 or code > 57:
			return "未载入"
	while digits.length() < 4:
		digits = "0" + digits
	var whole := digits.substr(0, digits.length() - 3).lstrip("0")
	if whole.is_empty():
		whole = "0"
	var fraction := digits.substr(digits.length() - 3, 3)
	var sign := "-" if negative and (whole != "0" or fraction != "000") else ""
	return "%s%s.%s" % [sign, whole, fraction]

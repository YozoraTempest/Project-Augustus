# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends VBoxContainer

var section_button: Button
var report_status: Label
var view_choice: OptionButton
var tree: Tree
var detail_label: Label
var section_expanded := false
var selected_key := ""
var selected_view := "categories"

var _content: VBoxContainer
var _report_data: Dictionary = {}
var _has_report := false
var _items_by_key: Dictionary = {}

const VIEWS := [
	{"key": "categories", "label": "累计分类记录"},
	{"key": "sources", "label": "待归集来源"},
	{"key": "snapshot", "label": "归集快照"},
	{"key": "accumulator", "label": "待结转累计"},
	{"key": "rollover", "label": "结转快照"},
	{"key": "auxiliary", "label": "辅助支出记录"},
]

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
	view_choice = OptionButton.new()
	view_choice.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for view in VIEWS:
		view_choice.add_item(view.label)
		view_choice.set_item_metadata(view_choice.item_count - 1, view.key)
	view_choice.item_selected.connect(_on_view_selected)
	_content.add_child(view_choice)
	tree = Tree.new()
	tree.name = "CountryLedger"
	tree.custom_minimum_size.y = 180
	tree.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tree.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	tree.clip_contents = true
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
	_sync_view_choice()
	_show_empty_selection()

func toggle_section() -> void:
	section_expanded = not section_expanded
	_content.visible = section_expanded
	_update_header()

func render_report(data: Dictionary) -> void:
	if _has_report and data == _report_data:
		return
	var expanded_groups: Dictionary = {}
	if _has_report:
		_capture_expanded_groups(tree.get_root(), expanded_groups)
		_remember_selection()
	_report_data = data.duplicate(true)
	_has_report = true
	_update_header()
	_sync_view_choice()
	if not _has_ledger():
		selected_key = ""
		_items_by_key.clear()
		tree.clear()
		tree.visible = false
		view_choice.visible = false
		_show_empty_selection()
		return
	view_choice.visible = true
	tree.visible = true
	_rebuild_tree(expanded_groups)
	_restore_selection()

func clear() -> void:
	_report_data.clear()
	_has_report = false
	selected_key = ""
	_items_by_key.clear()
	if tree != null:
		tree.clear()
		tree.visible = false
	if view_choice != null:
		view_choice.visible = false
	_update_header()
	_show_empty_selection()

func _has_ledger() -> bool:
	return _has_report and _report_data.get("ledger") is Dictionary

func _update_header() -> void:
	if section_button == null:
		return
	var state := "尚未载入"
	if _has_report:
		state = "已载入" if _has_ledger() else "暂无记录"
	section_button.text = "分类账 · %s %s" % [state, "▼" if section_expanded else "▶"]
	section_button.tooltip_text = "当前国家分类账：%s" % state
	if report_status != null:
		if not _has_report:
			report_status.text = "尚未载入；选择国家后读取分类账。"
			report_status.tooltip_text = ""
		else:
			var date_text := _format_date(_report_data.get("date"))
			var day_text := _exact_integer(_report_data.get("day"))
			var ledger: Variant = _report_data.get("ledger")
			var pending := "无账本记录" if not ledger is Dictionary else _format_money(ledger.get("pending_milli"))
			var lines: Array[String] = [
				"查询国家：%s · 日期 %s · 第 %s 天" % [str(_report_data.get("country", "未知")), date_text, day_text],
				"待结余额：%s（单独记录，不计入国库）" % pending,
				"各向量不可相加为总收入或净额；此处只读，不自动归集、清算待结余额或执行年度结转。",
			]
			if _has_ledger() and selected_view == "snapshot":
				lines.append("归集执行情况与日期未知；全零向量不能证明已执行。原始总额字段：收入 %s · 支出 %s；筛选字段：收入 %s · 支出 %s。" % [
					_format_money(ledger.get("income_total_milli")),
					_format_money(ledger.get("expense_total_milli")),
					_format_money(ledger.get("income_filtered_total_milli")),
					_format_money(ledger.get("expense_filtered_total_milli")),
				])
			elif _has_ledger() and selected_view == "rollover":
				lines.append("结转执行情况与日期未知；全零向量不能证明已执行。原始总额字段：收入 %s · 支出 %s。" % [
					_format_money(ledger.get("income_rollover_total_milli")),
					_format_money(ledger.get("expense_rollover_total_milli")),
				])
			report_status.text = "\n".join(lines)
			report_status.tooltip_text = report_status.text

func _sync_view_choice() -> void:
	if view_choice == null:
		return
	var index := 0
	for view_index in VIEWS.size():
		if VIEWS[view_index].key == selected_view:
			index = view_index
			break
	selected_view = str(view_choice.get_item_metadata(index))
	view_choice.select(index)

func _on_view_selected(index: int) -> void:
	if index < 0 or index >= view_choice.item_count:
		return
	var next_view := str(view_choice.get_item_metadata(index))
	if next_view == selected_view:
		return
	var expanded_groups: Dictionary = {}
	_capture_expanded_groups(tree.get_root(), expanded_groups)
	_remember_selection()
	selected_view = next_view
	_update_header()
	_rebuild_tree(expanded_groups)
	_restore_selection()

func _remember_selection() -> void:
	if tree == null:
		return
	var selected := tree.get_selected()
	if selected == null:
		return
	var metadata: Variant = selected.get_metadata(0)
	if metadata is Dictionary:
		selected_key = str(metadata.get("key", ""))

func _capture_expanded_groups(item: TreeItem, expanded_groups: Dictionary) -> void:
	if item == null:
		return
	var metadata: Variant = item.get_metadata(0)
	if metadata is Dictionary and metadata.get("kind") == "group":
		expanded_groups[str(metadata.get("key", ""))] = not item.is_collapsed()
	for index in item.get_child_count():
		_capture_expanded_groups(item.get_child(index), expanded_groups)

func _rebuild_tree(expanded_groups: Dictionary) -> void:
	tree.clear()
	_items_by_key.clear()
	var root := tree.create_item()
	_add_group(root, "income", expanded_groups)
	_add_group(root, "expense", expanded_groups)

func _add_group(root: TreeItem, kind: String, expanded_groups: Dictionary) -> void:
	var group_item := tree.create_item(root)
	var group_name := "收入" if kind == "income" else "支出"
	group_item.set_text(0, group_name)
	group_item.set_metadata(0, {
		"key": kind,
		"kind": "group",
		"category": null,
		"native_key": "",
		"details": "%s分类账条目。" % group_name,
		"tooltip": "%s分类账条目。" % group_name,
	})
	_items_by_key[kind] = group_item
	var count := 19 if kind == "income" else 38
	var native_keys: Variant = _report_data.get("income_keys" if kind == "income" else "expense_keys", [])
	for category in count:
		var native_key := "未载入"
		if native_keys is Array and category < native_keys.size():
			native_key = str(native_keys[category])
		_add_category(group_item, kind, category, native_key)
	group_item.set_collapsed(not bool(expanded_groups.get(kind, true)))

func _add_category(parent: TreeItem, kind: String, category: int, native_key: String) -> void:
	var key := "%s:%d" % [kind, category]
	var amount := _amount_for(kind, category)
	var category_label := _category_name(kind, category)
	var details := _category_details(kind, category, native_key, amount)
	var item := tree.create_item(parent)
	item.set_text(0, "%s · %s" % [category_label, amount])
	item.set_tooltip_text(0, "native_key: %s\n%s" % [native_key, details])
	item.set_metadata(0, {
		"key": key,
		"kind": kind,
		"category": category,
		"native_key": native_key,
		"details": details,
		"tooltip": "native_key: %s\n%s" % [native_key, details],
	})
	_items_by_key[key] = item

func _amount_for(kind: String, category: int) -> String:
	var ledger: Dictionary = _report_data.get("ledger", {})
	var field := "income_milli" if kind == "income" else "expense_milli"
	match selected_view:
		"categories":
			var categories: Variant = ledger.get("categories", {})
			if not categories is Dictionary:
				return "无分类记录"
			var entry: Variant = categories.get(str(category))
			if entry == null and categories.has(category):
				entry = categories.get(category)
			if entry == null:
				return "无分类记录"
			if not entry is Dictionary or not entry.has(field):
				return "未载入"
			return _format_money(entry.get(field))
		"sources":
			return _vector_amount(ledger, "income_sources_milli" if kind == "income" else "expense_sources_milli", category)
		"snapshot":
			return _vector_amount(ledger, "income_snapshot_milli" if kind == "income" else "expense_snapshot_milli", category)
		"accumulator":
			return _vector_amount(ledger, "income_accumulator_milli" if kind == "income" else "expense_accumulator_milli", category)
		"rollover":
			return _vector_amount(ledger, "income_rollover_milli" if kind == "income" else "expense_rollover_milli", category)
		"auxiliary":
			if kind == "income":
				return "不适用"
			return _vector_amount(ledger, "expense_auxiliary_milli", category)
	return "未载入"

func _vector_amount(ledger: Dictionary, field: String, category: int) -> String:
	var values: Variant = ledger.get(field, [])
	if not values is Array or values.is_empty():
		return "未载入此向量"
	if category >= values.size():
		return "未载入"
	return _format_money(values[category])

func _category_details(kind: String, category: int, native_key: String, amount: String) -> String:
	var ledger: Dictionary = _report_data.get("ledger", {})
	var kind_name := "收入" if kind == "income" else "支出"
	var lines: Array[String] = [
		"类型：%s" % kind_name,
		"类别：%s（%d）" % [_category_name(kind, category), category],
		"native_key：%s" % native_key,
		"当前视图：%s" % _view_name(selected_view),
		"金额：%s" % amount,
		"待归集来源：%s" % _vector_amount(ledger, "income_sources_milli" if kind == "income" else "expense_sources_milli", category),
		"归集快照：%s" % _vector_amount(ledger, "income_snapshot_milli" if kind == "income" else "expense_snapshot_milli", category),
		"待结转累计：%s" % _vector_amount(ledger, "income_accumulator_milli" if kind == "income" else "expense_accumulator_milli", category),
		"结转快照：%s" % _vector_amount(ledger, "income_rollover_milli" if kind == "income" else "expense_rollover_milli", category),
		"累计%s分类记录：%s" % [kind_name, _category_record(ledger, category, "income_milli" if kind == "income" else "expense_milli")],
	]
	if selected_view == "snapshot" or selected_view == "rollover":
		lines.append("此快照没有执行日期；全零向量也不能证明已执行归集或结转。")
	if kind == "expense":
		lines.append("辅助支出记录：%s（不额外再加支出）" % _vector_amount(ledger, "expense_auxiliary_milli", category))
	else:
		lines.append("辅助支出记录：不适用（此向量仅记录支出）")
	return "\n".join(lines)

func _category_record(ledger: Dictionary, category: int, field: String) -> String:
	var categories: Variant = ledger.get("categories", {})
	if not categories is Dictionary:
		return "无分类记录"
	var entry: Variant = categories.get(str(category))
	if entry == null and categories.has(category):
		entry = categories.get(category)
	if entry == null:
		return "无分类记录"
	if not entry is Dictionary or not entry.has(field):
		return "未载入"
	return _format_money(entry.get(field))

func _on_item_selected() -> void:
	var item := tree.get_selected()
	if item == null:
		selected_key = ""
		_show_empty_selection()
		return
	var metadata: Variant = item.get_metadata(0)
	if not metadata is Dictionary:
		selected_key = ""
		_show_empty_selection()
		return
	selected_key = str(metadata.get("key", ""))
	_show_selection(item)

func _restore_selection() -> void:
	if not _items_by_key.has(selected_key):
		selected_key = ""
		_show_empty_selection()
		return
	var item: TreeItem = _items_by_key[selected_key]
	tree.set_selected(item, 0)
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

func _category_name(kind: String, category: int) -> String:
	if kind == "income":
		match category:
			0: return "税收"
			1: return "普通生产"
			2: return "贸易"
			3: return "黄金"
		return "收入分类 %d" % category
	match category:
		2: return "州维护"
		8: return "要塞维护"
		12: return "舰船建造"
	return "支出分类 %d" % category

func _view_name(key: String) -> String:
	for view in VIEWS:
		if view.key == key:
			return str(view.label)
	return key

func _format_date(value: Variant) -> String:
	if not value is Dictionary:
		return "未记录"
	return "%04d.%02d.%02d" % [int(value.get("year", 0)), int(value.get("month", 0)), int(value.get("day", 0))]

func _exact_integer(value: Variant) -> String:
	return "未载入" if value == null else str(value)

func _format_money(value: Variant) -> String:
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
	while digits.length() < 3:
		digits = "0" + digits
	var whole := digits.substr(0, digits.length() - 3).lstrip("0")
	if whole.is_empty():
		whole = "0"
	var fraction := digits.substr(digits.length() - 3, 3)
	var sign := "-" if negative and (whole != "0" or fraction != "000") else ""
	return "%s%s.%s" % [sign, whole, fraction]

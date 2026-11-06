# SPDX-License-Identifier: GPL-3.0-only
class_name ProvinceInfoPanel
extends PanelContainer

signal construction_requested(province_id: int, building: String)
signal recruitment_requested(province_id: int, kind: String)
signal recruitment_cancel_requested(province_id: int, order_id: int)
signal army_selected(army_id: int)
signal army_stop_requested(army_id: int)
signal armies_merge_requested(target: int, sources: PackedInt64Array)
signal army_split_requested(army_id: int, count: int)
signal army_disband_requested(army_id: int)
signal war_declare_requested(tag: String)

@onready var tabs: TabContainer = %Tabs
@onready var province_title: Label = %ProvinceTitle
@onready var province_body: Label = %ProvinceBody
@onready var country_button: Button = %ViewCountry
@onready var country_title: Label = %CountryTitle
@onready var country_body: Label = %CountryBody
@onready var country_color: ColorRect = %CountryColor
@onready var wars_label: Label = %Wars
@onready var declare_button: Button = %DeclareWar
@onready var declare_reason: Label = %DeclareWarReason
@onready var war_feedback: Label = %WarFeedback
@onready var construction_panel: ProvinceConstructionPanel = %ConstructionPanel
@onready var building_hint: Label = %BuildingHint
@onready var military_panel: ProvinceMilitaryPanel = %MilitaryPanel
@onready var military_hint: Label = %MilitaryHint

var _country_tag := ""


func _ready() -> void:
	country_button.pressed.connect(func(): tabs.current_tab = 1)
	construction_panel.construction_requested.connect(func(province_id: int, building: String): construction_requested.emit(province_id, building))
	military_panel.recruitment_requested.connect(func(province_id: int, kind: String): recruitment_requested.emit(province_id, kind))
	military_panel.recruitment_cancel_requested.connect(func(province_id: int, order_id: int): recruitment_cancel_requested.emit(province_id, order_id))
	military_panel.army_selected.connect(func(id: int): army_selected.emit(id))
	military_panel.army_stop_requested.connect(func(id: int): army_stop_requested.emit(id))
	military_panel.armies_merge_requested.connect(func(target: int, sources: PackedInt64Array): armies_merge_requested.emit(target, sources))
	military_panel.army_split_requested.connect(func(id: int, count: int): army_split_requested.emit(id, count))
	military_panel.army_disband_requested.connect(func(id: int): army_disband_requested.emit(id))
	declare_button.pressed.connect(func(): war_declare_requested.emit(_country_tag))
	show_info({}, {})


func show_info(province: Dictionary, country: Dictionary, construction: Dictionary = {}, military: Dictionary = {}, selected_army: int = 0, organization: Dictionary = {}) -> void:
	construction_panel.show_province(0 if province.is_empty() else province.id, construction)
	building_hint.visible = not construction_panel.visible
	military_panel.show_province(0 if province.is_empty() else province.id, military, selected_army, organization)
	military_hint.visible = not military_panel.visible
	# 字典仅用于显示，归属、发展度和首都仍由 Rust 世界管理。
	if province.is_empty():
		province_title.text = "未选择省份"
		province_body.text = ""
		country_button.hide()
	else:
		province_title.text = province.name
		var kind: String = {"land": "陆地", "sea": "海洋", "lake": "湖泊", "wasteland": "荒地"}[province.kind]
		province_body.text = "编号：#%d\n类型：%s" % [province.id, kind]
		if province.kind == "land":
			var owner := _country_name(province, "owner") if province.owner != null else "无主"
			province_body.text += "\n\n所属国家：%s\n控制者：%s\n\n文化：%s\n宗教：%s\n商品：%s\n\n总发展度：%s\n税收发展度：%s\n生产发展度：%s\n人力发展度：%s" % [owner, _country_name(province, "controller"), _value(province.culture), _value(province.religion), _value(province.trade_good), _value(province.development), _value(province.base_tax), _value(province.base_production), _value(province.base_manpower)]
			province_body.text += "\n\n基础月税：%s 金币\n建筑税收加成：+%s\n当前月度税收：%s 金币" % [_value(province.base_monthly_tax), _value(province.building_tax_bonus), _value(province.monthly_tax)]
		country_button.visible = not country.is_empty()
	province_body.visible = not province.is_empty()
	country_color.visible = not country.is_empty()
	if country.is_empty():
		country_title.text = "未选择国家" if province.is_empty() else "无所属国家"
		country_body.text = ""
	else:
		country_title.text = country.name
		country_color.color = country.color
		var capital := "—" if country.capital == null else "%s (#%d)" % [country.capital_name, country.capital]
		country_body.text = "国家标签：%s\n\n首都：%s\n主流文化：%s\n国教：%s\n\n领土省份：%d\n总发展度：%s" % [country.tag, capital, _value(country.primary_culture), _value(country.religion), country.province_count, _value(country.development)]
		country_body.text += "\n\n国库：%s 金币\n月度税收：+%s 金币\n陆军维护：-%s 金币（基础 %s，超额 %s）\n月度净收支：%s 金币" % [_value(country.treasury), _value(country.monthly_tax), _value(country.army_maintenance), _value(country.base_army_maintenance), _value(country.excess_army_maintenance), _value(country.monthly_balance)]
		country_body.text += "\n\n可用人力：%d / %d\n每月恢复：+%d 人" % [country.manpower.available, country.manpower.capacity, country.manpower.monthly_recovery]
		if country.last_settlement_month == null:
			country_body.text += "\n最近结算：尚未结算"
		else:
			country_body.text += "\n\n最近结算月份：%s\n实收税额：+%s 金币\n基础军费：-%s 金币\n超额军费：-%s 金币\n净收支：%s 金币\n结算日期：%s" % [country.last_settlement_month, country.last_tax, country.last_base_maintenance, country.last_excess_maintenance, country.last_balance, country.last_settlement_date]
		if country.history_conflicts:
			country_body.text += "\n\n历史信息存在冲突"
	country_body.visible = not country.is_empty()
	_show_wars(country)


func _value(value: Variant) -> String:
	return "—" if value == null else str(value)


func show_construction_error(error: String) -> void:
	construction_panel.show_error(error)


func show_recruitment_error(error: String) -> void:
	military_panel.show_error(error)


func show_movement_error(error: String) -> void:
	military_panel.show_movement_error(error)


func show_organization_error(error: String) -> void:
	military_panel.show_organization_error(error)


func show_disband_error(error: String) -> void:
	military_panel.show_disband_error(error)


func show_war_error(message: String) -> void:
	war_feedback.text = message
	war_feedback.modulate = Color(1.0, 0.55, 0.48)
	war_feedback.show()


func _show_wars(country: Dictionary) -> void:
	war_feedback.hide()
	var has_country := not country.is_empty()
	wars_label.visible = has_country
	if not has_country:
		_country_tag = ""
		declare_button.hide()
		declare_reason.hide()
		return
	_country_tag = country.tag
	var lines := PackedStringArray()
	for war: Dictionary in country.wars:
		lines.append("%s 向 %s 宣战\n开始日期：%s" % [war.attacker_name, war.defender_name, war.started_on])
	wars_label.text = "当前没有战争" if lines.is_empty() else "\n\n".join(lines)
	var player := GameSession.get_player_country()
	var foreign := not player.is_empty() and player != _country_tag
	declare_button.visible = foreign
	declare_button.disabled = not country.can_declare_war
	declare_button.tooltip_text = "" if country.can_declare_war else str(country.declare_war_reason)
	declare_reason.text = str(country.declare_war_reason)
	declare_reason.visible = foreign and not country.can_declare_war


func _country_name(province: Dictionary, field: String) -> String:
	if province[field] == null:
		return "—"
	var tag: String = province[field]
	var display_name: String = province[field + "_name"]
	return tag if display_name == tag else "%s (%s)" % [display_name, tag]

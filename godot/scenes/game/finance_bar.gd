# SPDX-License-Identifier: GPL-3.0-only
extends VBoxContainer

@onready var treasury_label: Label = %Treasury
@onready var tax_label: Label = %MonthlyTax
@onready var settlement_label: Label = %LastSettlement


func _ready() -> void:
	GameSession.clock_changed.connect(_refresh)
	_refresh()


func _refresh() -> void:
	# 顶部始终查询玩家国家，浏览地图上的其他国家不会切换玩家财政栏。
	var country := GameSession.get_player_country()
	if country.is_empty():
		treasury_label.text = "国库：—"
		tax_label.text = "月度税收：—"
		settlement_label.text = "尚未开局"
		return
	var finances := GameSession.backend.get_country_finances(country)
	treasury_label.text = "国库：%s 金币" % finances.treasury
	tax_label.text = "月度税收：+%s 金币" % finances.monthly_tax
	settlement_label.text = "最近结算：尚未结算" if finances.last_tax_month == null else "最近结算：%s · 实收税额 +%s 金币（%s 入库）" % [finances.last_tax_month, finances.last_tax_income, finances.last_tax_date]

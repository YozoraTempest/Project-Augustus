# SPDX-License-Identifier: GPL-3.0-only
extends VBoxContainer

@onready var treasury_label: Label = %Treasury
@onready var tax_label: Label = %MonthlyTax
@onready var maintenance_label: Label = %ArmyMaintenance
@onready var balance_label: Label = %MonthlyBalance
@onready var settlement_label: Label = %LastSettlement


func _ready() -> void:
	GameSession.clock_changed.connect(_refresh)
	GameSession.world_changed.connect(_refresh)
	_refresh()


func _refresh() -> void:
	# 顶部始终查询玩家国家，浏览地图上的其他国家不会切换玩家财政栏。
	var country := GameSession.get_player_country()
	if country.is_empty():
		treasury_label.text = "国库：—"
		tax_label.text = "月度税收：—"
		maintenance_label.text = "陆军维护：—"
		balance_label.text = "月度净收支：—"
		settlement_label.text = "尚未开局"
		return
	var finances := GameSession.backend.get_country_finances(country)
	treasury_label.text = "国库：%s 金币" % finances.treasury
	tax_label.text = "月度税收：+%s 金币" % finances.monthly_tax
	maintenance_label.text = "陆军维护：-%s 金币" % finances.army_maintenance
	balance_label.text = "月度净收支：%s 金币" % finances.monthly_balance
	balance_label.modulate = Color(1.0, 0.55, 0.48) if finances.in_deficit else Color(0.55, 0.88, 0.66)
	treasury_label.modulate = Color(1.0, 0.55, 0.48) if finances.treasury_negative else Color.WHITE
	settlement_label.text = "最近结算：尚未结算" if finances.last_settlement_month == null else "最近结算：%s · 税收 +%s · 军费 -%s · 净收支 %s 金币" % [finances.last_settlement_month, finances.last_tax, finances.last_maintenance, finances.last_balance]
	settlement_label.tooltip_text = "" if finances.last_settlement_date == null else "结算日期：" + str(finances.last_settlement_date)

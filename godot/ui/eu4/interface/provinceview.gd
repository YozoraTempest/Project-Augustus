# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 挂 interface/provinceview.gui。按文件加载并显示其中的窗口，玩法还没接。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const SOURCE := "interface/provinceview.gui"
const WINDOWS := [
	"provinceview",
	"provinceview_client_state",
	"janissaries_state_view",
	"pasha_state_view",
	"patriarch_state_view",
	"banners_state_view",
	"cawa_state_view",
	"top_section_window",
	"province_window",
	"queue_window",
	"progress_bar_window",
	"allied_objectives_list",
	"change_culture_view",
	"build_army_view",
	"build_navy_view",
	"foreign_province_window",
	"client_state_selection",
	"transfer_occupation_window",
	"tradegoods_province_window",
	"state_window",
	"buildings_window",
	"great_projects_window",
	"general_buildings_window",
	"province_values_window",
	"select_building",
	"select_building_large",
	"natives_buildings_window",
	"natives_buildings_window_large",
	"institutions_window",
	"history_window",
	"unit",
	"merc",
	"naval_unit",
	"queue_entry_merc",
	"tier_0_icon",
	"tier_1_icon",
	"tier_2_icon",
	"tier_3_icon",
	"allied_objective_country",
	"new_buildings_entry2",
	"edicts_window",
	"province_estate_entry",
	"province_modifier_item",
	"CoreShieldItem",
	"TerritorialCoreShieldItem",
	"ClaimShieldItem",
	"PermanentClaimShieldItem",
	"foreignviewview_diplistitem",
	"culture_item",
	"queue_entry",
	"TradeGoodsItem",
	"great_project_entry",
	"possible_building_entry",
	"building_progress_entry",
	"native_building_entry",
	"allies_selection_entry",
	"province_institution_entry",
	"state_province_entry",
	"state_edict_entry",
]

var gui


func open(directory: String, backend: AugustusMap) -> bool:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	scale = Vector2.ONE
	gui = GUI_FILE.new()
	add_child(gui)
	if not gui.open(directory, SOURCE, backend):
		return false
	for window_name in WINDOWS:
		if gui.find_named(str(window_name)) == null:
			return false
	if WINDOWS.is_empty():
		return false
	return show_window(str(WINDOWS[0]))


func show_window(window_name: String) -> bool:
	if gui == null or WINDOWS.find(window_name) < 0:
		return false
	gui.show_window(window_name)
	var shown: Node = gui.focus
	if shown == null or shown == gui:
		return false
	if str(shown.get_meta("eu4_name", "")) != window_name:
		return false
	if not shown is CanvasItem or not (shown as CanvasItem).visible:
		return false
	if not shown is Control:
		return false
	var box := shown as Control
	var width := box.offset_right - box.offset_left
	var height := box.offset_bottom - box.offset_top
	if width <= 0.0 or height <= 0.0:
		return false
	custom_minimum_size = Vector2(width, height)
	size = custom_minimum_size
	return true

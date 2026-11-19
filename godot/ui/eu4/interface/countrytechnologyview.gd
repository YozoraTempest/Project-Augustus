# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 科技。挂 interface/countrytechnologyview.gui。按文件加载并显示其中的窗口，玩法还没接。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const SOURCE := "interface/countrytechnologyview.gui"
const WINDOWS := [
	"countrytechnologyview",
	"countrytechnologyview_table_adm_tech",
	"countrytechnologyview_table_dip_tech",
	"countrytechnologyview_table_mil_tech",
	"countrytechnologyview_researchunititem",
	"countrytechnologyview_researchbuildingitem",
	"countrytechnologyview_researchmiltacitem",
	"countrytechnologyview_researchlandmoraleitem",
	"countrytechnologyview_researchsupplylimititem",
	"countrytechnologyview_researchnavalmoraleitem",
	"countrytechnologyview_researchnavalmaintenanceitem",
	"countrytechnologyview_researchtradeefficiencyitem",
	"countrytechnologyview_researchtraderangeitem",
	"countrytechnologyview_researchcolonialrangeitem",
	"countrytechnologyview_researchcolonialgrowthitem",
	"countrytechnologyview_researchsupportrebelsitem",
	"countrytechnologyview_enable_flag",
	"countrytechnologyview_researchgovernmentitem",
	"countrytechnologyview_researchallowedideagroupsitem",
	"countrytechnologyview_researchmaxstatessitem",
	"countrytechnologyview_researchproductionefficiencyitem",
	"countrytechnologyview_researchadministrativeefficiencyitem",
	"countrytechnologyview_researchdevelopmentefficiencyitem",
	"countrytechnologyview_researchacceptedculturesitem",
	"countrytechnologyview_researchcombatwidthitem",
	"countrytechnologyview_researchnavalengagementwidthitem",
	"countrytechnologyview_researchfireitem",
	"countrytechnologyview_researchshockitem",
	"countrytechnologyview_researchmaneuveritem",
	"countrytechnologyview_institution",
	"countrytechnologyview_requiredinstitution",
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
		if _root_window(str(window_name)) == null:
			return false
	if WINDOWS.is_empty():
		return false
	return show_window(str(WINDOWS[0]))


func show_window(window_name: String) -> bool:
	if gui == null or WINDOWS.find(window_name) < 0:
		return false
	gui.show_window(window_name)
	var shown: Node = gui.focus
	if shown == null or shown == gui or str(shown.get_meta("eu4_kind", "")) != "window" or str(shown.get_meta("eu4_name", "")) != window_name:
		shown = _root_window(window_name)
		if shown == null:
			return false
		for child in gui.get_children():
			if child is CanvasItem:
				(child as CanvasItem).visible = child == shown
		gui.focus = shown
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


func _root_window(window_name: String) -> Node:
	if gui == null:
		return null
	for child in gui.get_children():
		if str(child.get_meta("eu4_kind", "")) == "window" and str(child.get_meta("eu4_name", "")) == window_name:
			return child
	return null

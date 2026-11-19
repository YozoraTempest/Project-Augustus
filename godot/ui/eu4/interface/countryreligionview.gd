# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 宗教。挂 interface/countryreligionview.gui。按文件加载并显示其中的窗口，玩法还没接。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const SOURCE := "interface/countryreligionview.gui"
const WINDOWS := [
	"fervor_power_entry",
	"countryreligionview_provincelistitem",
	"religionlistitem",
	"religion_modifier_item",
	"religion_sub_modifier_item",
	"countryreligionview",
	"personal_deity_item",
	"orthodox_icon_item",
	"religious_school_item",
	"fetishist_cult_item",
	"countryreligionview_unlockedaspectitem",
	"countryreligionview_aspectitem_protestant",
	"countryreligionview_aspectitem_hussite",
	"countryreligionview_aspectitem_jewish",
	"countryreligionview_aspectitem_anglican",
	"countryreligionview_cardinalitem",
	"countryreligionview_blessingitem",
	"religionview_incident_entry",
	"religion_select_item",
	"harmonized_listbox_item",
	"guru_teaching_entry",
	"potential_guru_teaching_entry",
	"guru_new_teachings_list",
	"ancestor_entry",
	"potential_ancestor_entry",
	"potential_ancestors_list",
	"automated_missionary_button_entry",
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

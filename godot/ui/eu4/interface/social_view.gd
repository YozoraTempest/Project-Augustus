# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 挂 interface/social_view.gui。按文件加载并显示其中的窗口，玩法还没接。
const GUI_FILE := preload("res://ui/eu4/eu4_gui_file.gd")
const SOURCE := "interface/social_view.gui"
const WINDOWS := [
	"social_view",
	"social_chat_window_header_self",
	"social_chat_window_header_other",
	"social_chat_window_text_self",
	"social_chat_window_text_other",
	"social_chat_window_invite",
	"social_chat_window",
	"chat_history_frame",
	"chat_text_frame",
	"social_input_dialog",
	"text_frame",
	"social_message_dialog",
	"social_addfriend_window",
	"frame_search",
	"list_frame",
	"social_friends_window",
	"social_message_popup_window",
	"social_friends_context_menu",
	"social_addfriend_context_menu",
	"social_status_context_menu",
	"social_friends_list_item",
	"social_friendrequest_ingoing_list_item",
	"social_addfriend_list_item",
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

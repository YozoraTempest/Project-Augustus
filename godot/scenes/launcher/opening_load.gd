# SPDX-License-Identifier: GPL-3.0-only
extends Control

# 启动器上的开局载入。进度按真实阶段前进：核对目录、读取开局、准备主菜单。
const PENDING := Color(0.55, 0.6, 0.66, 1)
const ACTIVE := Color(1, 0.84, 0.43, 1)
const DONE := Color(0.78, 0.82, 0.86, 1)

@onready var step_label: Label = %Step
@onready var progress_bar: ProgressBar = %Progress
@onready var percent_label: Label = %Percent
@onready var step_check: Label = %StepCheck
@onready var step_read: Label = %StepRead
@onready var step_ready: Label = %StepReady


func run(directory: String) -> String:
	show()
	var path := directory.strip_edges()
	await _show_progress(0.06, 0, "准备载入")
	if path.is_empty():
		return "还没有安装目录"
	await _show_progress(0.16, 1, "核对安装目录")
	if not DirAccess.dir_exists_absolute(path):
		return "找不到安装目录"
	await _show_progress(0.38, 2, "读取开局")
	if not _opening_is_ready(path):
		var error := GameSession.load_game(path)
		if not error.is_empty():
			return "载入失败：" + error
	await _show_progress(0.72, 3, "完成载入")
	await _show_progress(1.0, 4, "载入完成")
	return ""


func _opening_is_ready(directory: String) -> bool:
	if not GameSession.has_game() or GameSession.is_started():
		return false
	return _same_path(GameSession.game_directory, directory)


func _show_progress(amount: float, stage: int, caption: String) -> void:
	progress_bar.value = amount * 100.0
	percent_label.text = "%d%%" % int(round(amount * 100.0))
	step_label.text = caption
	_mark_steps(stage)
	# 先把这一档画出来，再进入下一档或阻塞式读取。
	await get_tree().process_frame
	await get_tree().process_frame


func _mark_steps(stage: int) -> void:
	var rows: Array[Label] = [step_check, step_read, step_ready]
	var names := ["核对安装目录", "读取开局", "完成载入"]
	for index in rows.size():
		var label := rows[index]
		if stage > index + 1:
			label.text = "已完成  " + names[index]
			label.modulate = DONE
		elif stage == index + 1:
			label.text = "正在进行  " + names[index]
			label.modulate = ACTIVE
		else:
			label.text = names[index]
			label.modulate = PENDING


func _same_path(left: String, right: String) -> bool:
	return _canonical(left) == _canonical(right)


func _canonical(path: String) -> String:
	var cleaned := path.strip_edges().replace("\\", "/")
	while cleaned.ends_with("/"):
		cleaned = cleaned.trim_suffix("/")
	return cleaned.to_lower()

# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0
var temp_root := ""

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var fixture := ProjectSettings.globalize_path("res://../examples/world")
	temp_root = ProjectSettings.globalize_path("user://mission-selection-import-%d" % OS.get_process_id())
	var game_root := temp_root.path_join("game")
	var missions_dir := game_root.path_join("missions")
	var save_path := temp_root.path_join("mission-selection.eu4")
	check(copy_fixture(fixture, game_root), "copy the self-authored base content")
	check(DirAccess.make_dir_recursive_absolute(missions_dir) == OK, "create temporary mission directory")
	if failures:
		finish()
		return
	if not write_text(
		missions_dir.path_join("selection.txt"),
		"saved_series={slot=1 potential={always=no} saved_mission={position=1 trigger={always=yes} effect={}}}"
	):
		check(false, "write temporary mission content")
	if not write_text(
		save_path,
		"EU4txt\ndate=1445.1.1\nplayer=AAA\ncountries={AAA={treasury=0 country_missions={mission_slot={saved_series}}} BBB={treasury=0 country_missions={mission_slot={}}}}\nprovinces={}\n"
	):
		check(false, "write temporary EU4 save")
	if failures:
		finish()
		return

	var session = AugustusWorldSession.new()
	var mod_directories: Array[String] = []
	var loaded: Dictionary = JSON.parse_string(session.load_world_with_eu4_save(
		game_root,
		save_path,
		mod_directories,
		"",
		""
	))
	check(loaded.get("ok", false), "load EU4 save with mission content: %s" % loaded.get("error", ""))
	if not loaded.get("ok", false):
		finish()
		return

	var alpha: Dictionary = JSON.parse_string(session.query_country_missions("AAA"))
	var beta: Dictionary = JSON.parse_string(session.query_country_missions("BBB"))
	check(alpha.get("ok", false), "query AAA mission selection: %s" % alpha.get("error", ""))
	check(beta.get("ok", false), "query BBB mission selection: %s" % beta.get("error", ""))
	if not alpha.get("ok", false) or not beta.get("ok", false):
		finish()
		return

	var alpha_mission := find_mission(alpha.value, "saved_mission")
	var beta_mission := find_mission(beta.value, "saved_mission")
	check(alpha.value.selection_known, "AAA saved mission series is known")
	check(alpha_mission.get("selected") == true, "AAA's saved series selects its mission")
	check(beta.value.selection_known, "BBB's explicitly empty saved selection is known")
	check(beta_mission.get("selected") == false, "BBB's empty selection leaves the mission unselected")
	check(alpha_mission.get("completed") == false, "importing selection does not complete the mission")
	var snapshot: Dictionary = JSON.parse_string(session.save_snapshot())
	check(snapshot.get("ok", false), "save imported mission selection")
	if snapshot.get("ok", false):
		var claimed: Dictionary = JSON.parse_string(session.claim_mission("AAA", "saved_mission"))
		check(claimed.get("ok", false), "claim the imported selected mission")
		var restored: Dictionary = JSON.parse_string(session.restore_snapshot(snapshot.value))
		check(restored.get("ok", false), "restore the imported selection snapshot")
		var resumed_alpha: Dictionary = JSON.parse_string(session.query_country_missions("AAA"))
		var resumed_beta: Dictionary = JSON.parse_string(session.query_country_missions("BBB"))
		check(resumed_alpha == alpha and resumed_beta == beta, "snapshot restores selection and completion independently")
	finish()

func find_mission(report: Dictionary, mission_id: String) -> Dictionary:
	for mission in report.get("missions", []):
		if mission.get("id") == mission_id:
			return mission
	return {}

func write_text(path: String, contents: String) -> bool:
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return false
	file.store_string(contents)
	var error := file.get_error()
	file.close()
	return error == OK

func copy_fixture(source: String, destination: String) -> bool:
	if DirAccess.make_dir_recursive_absolute(destination) != OK:
		return false
	for name in DirAccess.get_files_at(source):
		if DirAccess.copy_absolute(source.path_join(name), destination.path_join(name)) != OK:
			return false
	for name in DirAccess.get_directories_at(source):
		if not copy_fixture(source.path_join(name), destination.path_join(name)):
			return false
	return true

func finish() -> void:
	cleanup()
	print("Augustus mission selection import integration: %d failures" % failures)
	quit(1 if failures else 0)

func cleanup() -> void:
	if temp_root.is_empty():
		return
	var expected := ProjectSettings.globalize_path("user://mission-selection-import-%d" % OS.get_process_id())
	if temp_root == expected:
		remove_fixture(temp_root)

func remove_fixture(path: String) -> void:
	if path != temp_root and not path.begins_with(temp_root + "/"):
		return
	for name in DirAccess.get_files_at(path):
		DirAccess.remove_absolute(path.path_join(name))
	for name in DirAccess.get_directories_at(path):
		remove_fixture(path.path_join(name))
	DirAccess.remove_absolute(path)

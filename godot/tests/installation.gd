# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Project Augustus contributors

extends SceneTree

var failures := 0

func _initialize() -> void:
	var options := parse_arguments(OS.get_cmdline_user_args())
	if not options.ok:
		push_error(options.error)
		quit(2)
		return

	var session := AugustusWorldSession.new()
	var loaded: Variant = JSON.parse_string(session.load_world_with_baseline_save(
		options.game_root,
		options.date,
		options.mods,
		options.user_directory,
		options.launcher_config,
		options.baseline_save
	))
	if not check_reply(loaded, "load the installed EU4 world"):
		finish()
		return
	if not options.baseline_save.is_empty():
		check(loaded.value.saved_imperial_baseline != null, "import baseline HRE membership and office preferences")

	var simulation: Variant = JSON.parse_string(session.simulation_view())
	if check_reply(simulation, "query the loaded calendar"):
		var date: Dictionary = simulation.value.date
		check(date.year == 1444 and date.month == 11 and date.day == 11, "start at 1444.11.11")

	var country: Variant = JSON.parse_string(session.query_country(options.country))
	if check_reply(country, "query the selected country"):
		check(not country.value.owned_provinces.is_empty(), "selected country has loaded provinces")

	var province: Variant = JSON.parse_string(session.query_province(options.province))
	if check_reply(province, "query the selected province"):
		check(province.value.owner == options.country, "selected province belongs to the selected country")

	var first_day: Variant = JSON.parse_string(session.advance(1))
	if check_reply(first_day, "advance the installed world by one day"):
		check_date(first_day.value.date, 1444, 11, 12, "advance to 1444.11.12")

	var first_month: Variant = JSON.parse_string(session.advance(29))
	if check_reply(first_month, "advance the installed world through the first month"):
		check_date(first_month.value.date, 1444, 12, 11, "advance to 1444.12.11")
		check(first_month.value.tax_settlements == 1, "settle the first month")
		check(first_month.value.last_tax_day == 20, "record the first monthly settlement day")

	var snapshot: Variant = JSON.parse_string(session.save_snapshot())
	if check_reply(snapshot, "save the installed campaign"):
		var restored: Variant = JSON.parse_string(session.restore_snapshot(snapshot.value))
		if check_reply(restored, "restore the installed campaign"):
			check_date(restored.value.date, 1444, 12, 11, "restore at 1444.12.11")
			var second_month: Variant = JSON.parse_string(session.advance(30))
			if check_reply(second_month, "advance the restored campaign"):
				check_date(second_month.value.date, 1445, 1, 10, "advance to 1445.01.10")
				check(second_month.value.tax_settlements == 2, "settle the second month after restore")

	print("Augustus installed-game smoke: %d failures" % failures)
	finish()

func parse_arguments(args: Array[String]) -> Dictionary:
	var result := {
		"ok": true,
		"error": "",
		"game_root": "",
		"date": "1444.11.11",
		"mods": [] as Array[String],
		"user_directory": "",
		"launcher_config": "",
		"baseline_save": "",
		"country": "HAB",
		"province": 134,
	}
	var values := ["--game-root", "--date", "--mod", "--user-directory", "--launcher-config", "--baseline-save", "--country", "--province"]
	var index := 0
	while index < args.size():
		var option := args[index]
		if option not in values:
			return {"ok": false, "error": "unknown option: %s" % option}
		if index + 1 >= args.size():
			return {"ok": false, "error": "missing value for %s" % option}
		var value := args[index + 1]
		match option:
			"--game-root":
				result.game_root = value
			"--date":
				result.date = value
			"--mod":
				result.mods.append(value)
			"--user-directory":
				result.user_directory = value
			"--launcher-config":
				result.launcher_config = value
			"--baseline-save":
				result.baseline_save = value
			"--country":
				result.country = value
			"--province":
				if not value.is_valid_int():
					return {"ok": false, "error": "invalid province identifier"}
				result.province = value.to_int()
		index += 2
	if result.game_root.is_empty():
		return {"ok": false, "error": "--game-root is required"}
	return result

func check_reply(reply: Variant, message: String) -> bool:
	if reply is Dictionary and reply.get("ok", false):
		return true
	check(false, "%s: %s" % [message, JSON.stringify(reply)])
	return false

func check(condition: bool, message: String) -> void:
	if not condition:
		push_error(message)
		failures += 1

func check_date(date: Dictionary, year: int, month: int, day: int, message: String) -> void:
	check(date.year == year and date.month == month and date.day == day, message)

func finish() -> void:
	quit(1 if failures else 0)

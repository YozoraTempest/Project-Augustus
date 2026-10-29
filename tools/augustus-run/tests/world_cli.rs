// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use jomini::{binary::Token, common::Date};
use std::{fs, path::Path, process::Command};

fn binary_startup_save() -> Vec<u8> {
    let date = Date::parse("1444.11.11").unwrap().to_binary();
    let mut bytes = b"EU4bin".to_vec();
    for token in [
        Token::Id(0x3000),
        Token::Equal,
        Token::I32(date),
        Token::Id(0x3001),
        Token::Equal,
        Token::Lookup(7),
        Token::Id(0x3003),
        Token::Equal,
        Token::Open,
        Token::Id(0x3002),
        Token::Equal,
        Token::Open,
        Token::Id(0x3005),
        Token::Equal,
        Token::F32(12_345_i32.to_le_bytes()),
        Token::Close,
        Token::Close,
        Token::Id(0x3004),
        Token::Equal,
        Token::Open,
        Token::Close,
    ] {
        token.write(&mut bytes).unwrap();
    }
    bytes
}

fn plaintext_startup_save() -> &'static str {
    r#"EU4txt
date=1444.11.11
player="AAA"
countries={
	AAA={ human=yes prestige=5.000 }
	BBB={ human=no prestige=0.000 }
}
provinces={
	-1={ owner="AAA" controller="AAA" }
}
"#
}

#[test]
fn world_cli_resumes_saved_naval_construction_without_charging_or_completing_twice() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    copy_directory(&fixture, &game_root);
    let map_path = game_root.join("map/default.map");
    let map = fs::read_to_string(&map_path).unwrap();
    fs::write(&map_path, format!("{map}\nforce_coastal={{1}}\n")).unwrap();
    let save_path = temp.path().join("naval-order.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={ AAA={ human=yes treasury=20.000 } BBB={ human=no } }
provinces={
    -1={
        owner="AAA" controller="AAA"
        military_construction={
            country="AAA" type="sample_light_ship"
            start_date=1444.11.10 total=3 original_total=3
            progress=0.333 date=1444.11.13 cost=5.000
        }
    }
}
"#,
    )
    .unwrap();

    let run = |days: &str, load: Option<&Path>, save: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        command
            .arg("--game-root")
            .arg(&game_root)
            .arg("--eu4-save")
            .arg(&save_path)
            .args(["--days", days, "--country", "AAA"]);
        if let Some(load) = load {
            command.arg("--load").arg(load);
        }
        if let Some(save) = save {
            command.arg("--save").arg(save);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let before_path = temp.path().join("before.json");
    let before = run("2", None, Some(&before_path));
    assert_eq!(before["naval_construction_import"]["imported_count"], 1);
    assert!(
        before["naval_construction_import"]["unresolved"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(before["simulation"]["pending_naval_constructions"], 1);
    assert_eq!(before["simulation"]["pending_tasks"], 0);
    assert_eq!(before["simulation"]["naval_units"], 0);
    assert_eq!(before["country"]["resources"]["treasury_milli"], 20_000);

    let resumed_path = temp.path().join("resumed.json");
    let resumed = run("1", Some(&before_path), Some(&resumed_path));
    assert_eq!(resumed["simulation"]["pending_naval_constructions"], 0);
    assert_eq!(resumed["simulation"]["naval_units"], 1);
    assert_eq!(resumed["simulation"]["fleets"], 1);
    assert_eq!(resumed["country"]["resources"]["treasury_milli"], 20_000);
    let continuous_path = temp.path().join("continuous.json");
    run("3", None, Some(&continuous_path));
    assert_eq!(
        fs::read_to_string(&resumed_path).unwrap(),
        fs::read_to_string(&continuous_path).unwrap()
    );
    let later = run("4", Some(&resumed_path), None);
    assert_eq!(later["simulation"]["pending_naval_constructions"], 0);
    assert_eq!(later["simulation"]["naval_units"], 1);
    assert_eq!(later["country"]["resources"]["treasury_milli"], 20_000);

    let unsupported = fs::read_to_string(&save_path)
        .unwrap()
        .replace("sample_light_ship", "unavailable_hull");
    fs::write(&save_path, unsupported).unwrap();
    let unsupported = run("0", None, None);
    assert_eq!(
        unsupported["naval_construction_import"]["imported_count"],
        0
    );
    assert_eq!(unsupported["simulation"]["pending_naval_constructions"], 0);
    assert_eq!(unsupported["simulation"]["naval_units"], 0);
    assert!(
        unsupported["naval_construction_import"]["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["province_id"] == 1
                && entry["reason"]
                    .as_str()
                    .unwrap()
                    .contains("unavailable_hull"))
    );
}

fn check_priced_naval_cli_round_trip(classified: bool) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    copy_directory(&fixture, &game_root);
    let map_path = game_root.join("map/default.map");
    let map = fs::read_to_string(&map_path).unwrap();
    fs::write(map_path, format!("{map}\nforce_coastal={{1}}\n")).unwrap();
    let save = temp.path().join("baseline.eu4");
    fs::write(&save, "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={human=yes treasury=20.000} BBB={human=no}}\nprovinces={-1={owner=AAA controller=AAA}}\n").unwrap();
    let requests = temp.path().join("requests.json");
    fs::write(
        &requests,
        serde_json::to_vec(&serde_json::json!([{
            "country": "AAA", "province_id": 1, "unit_type": "sample_light_ship",
            "completion_day": 3, "cost_milli": 5000
        }]))
        .unwrap(),
    )
    .unwrap();
    let run = |days: &str, submit: bool, load: Option<&Path>, output: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        command
            .arg("--game-root")
            .arg(&game_root)
            .arg("--eu4-save")
            .arg(&save)
            .args(["--country", "AAA", "--days", days]);
        if submit {
            command.arg("--naval-constructions").arg(&requests);
            if classified {
                command.arg("--naval-construction-ledger");
            }
        }
        if let Some(load) = load {
            command.arg("--load").arg(load);
        }
        if let Some(output) = output {
            command.arg("--save").arg(output);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let baseline = run("0", false, None, None);
    let pending_path = temp.path().join("pending.json");
    let pending = run("1", true, None, Some(&pending_path));
    assert_eq!(
        pending["submitted_naval_constructions"],
        serde_json::json!([0])
    );
    assert_eq!(pending["simulation"]["pending_naval_constructions"], 1);
    assert_eq!(pending["simulation"]["naval_units"], 0);
    assert_eq!(pending["country"]["resources"]["treasury_milli"], 15000);
    if classified {
        assert_eq!(
            pending["country"]["ledger"]["expense_sources_milli"][12],
            5000
        );
        assert_eq!(
            pending["country"]["ledger"]["expense_accumulator_milli"][12],
            5000
        );
        assert_eq!(
            pending["country"]["ledger"]["expense_auxiliary_milli"][12],
            5000
        );
        assert_eq!(pending["country"]["ledger"]["pending_milli"], 0);
    } else {
        assert_eq!(
            pending["country"].get("ledger"),
            baseline["country"].get("ledger")
        );
    }
    let resumed_path = temp.path().join("resumed.json");
    let resumed = run("2", false, Some(&pending_path), Some(&resumed_path));
    assert!(resumed.get("submitted_naval_constructions").is_none());
    assert_eq!(resumed["simulation"]["pending_naval_constructions"], 0);
    assert_eq!(resumed["simulation"]["naval_units"], 1);
    assert_eq!(resumed["country"]["resources"]["treasury_milli"], 15000);
    if classified {
        assert_eq!(resumed["country"]["ledger"], pending["country"]["ledger"]);
    }
    let continuous_path = temp.path().join("continuous.json");
    run("3", true, None, Some(&continuous_path));
    assert_eq!(
        fs::read_to_string(resumed_path).unwrap(),
        fs::read_to_string(continuous_path).unwrap()
    );
}

#[test]
fn world_cli_submits_priced_naval_orders_once_and_restores_progress() {
    check_priced_naval_cli_round_trip(false);
}

#[test]
fn world_cli_classifies_naval_order_expenses_once_and_restores_progress() {
    check_priced_naval_cli_round_trip(true);
}

#[test]
fn world_cli_reports_live_production_contribution_to_ship_recruit_speed() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("game");
    copy_directory(&fixture, &game_root);
    fs::write(
        game_root.join("common/static_modifiers/ship_speed.txt"),
        "provincial_production_size = { ship_recruit_speed = -0.01 }\n",
    )
    .unwrap();
    let province_path = game_root.join("history/provinces/1 - North.txt");
    let history = fs::read_to_string(&province_path).unwrap();
    let run = |production: &str| {
        fs::write(
            &province_path,
            history.replace(
                "base_production = 2",
                &format!("base_production = {production}"),
            ),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(&game_root)
            .args(["--province", "1", "--seed", "7", "--ai-only"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        output["province"]["attributes"]["province_ship_recruit_speed_modifier"].clone()
    };
    for (production, expected) in [("3", -30000), ("4", -40000)] {
        let attribute = run(production);
        assert_eq!(attribute["known_subtotal_micros"], expected);
        assert!(attribute["value_micros"].is_null());
        assert!(
            attribute["unresolved"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reason| reason
                    .as_str()
                    .unwrap()
                    .contains("remaining_sources_not_bound"))
        );
    }
}

#[test]
fn world_cli_imports_overlays_exports_and_restores_native_development() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let save_path = temp.path().join("native-development.eu4");
    fs::write(
        &save_path,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={
        human=yes
        native={ adm_dev=1.112 dip_dev=1.146 mil_dev=1.275 }
        history={ native={ adm_dev=9.999 } }
    }
    BBB={ human=no }
}
provinces={ -1={ owner="AAA" controller="AAA" } }
"#,
    )
    .unwrap();
    let overlay_path = temp.path().join("overlay.json");
    fs::write(
        &overlay_path,
        r#"{"country_metric_values":{"AAA":{"native_dip_development":2146,"native_mil_development":null}}}"#,
    )
    .unwrap();
    let run = |overlay: Option<&Path>,
               load: Option<&Path>,
               save: Option<&Path>,
               export: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        command
            .arg("--game-root")
            .arg(&fixture)
            .arg("--eu4-save")
            .arg(&save_path)
            .args([
                "--country",
                "AAA",
                "--days",
                if load.is_some() { "1" } else { "0" },
            ]);
        for (flag, path) in [
            ("--external-state", overlay),
            ("--load", load),
            ("--save", save),
            ("--export-external-state", export),
        ] {
            if let Some(path) = path {
                command.arg(flag).arg(path);
            }
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let original = run(None, None, None, None);
    assert_eq!(
        original["country"]["metrics"]["native_adm_development"],
        1112
    );
    assert_eq!(
        original["country"]["metrics"]["native_dip_development"],
        1146
    );
    assert_eq!(
        original["country"]["metrics"]["native_mil_development"],
        1275
    );

    let snapshot_path = temp.path().join("snapshot.json");
    let export_path = temp.path().join("external.json");
    let overlaid = run(
        Some(&overlay_path),
        None,
        Some(&snapshot_path),
        Some(&export_path),
    );
    let resumed = run(Some(&overlay_path), Some(&snapshot_path), None, None);
    for output in [&overlaid, &resumed] {
        let metrics = output["country"]["metrics"].as_object().unwrap();
        assert_eq!(metrics["native_adm_development"], 1112);
        assert_eq!(metrics["native_dip_development"], 2146);
        assert!(metrics.contains_key("native_mil_development"));
        assert!(metrics["native_mil_development"].is_null());
    }
    let external: serde_json::Value =
        serde_json::from_slice(&fs::read(&export_path).unwrap()).unwrap();
    for name in [
        "native_adm_development",
        "native_dip_development",
        "native_mil_development",
    ] {
        assert_eq!(
            external["country_metric_values"]["AAA"][name],
            overlaid["country"]["metrics"][name]
        );
    }

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&export_path)
        .args(["--country", "AAA", "--seed", "7", "--ai-only"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let imported: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in [
        "native_adm_development",
        "native_dip_development",
        "native_mil_development",
    ] {
        assert_eq!(
            imported["country"]["metrics"][name],
            overlaid["country"]["metrics"][name]
        );
    }
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn world_cli_reports_state_sources_and_query_failures() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args([
            "--country",
            "AAA",
            "--province",
            "1",
            "--economy",
            "--technology",
            "administrative",
            "--modifier",
            "test_river",
            "--unit",
            "sample_light_ship",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["report"].is_object());
    assert_eq!(value["simulation"]["pending_tasks"], 0);
    assert_eq!(value["simulation"]["fleets"], 0);
    assert_eq!(value["country"]["bound_capital"], 2);
    assert_eq!(value["province"]["owner"], "BBB");
    assert_eq!(value["province_sources"][0]["line"], 2);
    assert_eq!(value["country"]["politics"]["ruler"]["name"], "Iris");
    assert_eq!(value["country"]["diplomacy"].as_array().unwrap().len(), 5);
    assert_eq!(
        value["province"]["buildings"],
        serde_json::json!(["test_fort"])
    );
    assert_eq!(value["province"]["trade_node"], "river");
    assert_eq!(value["province_trade_good_base_price_milli"], 2125);
    assert_eq!(value["province_trade_good_price_milli"], 2125);
    assert_eq!(value["country"]["technology"]["administrative"], 3);
    assert_eq!(
        value["country"]["selected_unit_templates"]["infantry"],
        "test_infantry"
    );
    assert_eq!(
        value["country"]["selected_unit_templates"]["cavalry"],
        "test_cavalry"
    );
    assert_eq!(
        value["country"]["available_unit_templates"]["artillery"],
        serde_json::json!(["test_artillery"])
    );
    assert_eq!(value["country"]["resources"]["mercantilism_milli"], 18500);
    assert_eq!(
        value["province"]["attributes"]["tax_basis"]["known_subtotal_micros"],
        212000
    );
    assert!(value["province"]["attributes"]["tax_basis"]["value_micros"].is_null());
    assert_eq!(value["province"]["administration"]["area"], "north_area");
    assert_eq!(
        value["province"]["administration"]["history_autonomy_delta_micros"],
        0
    );
    assert_eq!(
        value["province"]["administration"]["raw_autonomy_micros"],
        300000
    );
    assert_eq!(
        value["country"]["administrative_areas"]["east_area"],
        "state"
    );
    assert_eq!(
        value["province"]["attributes"]["tax_after_autonomy"]["known_subtotal_micros"],
        212000
    );
    assert_eq!(
        value["country"]["province_attribute_totals"]["tax_basis"]["known_subtotal_micros"],
        1100000
    );
    assert_eq!(
        value["modifier"]["definition"]["values_micros"]["test_growth"],
        -2500
    );
    assert_eq!(
        value["technology"]["levels"][0]["values_micros"]["test_yield"],
        12500
    );
    assert_eq!(
        value["unit_template"]["definition"]["category"],
        "light_ship"
    );
    assert_eq!(
        value["unit_template"]["unlocked_at"][0]["branch"],
        "diplomatic"
    );
    assert_eq!(value["unit_template"]["unlocked_at"][0]["level"], 3);
    assert_eq!(value["unit_template_source"]["line"], 4);
    assert_eq!(
        value["economy"]["buildings"]["test_market"]["cost_milli"],
        87125
    );
    assert_eq!(
        value["economy"]["trade_nodes"][1]["members"],
        serde_json::json!([1])
    );
    let compact = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--country", "AAA", "--province", "1", "--omit-load-report"])
        .output()
        .unwrap();
    assert!(
        compact.status.success(),
        "{}",
        String::from_utf8_lossy(&compact.stderr)
    );
    let compact: serde_json::Value = serde_json::from_slice(&compact.stdout).unwrap();
    assert!(compact.get("report").is_none());
    assert!(compact["simulation"].is_object());
    assert_eq!(compact["country"]["bound_capital"], 2);
    assert_eq!(compact["province"]["owner"], "BBB");
    let all = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--diplomacy")
        .output()
        .unwrap();
    assert!(all.status.success());
    let all: serde_json::Value = serde_json::from_slice(&all.stdout).unwrap();
    assert_eq!(all["political_offices"]["imperial_emperor"], "AAA");
    assert_eq!(all["diplomacy"].as_array().unwrap().len(), 5);
    let error = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .args(["--province", "999"])
        .output()
        .unwrap();
    assert!(!error.status.success());
    assert!(error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&error.stderr).contains("unknown province"));
}

#[test]
fn diplomacy_query_reports_active_historical_truces() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    copy_directory(&fixture, temp.path());

    let defines = temp.path().join("common/defines.lua");
    let contents = fs::read_to_string(&defines).unwrap();
    let defines_with_truce = contents.replace(
        "NMilitary = {",
        "NDiplomacy = { TRUCE_YEARS = 1 },\nNMilitary = {",
    );
    assert_ne!(defines_with_truce, contents);
    fs::write(defines, defines_with_truce).unwrap();

    let history = temp.path().join("history/countries/AAA - Alpha.txt");
    let contents = fs::read_to_string(&history).unwrap();
    fs::write(&history, format!("{contents}\nadd_truce_with = BBB\n")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(temp.path())
        .arg("--diplomacy")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["truces"],
        serde_json::json!([{
            "first": "AAA",
            "second": "BBB",
            "expires_on": "1445.11.11",
            "monthly": true,
        }])
    );
}

#[test]
fn diplomacy_query_reports_saved_country_relation_state() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let external_state = temp.path().join("relation-state.json");
    fs::write(
        &external_state,
        serde_json::json!({
            "country_relation_states": {
                "AAA": {
                    "BBB": { "attitude": "probe_attitude", "trust_value": 42 }
                },
                "BBB": {
                    "AAA": { "attitude": "other_probe_attitude", "trust_value": 7 }
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external_state)
        .arg("--diplomacy")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["country_relation_states"],
        serde_json::json!({
            "AAA": {
                "BBB": { "attitude": "probe_attitude", "trust_value": 42 }
            },
            "BBB": {
                "AAA": { "attitude": "other_probe_attitude", "trust_value": 7 }
            }
        })
    );

    let filtered = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external_state)
        .args(["--country", "AAA", "--diplomacy", "--omit-load-report"])
        .output()
        .unwrap();
    assert!(
        filtered.status.success(),
        "{}",
        String::from_utf8_lossy(&filtered.stderr)
    );
    let filtered: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert_eq!(
        filtered["country_relation_states"],
        serde_json::json!({
            "AAA": {
                "BBB": { "attitude": "probe_attitude", "trust_value": 42 }
            }
        })
    );
}

#[test]
fn world_cli_loads_eu4_save_and_executes_delayed_startup_action() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    copy_directory(&source, &fixture);
    let save = temp.path().join("startup.eu4");
    fs::write(&save, plaintext_startup_save()).unwrap();

    let run = |snapshot: Option<&Path>, days: Option<&str>, save_snapshot: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        command
            .arg("--game-root")
            .arg(&fixture)
            .arg("--eu4-save")
            .arg(&save);
        if let Some(snapshot) = snapshot {
            command.arg("--load").arg(snapshot);
        }
        if let Some(days) = days {
            command.args(["--days", days]);
        }
        if let Some(save_snapshot) = save_snapshot {
            command.arg("--save").arg(save_snapshot);
        }
        let output = command
            .args(["--country", "AAA", "--omit-load-report"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let without_delay = run(None, None, None);
    assert_eq!(without_delay["simulation"]["pending_tasks"], 0);
    assert_eq!(without_delay["country"]["metrics"]["prestige"], 5_000);

    let on_actions = fixture.join("common/on_actions");
    fs::create_dir_all(&on_actions).unwrap();
    fs::write(
        on_actions.join("startup.txt"),
        "on_startup = { if = { limit = { tag = AAA prestige = 5 } country_event = { id = probe.1 days = 5 } } }",
    )
    .unwrap();
    let events = fixture.join("events");
    fs::create_dir_all(&events).unwrap();
    fs::write(
        events.join("startup.txt"),
        "namespace = probe country_event = { id = probe.1 hidden = yes is_triggered_only = yes immediate = { add_prestige = 2 } }",
    )
    .unwrap();

    let pending_snapshot = temp.path().join("startup-pending.json");
    let scheduled = run(None, Some("0"), Some(&pending_snapshot));
    assert_eq!(scheduled["simulation"]["pending_tasks"], 1);
    assert_eq!(scheduled["country"]["metrics"]["prestige"], 5_000);

    let before_due = run(Some(&pending_snapshot), Some("4"), None);
    assert_eq!(before_due["simulation"]["pending_tasks"], 1);
    assert_eq!(before_due["country"]["metrics"]["prestige"], 5_000);

    let due = run(Some(&pending_snapshot), Some("5"), None);
    assert_eq!(due["simulation"]["pending_tasks"], 0);
    assert_eq!(due["country"]["metrics"]["prestige"], 7_000);
}

#[test]
fn world_cli_loads_external_state_from_another_simulation() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let external = temp.path().join("external-state.json");
    fs::write(
        &external,
        serde_json::json!({
            "country_metric_values": {
                "AAA": {"prestige": 26942}
            },
            "country_resources": {
                "AAA": {
                    "treasury_milli": 1234,
                    "manpower_milli": 5000,
                    "mercantilism_milli": 18500
                }
            },
            "country_monthly_cashflows_milli": {
                "AAA": 125
            },
            "production_inputs": {
                "2": {
                    "input": {
                        "base_value_milli": 1000,
                        "country_modifier_milli": 0,
                        "legacy_province_modifier_milli": 0,
                        "queried_province_modifier_milli": 0
                    },
                    "unresolved": []
                }
            },
            "production_value_inputs": {
                "1": {
                    "goods_size_milli": 1000,
                    "flat_modifier_milli": 0,
                    "external_flat_modifier_milli": 0,
                    "rate_modifier_milli": 0,
                    "external_rate_modifier_milli": 0,
                    "country_modifier_milli": 0,
                    "legacy_province_modifier_milli": 0,
                    "queried_province_modifier_milli": 0
                }
            },
            "armies": [
                {
                    "owner": "AAA",
                    "location": 1,
                    "name": "CLI Army",
                    "regiments": ["test_infantry"]
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let initial_output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args(["--country", "AAA", "--province", "1"])
        .output()
        .unwrap();
    assert!(
        initial_output.status.success(),
        "{}",
        String::from_utf8_lossy(&initial_output.stderr)
    );
    let initial_value: serde_json::Value = serde_json::from_slice(&initial_output.stdout).unwrap();
    assert_eq!(
        initial_value["country"]["resources"]["treasury_milli"],
        1234
    );
    assert_eq!(
        initial_value["country"]["resources"]["manpower_milli"],
        5000
    );
    assert_eq!(initial_value["country"]["metrics"]["prestige"], 26942);

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--country",
            "AAA",
            "--province",
            "1",
            "--days",
            "30",
            "--player",
            "AAA",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["country"]["resources"]["treasury_milli"].is_null());
    assert_eq!(value["province_trade_good_base_price_milli"], 2125);
    assert_eq!(value["province_trade_good_price_milli"], 2125);
    assert_eq!(value["simulation"]["day"], 30);
    assert_eq!(value["simulation"]["cashflow_settlements"], 1);
    assert_eq!(value["simulation"]["last_cashflow_day"], 20);
    assert_eq!(value["simulation"]["production_settlements"], 1);
    assert_eq!(value["simulation"]["last_production_day"], 20);
    assert_eq!(value["simulation"]["armies"], 1);
    assert_eq!(value["simulation"]["naval_units"], 0);
    assert!(value["country"]["last_monthly_production"].is_object());
}

#[test]
fn world_cli_loads_tokenized_eu4_save_before_startup() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let save = temp.path().join("state.eu4");
    let token_map = temp.path().join("tokens.txt");
    fs::write(
        &token_map,
        "0x3000 date\n0x3001 player\n0x3002 AAA\n0x3003 countries\n0x3004 provinces\n0x3005 treasury\nlookup 7 AAA\n",
    )
    .unwrap();
    fs::write(&save, binary_startup_save()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .arg("--eu4-token-map")
        .arg(&token_map)
        .args(["--country", "AAA"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["country_controllers"]["AAA"], "human");
    assert_eq!(value["country"]["resources"]["treasury_milli"], 12_345);
}

#[test]
fn world_cli_loads_plaintext_eu4_save_state_before_startup() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let save = temp.path().join("state.eu4");
    let exported = temp.path().join("exported-state.json");
    fs::write(
        &save,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={
	AAA={
		treasury=12.345
		manpower=6.000
		mercantilism=18.500
		stability=0.000
		is_at_war=yes
		prestige=26.942
		powers={
			42 84 126
		}
		navy={
			location=2
			ship={
				type="light_ship"
			}
		}
		navy={
			location=1
			ship={
				type="light_ship"
			}
		}
		human=yes
	}
	BBB={
		treasury=4.000
		manpower=2.000
		mercantilism=10.000
		stability=0.000
		prestige=5.000
		powers={
			10 20 30
		}
	}
}
trade={
    node={ definitions=river current=1.234 local_value=5.945 outgoing=5.626 p_pow=36.912 }
	node={ definitions=harbor current=0.000 }
	node={ definitions=missing_node current=9.999 }
}
provinces={
	-1={
		owner="AAA"
		controller="AAA"
		local_autonomy=12.500
		base_tax=12.345
		base_production=7.890
		base_manpower=4.250
	}
	-2={
		local_autonomy=0.000
	}
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .args(["--country", "AAA", "--province", "1", "--economy"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["country_controllers"]["AAA"], "human");
    assert_eq!(value["country"]["resources"]["treasury_milli"], 12_345);
    assert_eq!(value["country"]["resources"]["manpower_milli"], 6_000);
    assert_eq!(value["country"]["metrics"]["prestige"], 26_942);
    assert_eq!(value["country"]["politics"]["statuses"]["at_war"], true);
    assert_eq!(value["simulation"]["naval_units"], 2);
    assert_eq!(value["simulation"]["fleets"], 2);
    assert_eq!(value["country"]["metrics"]["adm_power"], 42);
    assert_eq!(
        value["province"]["administration"]["raw_autonomy_micros"],
        125_000
    );
    assert_eq!(value["province"]["tax_milli"], 12_345);
    assert_eq!(value["province"]["production_milli"], 7_890);
    assert_eq!(value["province"]["manpower_milli"], 4_250);
    assert_eq!(value["province"]["controller"], "AAA");
    assert_eq!(value["province"]["trade_node_current_milli"], 1_234);
    let trade_nodes = value["economy"]["trade_nodes"].as_array().unwrap();
    assert_eq!(
        trade_nodes
            .iter()
            .find(|node| node["id"] == "river")
            .unwrap()["current_milli"],
        1_234
    );
    assert_eq!(
        trade_nodes
            .iter()
            .find(|node| node["id"] == "harbor")
            .unwrap()["current_milli"],
        0
    );
    assert_eq!(
        trade_nodes
            .iter()
            .find(|node| node["id"] == "river")
            .unwrap()["runtime_state"]["local_value_milli"],
        5_945
    );
    assert_eq!(
        trade_nodes
            .iter()
            .find(|node| node["id"] == "river")
            .unwrap()["runtime_state"]["outgoing_milli"],
        5_626
    );
    assert_eq!(
        trade_nodes
            .iter()
            .find(|node| node["id"] == "river")
            .unwrap()["runtime_state"]["p_pow_milli"],
        36_912
    );
    assert!(trade_nodes.iter().all(|node| node["id"] != "missing_node"));

    let export = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .arg("--export-external-state")
        .arg(&exported)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let exported_state: serde_json::Value =
        serde_json::from_slice(&fs::read(&exported).unwrap()).unwrap();
    assert_eq!(
        exported_state["country_resources"]["AAA"]["treasury_milli"],
        12_345
    );
    assert_eq!(
        exported_state["country_metric_values"]["AAA"]["prestige"],
        26_942
    );
    assert_eq!(exported_state["country_statuses"]["AAA"]["at_war"], true);
    assert_eq!(exported_state["naval_units"].as_array().unwrap().len(), 2);
    assert_eq!(exported_state["naval_units"][0]["location"], 2);
    assert_eq!(exported_state["naval_fleets"].as_array().unwrap().len(), 2);
    assert_eq!(exported_state["naval_fleets"][0]["location"], 2);
    assert_eq!(exported_state["naval_fleets"][1]["location"], 1);
    assert_eq!(
        exported_state["province_development"]["1"]["base_tax_milli"],
        12_345
    );
    assert_eq!(
        exported_state["province_development"]["1"]["base_production_milli"],
        7_890
    );
    assert_eq!(
        exported_state["province_development"]["1"]["base_manpower_milli"],
        4_250
    );
    assert_eq!(exported_state["province_controllers"]["1"], "AAA");

    let reloaded = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&exported)
        .arg("--player")
        .arg("AAA")
        .arg("--country")
        .arg("AAA")
        .arg("--province")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        reloaded.status.success(),
        "{}",
        String::from_utf8_lossy(&reloaded.stderr)
    );
    let reloaded: serde_json::Value = serde_json::from_slice(&reloaded.stdout).unwrap();
    assert_eq!(reloaded["country"]["resources"]["treasury_milli"], 12_345);
    assert_eq!(reloaded["country"]["metrics"]["prestige"], 26_942);
    assert_eq!(reloaded["country"]["politics"]["statuses"]["at_war"], true);
    assert_eq!(reloaded["simulation"]["naval_units"], 2);
    assert_eq!(reloaded["simulation"]["fleets"], 2);
    assert_eq!(reloaded["province"]["tax_milli"], 12_345);
    assert_eq!(reloaded["province"]["production_milli"], 7_890);
    assert_eq!(reloaded["province"]["manpower_milli"], 4_250);
    assert_eq!(reloaded["province"]["controller"], "AAA");

    let snapshot = temp.path().join("eu4-save-snapshot.json");
    let initial_snapshot = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .args(["--days", "30"])
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        initial_snapshot.status.success(),
        "{}",
        String::from_utf8_lossy(&initial_snapshot.stderr)
    );

    let resumed_snapshot = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .arg("--load")
        .arg(&snapshot)
        .args(["--days", "30", "--country", "AAA"])
        .output()
        .unwrap();
    assert!(
        resumed_snapshot.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed_snapshot.stderr)
    );
    let resumed_snapshot: serde_json::Value =
        serde_json::from_slice(&resumed_snapshot.stdout).unwrap();
    assert_eq!(resumed_snapshot["simulation"]["day"], 60);
    assert_eq!(
        resumed_snapshot["simulation"]["date"],
        serde_json::json!({"year": 1445, "month": 1, "day": 10})
    );
    assert_eq!(resumed_snapshot["simulation"]["tax_settlements"], 2);
    assert_eq!(resumed_snapshot["simulation"]["last_tax_day"], 51);
    assert_eq!(resumed_snapshot["country_controllers"]["AAA"], "human");
    assert_eq!(resumed_snapshot["simulation"]["naval_units"], 2);
    assert_eq!(resumed_snapshot["simulation"]["fleets"], 2);

    let overlay = temp.path().join("eu4-save-overlay.json");
    fs::write(
        &overlay,
        serde_json::json!({
            "country_monthly_cashflows_milli": {"AAA": 125},
            "province_is_city": {"2": true},
            "production_inputs": {
                "2": {
                    "input": {
                        "base_value_milli": 1000,
                        "country_modifier_milli": 0,
                        "legacy_province_modifier_milli": 0,
                        "queried_province_modifier_milli": 0
                    },
                    "unresolved": []
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    let combined = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .arg("--external-state")
        .arg(&overlay)
        .args(["--days", "30", "--country", "AAA"])
        .output()
        .unwrap();
    assert!(
        combined.status.success(),
        "{}",
        String::from_utf8_lossy(&combined.stderr)
    );
    let combined: serde_json::Value = serde_json::from_slice(&combined.stdout).unwrap();
    assert_eq!(combined["simulation"]["production_settlements"], 1);
    assert_eq!(combined["simulation"]["last_production_day"], 20);
    assert_eq!(combined["simulation"]["cashflow_settlements"], 1);
    assert_eq!(combined["simulation"]["last_cashflow_day"], 20);
    assert_eq!(combined["simulation"]["naval_units"], 2);
    assert_eq!(combined["simulation"]["fleets"], 2);

    let combined_export = temp.path().join("eu4-save-overlay-export.json");
    let export = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&save)
        .arg("--external-state")
        .arg(&overlay)
        .arg("--export-external-state")
        .arg(&combined_export)
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let exported: serde_json::Value =
        serde_json::from_slice(&fs::read(&combined_export).unwrap()).unwrap();
    assert_eq!(exported["country_monthly_cashflows_milli"]["AAA"], 125);
    assert_eq!(
        exported["production_inputs"]["2"]["input"]["base_value_milli"],
        1000
    );
    assert_eq!(exported["province_is_city"]["2"], true);
}

#[test]
fn world_cli_saves_and_resumes_across_monthly_tax_settlements() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let snapshot = temp.path().join("world.json");
    let resumed_snapshot = temp.path().join("resumed.json");
    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .args(["--days", "30", "--seed", "8351375"])
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["simulation"]["day"], 30);
    assert!(value["simulation"]["yearly_system_id"].is_null());
    assert_eq!(
        value["simulation"]["date"],
        serde_json::json!({"year": 1444, "month": 12, "day": 11})
    );
    assert_eq!(value["simulation"]["tax_settlements"], 1);
    assert_eq!(value["simulation"]["last_tax_day"], 20);
    assert_eq!(value["simulation"]["eu4_random"]["seed"], 8_351_375);
    assert_eq!(value["simulation"]["eu4_random"]["count"], 0);
    assert_eq!(value["simulation"]["eu4_random"]["index"], 0);
    assert!(snapshot.is_file());
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    assert_eq!(saved["format"], 2);
    assert_eq!(saved["eu4_random"]["seed"], 8_351_375);
    assert_eq!(saved["eu4_event_seed"], 8_351_375);

    let resumed = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"))
        .arg("--load")
        .arg(&snapshot)
        .args(["--days", "31", "--seed", "1"])
        .arg("--save")
        .arg(&resumed_snapshot)
        .output()
        .unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(value["simulation"]["day"], 61);
    assert_eq!(
        value["simulation"]["date"],
        serde_json::json!({"year": 1445, "month": 1, "day": 11})
    );
    assert_eq!(value["simulation"]["tax_settlements"], 2);
    assert_eq!(value["simulation"]["last_tax_day"], 51);
    assert_eq!(value["simulation"]["eu4_random"]["seed"], 8_351_375);
    assert_eq!(value["simulation"]["eu4_random"]["count"], 0);
    assert_eq!(value["simulation"]["eu4_random"]["index"], 0);
    let resumed: serde_json::Value =
        serde_json::from_slice(&fs::read(resumed_snapshot).unwrap()).unwrap();
    assert_eq!(resumed["eu4_event_seed"], 8_351_375);
}

#[test]
fn world_cli_seed_matches_eu4_signed_32_bit_conversion() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let cases: [(&str, u32); 2] = [("4149563735", 2_147_483_647), ("-145403561", 4_149_563_735)];
    for (seed_text, expected_seed) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(&fixture)
            .args(["--seed", seed_text, "--days", "0", "--omit-load-report"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "seed {seed_text}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["simulation"]["eu4_random"]["seed"], expected_seed,
            "seed {seed_text}"
        );
    }
}

#[test]
fn world_cli_runs_and_resumes_the_monthly_ledger_pipeline() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let external = temp.path().join("external-state.json");
    let snapshot = temp.path().join("monthly-ledger.json");
    fs::write(
        &external,
        serde_json::json!({
            "country_resources": {
                "AAA": {"treasury_milli": 10_000}
            },
            "province_annual_tax_milli": {
                "1": 12_000,
                "2": 8_000
            },
            "production_inputs": {
                "2": {
                    "input": {
                        "base_value_milli": 1_000,
                        "country_modifier_milli": 20,
                        "legacy_province_modifier_milli": 0,
                        "queried_province_modifier_milli": -10
                    },
                    "unresolved": []
                }
            },
            "country_monthly_cashflows_milli": {"AAA": 250},
            "country_monthly_cashflow_breakdowns": {
                "AAA": {
                    "trade_milli": 200,
                    "gold_milli": 100,
                    "expenses_milli": 50
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--monthly-ledger-pipeline",
            "--days",
            "30",
            "--country",
            "AAA",
        ])
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        first["simulation"]["monthly_system_id"],
        "augustus-world-monthly-ledger:v2"
    );
    assert_eq!(first["simulation"]["cashflow_settlements"], 1);
    assert_eq!(first["country"]["ledger"]["income_snapshot_milli"][2], 200);
    assert_eq!(first["country"]["ledger"]["income_snapshot_milli"][3], 100);
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    assert_eq!(
        saved["monthly_system_id"],
        "augustus-world-monthly-ledger:v2"
    );

    let resumed = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--monthly-ledger-pipeline",
            "--days",
            "31",
            "--country",
            "AAA",
        ])
        .arg("--load")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let resumed: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(resumed["simulation"]["day"], 61);
    assert_eq!(resumed["simulation"]["cashflow_settlements"], 2);
    assert_eq!(
        resumed["simulation"]["monthly_system_id"],
        "augustus-world-monthly-ledger:v2"
    );
    assert_eq!(
        resumed["country"]["ledger"]["income_snapshot_milli"][2],
        200
    );
    assert_eq!(
        resumed["country"]["ledger"]["income_snapshot_milli"][3],
        100
    );

    let legacy_snapshot = temp.path().join("monthly-ledger-v1.json");
    let mut legacy_saved = saved;
    legacy_saved["monthly_system_id"] = serde_json::json!("augustus-world-monthly-ledger:v1");
    fs::write(&legacy_snapshot, serde_json::to_vec(&legacy_saved).unwrap()).unwrap();
    let legacy_resume = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--monthly-ledger-pipeline",
            "--days",
            "1",
            "--country",
            "AAA",
        ])
        .arg("--load")
        .arg(&legacy_snapshot)
        .output()
        .unwrap();
    assert!(
        legacy_resume.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy_resume.stderr)
    );
    let legacy_resume: serde_json::Value = serde_json::from_slice(&legacy_resume.stdout).unwrap();
    assert_eq!(legacy_resume["simulation"]["day"], 31);
    assert_eq!(
        legacy_resume["simulation"]["monthly_system_id"],
        "augustus-world-monthly-ledger:v1"
    );
}

#[test]
fn world_cli_runs_and_resumes_an_explicit_annual_ledger_rollover() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let external = temp.path().join("external-state.json");
    let snapshot = temp.path().join("before-annual-rollover.json");
    fs::write(
        &external,
        serde_json::json!({
            "country_resources": {
                "AAA": {"treasury_milli": 10_000}
            },
            "province_annual_tax_milli": {
                "1": 12_000,
                "2": 8_000
            },
            "production_inputs": {
                "2": {
                    "input": {
                        "base_value_milli": 1_000,
                        "country_modifier_milli": 20,
                        "legacy_province_modifier_milli": 0,
                        "queried_province_modifier_milli": -10
                    },
                    "unresolved": []
                }
            },
            "country_monthly_cashflows_milli": {"AAA": 250},
            "country_monthly_cashflow_breakdowns": {
                "AAA": {
                    "trade_milli": 200,
                    "gold_milli": 100,
                    "expenses_milli": 50
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    let before_rollover = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--annual-ledger-rollover",
            "--days",
            "415",
            "--seed",
            "8351375",
            "--country",
            "AAA",
        ])
        .arg("--save")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        before_rollover.status.success(),
        "{}",
        String::from_utf8_lossy(&before_rollover.stderr)
    );
    let before_rollover: serde_json::Value =
        serde_json::from_slice(&before_rollover.stdout).unwrap();
    assert_eq!(before_rollover["simulation"]["day"], 415);
    assert_eq!(
        before_rollover["simulation"]["date"],
        serde_json::json!({"year": 1445, "month": 12, "day": 31})
    );
    assert_eq!(
        before_rollover["simulation"]["yearly_system_id"],
        "country-ledger-annual-rollover:1"
    );
    assert!(
        before_rollover["country"]["ledger"]["income_rollover_total_milli"]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(
        before_rollover["country"]["ledger"]["income_accumulator_milli"][2]
            .as_i64()
            .unwrap()
            > 0
    );
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    assert_eq!(
        saved["yearly_system_id"],
        "country-ledger-annual-rollover:1"
    );

    let after_rollover = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--external-state")
        .arg(&external)
        .args([
            "--annual-ledger-rollover",
            "--days",
            "1",
            "--seed",
            "1",
            "--country",
            "AAA",
        ])
        .arg("--load")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        after_rollover.status.success(),
        "{}",
        String::from_utf8_lossy(&after_rollover.stderr)
    );
    let after_rollover: serde_json::Value = serde_json::from_slice(&after_rollover.stdout).unwrap();
    assert_eq!(after_rollover["simulation"]["day"], 416);
    assert_eq!(
        after_rollover["simulation"]["date"],
        serde_json::json!({"year": 1446, "month": 1, "day": 1})
    );
    assert_eq!(
        after_rollover["simulation"]["yearly_system_id"],
        "country-ledger-annual-rollover:1"
    );
    assert!(
        after_rollover["country"]["ledger"]["income_rollover_total_milli"]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(
        after_rollover["country"]["ledger"]["income_rollover_milli"][2]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(
        after_rollover["country"]["ledger"]["income_rollover_milli"][3]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(
        after_rollover["country"]["ledger"]["income_accumulator_milli"]
            .as_array()
            .unwrap()
            .iter()
            .all(|value| value == 0)
    );
}

#[test]
fn world_cli_loads_a_directory_mod_through_eu4_source_resolution() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let mod_root = temp.path().join("test-mod");
    fs::create_dir_all(mod_root.join("common/ideas")).unwrap();
    fs::write(mod_root.join("descriptor.mod"), "name=\"CLI Test Mod\"").unwrap();
    fs::write(
        mod_root.join("common/ideas/99_mod.txt"),
        "mod_group = { free = yes first_idea = {} }",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .arg("--mod")
        .arg(&mod_root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["report"]["ideas"]["definition_sources"]["mod_group"]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("99_mod.txt"))
    );
}

#[test]
fn world_cli_applies_launcher_mod_selection() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("selected");
    let unselected = temp.path().join("unselected");
    fs::create_dir_all(selected.join("common/ideas")).unwrap();
    fs::create_dir_all(unselected.join("common/ideas")).unwrap();
    fs::write(selected.join("descriptor.mod"), "name=\"Selected Mod\"").unwrap();
    fs::write(unselected.join("descriptor.mod"), "name=\"Unselected Mod\"").unwrap();
    fs::write(
        selected.join("common/ideas/99_selected.txt"),
        "mod_group = { free = yes first_idea = {} }",
    )
    .unwrap();
    fs::write(
        unselected.join("common/ideas/99_unselected.txt"),
        "other_group = { free = yes first_idea = {} }",
    )
    .unwrap();
    let launcher_config = temp.path().join("dlc_load.json");
    fs::write(&launcher_config, r#"{"enabled_mods":["Selected Mod"]}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(fixture)
        .arg("--mod")
        .arg(&unselected)
        .arg("--mod")
        .arg(&selected)
        .arg("--launcher-config")
        .arg(&launcher_config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["report"]["ideas"]["definition_sources"]
            .get("mod_group")
            .is_some()
    );
    assert!(
        value["report"]["ideas"]["definition_sources"]
            .get("other_group")
            .is_none()
    );
}

#[test]
fn migration_queries_read_post_advance_and_restored_power_without_changing_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let defines = fs::read_to_string(&defines_path).unwrap();
    fs::write(
        &defines_path,
        format!(
            "{defines}
NDefines.NCountry.DEBT_LIMIT_DIP = -100
"
        ),
    )
    .unwrap();
    fs::create_dir_all(fixture.join("common/on_actions")).unwrap();
    fs::create_dir_all(fixture.join("events")).unwrap();
    fs::write(
        fixture.join("common/on_actions/migration_query.txt"),
        "on_startup={if={limit={tag=AAA} country_event={id=migration_query.1 days=1}}}",
    )
    .unwrap();
    fs::write(fixture.join("events/migration_query.txt"),
        "country_event={id=migration_query.1 hidden=yes is_triggered_only=yes immediate={add_mil_power=-1}}").unwrap();
    let state = temp.path().join("state.json");
    fs::write(
        &state,
        r#"{"country_metric_values":{"AAA":{"adm_power":140,"dip_power":126,"mil_power":150}}}"#,
    )
    .unwrap();
    let queries = temp.path().join("queries.json");
    let request = serde_json::json!([{
        "country":"AAA", "destination":1,
        "pricing": { "define_adm_power_milli":0, "define_dip_power_milli":0,
            "define_mil_power_milli":50000, "migration_steps":2,
            "destination_in_native_membership":false, "all_power_cost_modifier_milli":0,
            "country_migration_cost_modifier_milli":0, "province_migration_cost_modifier_milli":0 },
        "native_membership_known":true, "native_mode_eligible":true,
        "owner_eligible":true, "controller_eligible":true,
        "tribal_owner_eligible":true, "additional_gate_eligible":true
    }]);
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let run = |days: &str, load: Option<&Path>, save: Option<&Path>, query: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        cmd.arg("--game-root")
            .arg(&fixture)
            .arg("--external-state")
            .arg(&state)
            .args([
                "--seed",
                "7",
                "--ai-only",
                "--days",
                days,
                "--omit-load-report",
            ]);
        if let Some(path) = load {
            cmd.arg("--load").arg(path);
        }
        if let Some(path) = save {
            cmd.arg("--save").arg(path);
        }
        if query {
            cmd.arg("--native-migration-queries").arg(&queries);
        }
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let start = temp.path().join("start.json");
    let passed = run("0", None, Some(&start), true);
    assert_eq!(
        passed["native_migration_queries"][0]["evaluation"]["resolved_gate"],
        true
    );
    assert_eq!(
        passed["native_migration_queries"][0]["available_power"]["mil_power"],
        150
    );
    let continuous = temp.path().join("continuous.json");
    let failed = run("1", None, Some(&continuous), true);
    assert_eq!(
        failed["native_migration_queries"][0]["available_power"]["mil_power"],
        149
    );
    assert_eq!(
        failed["native_migration_queries"][0]["evaluation"]["resolved_gate"],
        false
    );
    assert_eq!(
        failed["native_migration_queries"][0]["evaluation"]["cost"]["mil_cost"],
        150
    );
    let resumed = temp.path().join("resumed.json");
    let restored = run("1", Some(&start), Some(&resumed), true);
    assert_eq!(
        restored["native_migration_queries"],
        failed["native_migration_queries"]
    );
    let plain = temp.path().join("plain.json");
    let without = run("1", None, Some(&plain), false);
    assert!(without.get("native_migration_queries").is_none());
    assert_eq!(fs::read(&continuous).unwrap(), fs::read(&resumed).unwrap());
    assert_eq!(fs::read(&continuous).unwrap(), fs::read(&plain).unwrap());
}

#[test]
fn migration_query_cli_preserves_unknown_power_and_rejects_stale_resource_fields() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    let queries = temp.path().join("query.json");
    let mut request = serde_json::json!([{
        "country":"AAA", "destination":1,
        "pricing": { "define_adm_power_milli":0, "define_dip_power_milli":0,
            "define_mil_power_milli":50000, "migration_steps":0,
            "destination_in_native_membership":true, "all_power_cost_modifier_milli":0,
            "country_migration_cost_modifier_milli":0, "province_migration_cost_modifier_milli":0 },
        "native_membership_known":true, "native_mode_eligible":true,
        "owner_eligible":true, "controller_eligible":true,
        "tribal_owner_eligible":true, "additional_gate_eligible":true
    }]);
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(&fixture)
            .arg("--native-migration-queries")
            .arg(&queries)
            .args(["--seed", "7", "--ai-only", "--omit-load-report"])
            .output()
            .unwrap()
    };
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let unknown = run();
    assert!(
        unknown.status.success(),
        "{}",
        String::from_utf8_lossy(&unknown.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&unknown.stdout).unwrap();
    let report = &value["native_migration_queries"][0];
    assert_eq!(report["evaluation"], serde_json::Value::Null);
    assert_eq!(
        report["available_power"]["mil_power"],
        serde_json::Value::Null
    );
    assert!(
        report["unresolved"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("mil_power_unknown"))
    );
    request[0]["pricing"]["available_mil_power"] = serde_json::json!(150);
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let invalid = run();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("available_mil_power"));
}

#[test]
fn migration_content_queries_bind_effective_defines_and_keep_missing_values_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let original = fs::read_to_string(&defines_path).unwrap();
    fs::write(
        &defines_path,
        format!("{original}\nNDefines.NCountry.DEBT_LIMIT_DIP=-100\n"),
    )
    .unwrap();
    let state = temp.path().join("state.json");
    fs::write(
        &state,
        r#"{"country_metric_values":{"AAA":{"adm_power":140,"dip_power":126,"mil_power":50}}}"#,
    )
    .unwrap();
    let queries = temp.path().join("queries.json");
    let mut request = serde_json::json!([{
        "country":"AAA", "destination":1,
        "pricing": { "migration_steps":0, "destination_in_native_membership":true,
            "all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,
            "province_migration_cost_modifier_milli":0 },
        "native_membership_known":true,"native_mode_eligible":true,"owner_eligible":true,
        "controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
    }]);
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let run = |load: Option<&Path>, save: Option<&Path>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        cmd.arg("--game-root")
            .arg(&fixture)
            .arg("--external-state")
            .arg(&state)
            .arg("--native-migration-content-queries")
            .arg(&queries)
            .args(["--seed", "7", "--ai-only"]);
        if let Some(path) = load {
            cmd.arg("--load").arg(path);
        }
        if let Some(path) = save {
            cmd.arg("--save").arg(path);
        }
        cmd.output().unwrap()
    };
    let missing = run(None, None);
    assert!(
        missing.status.success(),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    let missing: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    let report = &missing["native_migration_content_queries"][0];
    assert_eq!(report["evaluation"], serde_json::Value::Null);
    assert_eq!(report["available_power"]["mil_power"], 50);
    assert!(
        report["unresolved"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(
                "native_migration_cost:missing_define:TRIBE_MIGRATION_MIL_COST"
            ))
    );
    let base_defines = fs::read_to_string(&defines_path).unwrap();
    let with_costs = format!(
        "{base_defines}\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n"
    );
    fs::write(&defines_path, &with_costs).unwrap();
    let snapshot = temp.path().join("snapshot.json");
    let passed = run(None, Some(&snapshot));
    assert!(
        passed.status.success(),
        "{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    let passed: serde_json::Value = serde_json::from_slice(&passed.stdout).unwrap();
    assert_eq!(
        passed["native_migration_content_queries"][0]["evaluation"]["cost"]["mil_cost"],
        50
    );
    assert_eq!(
        passed["native_migration_content_queries"][0]["evaluation"]["resolved_gate"],
        true
    );
    assert_eq!(
        passed["report"]["setup"]["native_migration_cost_defines_milli"]["TRIBE_MIGRATION_MIL_COST"],
        50000
    );
    // The query uses currently loaded content, even when restoring an otherwise matching world.
    fs::write(
        &defines_path,
        with_costs.replace("MIGRATION_MIL_COST=50", "MIGRATION_MIL_COST=60"),
    )
    .unwrap();
    let changed = run(Some(&snapshot), None);
    assert!(
        changed.status.success(),
        "{}",
        String::from_utf8_lossy(&changed.stderr)
    );
    let changed: serde_json::Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(
        changed["native_migration_content_queries"][0]["evaluation"]["cost"]["mil_cost"],
        60
    );
    assert_eq!(
        changed["native_migration_content_queries"][0]["evaluation"]["resolved_gate"],
        false
    );
    fs::write(
        &defines_path,
        with_costs.replace("MIGRATION_MIL_COST=50", "MIGRATION_MIL_COST=-1"),
    )
    .unwrap();
    let invalid_define = run(None, None);
    assert!(!invalid_define.status.success());
    assert!(String::from_utf8_lossy(&invalid_define.stderr).contains("TRIBE_MIGRATION_MIL_COST"));
    let plain = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--seed", "7", "--ai-only"])
        .output()
        .unwrap();
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let plain: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert!(
        plain["report"]["setup"]["native_migration_cost_define_errors"]["TRIBE_MIGRATION_MIL_COST"]
            .is_string()
    );
    request[0]["pricing"]["define_mil_power_milli"] = serde_json::json!(1);
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let invalid = run(None, None);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("define_mil_power_milli"));
}

#[test]
fn migration_state_import_drives_membership_costs_and_survives_snapshot_restore() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let defines = fs::read_to_string(&defines_path).unwrap();
    fs::write(&defines_path, format!("{defines}\nNDefines.NCountry.DEBT_LIMIT_DIP=-100\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n")).unwrap();
    let source_save = temp.path().join("native.eu4");
    let make_save = |members: &str| {
        format!(
            "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={{AAA={{capital=2 powers={{140 126 150}} native={{provinces={{{members}}} migration_steps=2}}}}}}\nprovinces={{-1={{owner=BBB controller=BBB tribal_owner=AAA}} -2={{owner=AAA controller=AAA}}}}\n"
        )
    };
    fs::write(&source_save, make_save("2")).unwrap();
    let queries = temp.path().join("query.json");
    let request = serde_json::json!([{
        "country":"AAA","destination":1,
        "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
        "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
    },{
        "country":"AAA","destination":2,
        "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
        "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
    }]);
    fs::write(&queries, serde_json::to_vec(&request).unwrap()).unwrap();
    let run = |import: bool, load: Option<&Path>, save: Option<&Path>, query: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        cmd.arg("--game-root")
            .arg(&fixture)
            .arg("--eu4-save")
            .arg(&source_save)
            .arg("--omit-load-report");
        if import {
            cmd.arg("--import-native-migration-state");
        }
        if let Some(path) = load {
            cmd.arg("--load").arg(path);
        }
        if let Some(path) = save {
            cmd.arg("--save").arg(path);
        }
        if query {
            cmd.arg("--native-migration-state-queries").arg(&queries);
        }
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let unknown = run(false, None, None, true);
    assert_eq!(
        unknown["native_migration_state_queries"][0]["query"]["evaluation"],
        serde_json::Value::Null
    );
    let snapshot = temp.path().join("state.json");
    let imported = run(true, None, Some(&snapshot), true);
    let report = &imported["native_migration_state_queries"];
    assert_eq!(report[0]["context"]["provinces"], serde_json::json!([2]));
    assert_eq!(report[0]["context"]["migration_steps"], 2);
    assert_eq!(
        report[0]["context"]["destination_in_native_membership"],
        false
    );
    assert_eq!(report[0]["query"]["evaluation"]["cost"]["mil_cost"], 150);
    assert_eq!(
        report[1]["context"]["destination_in_native_membership"],
        true
    );
    assert_eq!(report[1]["query"]["evaluation"]["cost"]["mil_cost"], 50);
    // Snapshots retain the imported component even without re-importing it.
    let restored = run(false, Some(&snapshot), None, true);
    assert_eq!(restored["native_migration_state_queries"], *report);
    let plain = temp.path().join("plain.json");
    run(true, None, Some(&plain), false);
    assert_eq!(fs::read(&plain).unwrap(), fs::read(&snapshot).unwrap());

    let explicit_state = temp.path().join("explicit-state.json");
    fs::write(
        &explicit_state,
        r#"{"AAA":{"provinces":[],"migration_steps":3}}"#,
    )
    .unwrap();
    let explicit_command = || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_augustus-world"));
        cmd.arg("--game-root")
            .arg(&fixture)
            .arg("--eu4-save")
            .arg(&source_save)
            .arg("--native-migration-state")
            .arg(&explicit_state)
            .arg("--native-migration-state-queries")
            .arg(&queries)
            .arg("--omit-load-report");
        cmd
    };
    let explicit = explicit_command().output().unwrap();
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let explicit: serde_json::Value = serde_json::from_slice(&explicit.stdout).unwrap();
    assert_eq!(
        explicit["native_migration_state_queries"][0]["query"]["evaluation"]["cost"]["mil_cost"],
        200
    );
    let restored = explicit_command()
        .arg("--load")
        .arg(&snapshot)
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["native_migration_state_queries"], *report);
    let conflict = explicit_command()
        .arg("--import-native-migration-state")
        .output()
        .unwrap();
    assert!(!conflict.status.success());

    fs::write(&source_save, make_save("999")).unwrap();
    let filtered = run(true, None, None, true);
    assert_eq!(
        filtered["native_migration_state_queries"][0]["context"]["provinces"],
        serde_json::Value::Null
    );
    assert_eq!(
        filtered["native_migration_state_queries"][0]["context"]["migration_steps"],
        2
    );
    assert_eq!(
        filtered["native_migration_state_queries"][0]["query"]["evaluation"],
        serde_json::Value::Null
    );
    fs::write(&source_save, make_save("1 1")).unwrap();
    // The existing save reader continues to ignore this separate component.
    run(false, None, None, false);
    let strict = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .output()
        .unwrap();
    assert!(!strict.status.success());
    assert!(String::from_utf8_lossy(&strict.stderr).contains("repeats native province"));

    // Transfer execution runs after snapshot restore and before the optional
    // day advance, then persists its report and resulting snapshot.
    fs::write(
        &source_save,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={capital=2 powers={140 126 150} native={provinces={2} migration_steps=2}}}\nprovinces={-1={tribal_owner=AAA} -2={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    let transfers = temp.path().join("transfers.json");
    fs::write(
        &transfers,
        serde_json::to_vec(&serde_json::json!([{
            "country":"AAA","destination":1,
            "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
            "expected_current_capital":2,"expected_migration_steps":2,"next_migration_steps":3,
            "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
        }]))
        .unwrap(),
    )
    .unwrap();
    let transfer_snapshot = temp.path().join("transfer.json");
    let transfer = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .arg("--native-migration-transfers")
        .arg(&transfers)
        .arg("--save")
        .arg(&transfer_snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        transfer.status.success(),
        "{}",
        String::from_utf8_lossy(&transfer.stderr)
    );
    let transfer_output: serde_json::Value = serde_json::from_slice(&transfer.stdout).unwrap();
    assert_eq!(
        transfer_output["native_migration_transfers"][0]["status"],
        "applied"
    );
    assert_eq!(
        transfer_output["native_migration_transfers"][0]["transfer"]["migration_steps_after"],
        3
    );
    let restored = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&transfer_snapshot)
        .arg("--country")
        .arg("AAA")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["country"]["bound_capital"], 1);
}

#[test]
fn native_migration_transfer_cli_orders_reports_and_preserves_applied_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let defines = fs::read_to_string(&defines_path).unwrap();
    fs::write(
        &defines_path,
        format!(
            "{defines}\nNDefines.NCountry.DEBT_LIMIT_DIP=-100\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n"
        ),
    )
    .unwrap();
    let source_save = temp.path().join("native.eu4");
    fs::write(
        &source_save,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={capital=2 powers={140 126 350} native={provinces={2} migration_steps=2}}}\nprovinces={-1={tribal_owner=AAA} -2={owner=AAA controller=AAA}}\n",
    )
    .unwrap();

    let applied_request = serde_json::json!({
        "country":"AAA","destination":1,
        "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
        "expected_current_capital":2,"expected_migration_steps":2,"next_migration_steps":3,
        "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
    });
    let applied_requests = temp.path().join("applied.json");
    fs::write(
        &applied_requests,
        serde_json::to_vec(&serde_json::json!([applied_request.clone()])).unwrap(),
    )
    .unwrap();
    let batch_requests = temp.path().join("batch.json");
    let mut rejected = applied_request.clone();
    rejected["expected_current_capital"] = serde_json::json!(2);
    let mut unresolved = applied_request.clone();
    unresolved["owner_eligible"] = serde_json::Value::Null;
    fs::write(
        &batch_requests,
        serde_json::to_vec(&serde_json::json!([applied_request, rejected, unresolved])).unwrap(),
    )
    .unwrap();

    let run = |requests: &Path, snapshot: &Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
            .arg("--game-root")
            .arg(&fixture)
            .arg("--eu4-save")
            .arg(&source_save)
            .arg("--import-native-migration-state")
            .arg("--native-migration-transfers")
            .arg(requests)
            .arg("--save")
            .arg(snapshot)
            .arg("--omit-load-report")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let applied_snapshot = temp.path().join("applied-snapshot.json");
    let applied = run(&applied_requests, &applied_snapshot);
    assert_eq!(
        applied["native_migration_transfers"][0]["status"],
        "applied"
    );
    assert_eq!(
        applied["native_migration_transfers"][0]["transfer"]["migration_steps_after"],
        3
    );

    let batch_snapshot = temp.path().join("batch-snapshot.json");
    let batch = run(&batch_requests, &batch_snapshot);
    assert_eq!(
        batch["native_migration_transfers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|report| report["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["applied", "rejected", "unresolved"]
    );
    assert!(batch["native_migration_transfers"][1]["transfer"].is_null());
    assert!(batch["native_migration_transfers"][2]["transfer"].is_null());
    assert_eq!(
        fs::read(&batch_snapshot).unwrap(),
        fs::read(&applied_snapshot).unwrap()
    );

    let restored = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&batch_snapshot)
        .arg("--country")
        .arg("AAA")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["country"]["bound_capital"], 1);
}

#[test]
fn native_migration_transfer_cli_restores_snapshot_before_execution() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let defines = fs::read_to_string(&defines_path).unwrap();
    fs::write(
        &defines_path,
        format!(
            "{defines}\nNDefines.NCountry.DEBT_LIMIT_DIP=-100\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n"
        ),
    )
    .unwrap();
    let source_save = temp.path().join("native.eu4");
    fs::write(
        &source_save,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={capital=2 powers={140 126 350}}}\nprovinces={-1={tribal_owner=AAA} -2={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    let native_state = temp.path().join("native-state.json");
    fs::write(
        &native_state,
        r#"{"AAA":{"provinces":[2],"migration_steps":2}}"#,
    )
    .unwrap();
    let transfer = temp.path().join("transfer.json");
    fs::write(
        &transfer,
        serde_json::to_vec(&serde_json::json!([{
            "country":"AAA","destination":1,
            "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
            "expected_current_capital":2,"expected_migration_steps":2,"next_migration_steps":3,
            "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
        }]))
        .unwrap(),
    )
    .unwrap();

    let initial_snapshot = temp.path().join("initial.json");
    let initial = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--native-migration-state")
        .arg(&native_state)
        .arg("--save")
        .arg(&initial_snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );

    // The selected EU4 save has no native block and this invocation omits the
    // explicit state file. A transfer can succeed only if restore runs before
    // the request and restores the native state component.
    let restored_snapshot = temp.path().join("restored-transfer.json");
    let restored = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&initial_snapshot)
        .arg("--native-migration-transfers")
        .arg(&transfer)
        .arg("--save")
        .arg(&restored_snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored_output: serde_json::Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(
        restored_output["native_migration_transfers"][0]["status"],
        "applied"
    );
    assert_eq!(
        restored_output["native_migration_transfers"][0]["transfer"]["migration_steps_after"],
        3
    );

    let reloaded = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&restored_snapshot)
        .arg("--country")
        .arg("AAA")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        reloaded.status.success(),
        "{}",
        String::from_utf8_lossy(&reloaded.stderr)
    );
    let reloaded: serde_json::Value = serde_json::from_slice(&reloaded.stdout).unwrap();
    assert_eq!(reloaded["country"]["bound_capital"], 1);
}

#[test]
fn world_cli_roundtrips_combined_post_migration_relations() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let source_save = temp.path().join("post-migration.eu4");
    fs::write(
        &source_save,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={
        capital=4
        original_capital=2
        powers={140 126 150}
        native={provinces={2} migration_steps=1}
        history={1444.11.11={capital=4}}
    }
}
provinces={
    -1={previous_controller="AAA" tribal_owner="AAA"}
    -2={owner="BBB" controller="BBB" tribal_owner="AAA"}
    -4={owner="AAA" controller="AAA" previous_controller="BBB" tribal_owner="AAA"}
}
"#,
    )
    .unwrap();
    let snapshot = temp.path().join("post-migration.json");
    let first = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .arg("--country")
        .arg("AAA")
        .arg("--province")
        .arg("1")
        .arg("--save")
        .arg(&snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["country"]["bound_capital"], 4);
    assert_eq!(first["country"]["original_capital"], 2);
    assert_eq!(first["country"]["capital_history"]["1444.11.11"], 4);
    assert_eq!(first["province"]["previous_controller"], "AAA");
    assert_eq!(first["province"]["tribal_owner"], "AAA");

    let second = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&snapshot)
        .arg("--country")
        .arg("AAA")
        .arg("--province")
        .arg("1")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["country"]["bound_capital"], 4);
    assert_eq!(second["country"]["original_capital"], 2);
    assert_eq!(second["country"]["capital_history"]["1444.11.11"], 4);
    assert_eq!(second["province"]["previous_controller"], "AAA");
    assert_eq!(second["province"]["tribal_owner"], "AAA");
}

#[test]
fn world_cli_reports_caller_resolved_native_migration_scores_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let scores = temp.path().join("scores.json");
    let score_input = |candidate_id: u32, member: bool, steps: serde_json::Value| {
        serde_json::json!({
            "candidate_id": candidate_id,
            "score": {
                "eligible": true,
                "trade_factor": 1519919863,
                "a0d180_result": 6000,
                "devastation": 0,
                "destination_in_native_membership": member,
                "migration_steps": steps,
                "normalized_distance_penalties": [1264, 1910]
            }
        })
    };
    fs::write(
        &scores,
        serde_json::to_vec(&serde_json::json!([
            score_input(10, false, serde_json::json!(2)),
            score_input(20, true, serde_json::json!(2)),
            score_input(30, false, serde_json::Value::Null)
        ]))
        .unwrap(),
    )
    .unwrap();

    let score_snapshot = temp.path().join("score.json");
    let scored = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--seed", "7", "--ai-only", "--omit-load-report"])
        .arg("--native-migration-score-queries")
        .arg(&scores)
        .arg("--save")
        .arg(&score_snapshot)
        .output()
        .unwrap();
    assert!(
        scored.status.success(),
        "{}",
        String::from_utf8_lossy(&scored.stderr)
    );
    let scored: serde_json::Value = serde_json::from_slice(&scored.stdout).unwrap();
    assert_eq!(
        scored["native_migration_score_queries"]["candidates"][0]["score"]["score"],
        201736
    );
    assert_eq!(
        scored["native_migration_score_queries"]["candidates"][2]["score"],
        serde_json::Value::Null
    );
    assert_eq!(
        scored["native_migration_score_queries"]["winner"]["candidate_id"],
        20
    );

    let plain_snapshot = temp.path().join("plain.json");
    let plain = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--seed", "7", "--ai-only", "--omit-load-report"])
        .arg("--save")
        .arg(&plain_snapshot)
        .output()
        .unwrap();
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert_eq!(
        fs::read(&score_snapshot).unwrap(),
        fs::read(&plain_snapshot).unwrap()
    );
}

#[test]
fn world_cli_rejects_invalid_score_queries_before_creating_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );

    let unknown = temp.path().join("unknown.json");
    fs::write(
        &unknown,
        serde_json::to_vec(&serde_json::json!([{
            "candidate_id": 1,
            "score": {
                "eligible": true,
                "trade_factor": 1519919863,
                "a0d180_result": 6000,
                "devastation": 0,
                "destination_in_native_membership": true,
                "migration_steps": null,
                "normalized_distance_penalties": [],
                "unexpected": true
            }
        }]))
        .unwrap(),
    )
    .unwrap();
    let unknown_snapshot = temp.path().join("unknown-snapshot.json");
    let unknown_run = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--seed", "7", "--ai-only", "--omit-load-report"])
        .arg("--native-migration-score-queries")
        .arg(&unknown)
        .arg("--save")
        .arg(&unknown_snapshot)
        .output()
        .unwrap();
    assert!(!unknown_run.status.success());
    assert!(String::from_utf8_lossy(&unknown_run.stderr).contains("unknown field"));
    assert!(!unknown_snapshot.exists());

    let negative = temp.path().join("negative.json");
    fs::write(
        &negative,
        serde_json::to_vec(&serde_json::json!([{
            "candidate_id": 1,
            "score": {
                "eligible": true,
                "trade_factor": 1519919863,
                "a0d180_result": 6000,
                "devastation": 0,
                "destination_in_native_membership": false,
                "migration_steps": 2,
                "normalized_distance_penalties": [-1]
            }
        }]))
        .unwrap(),
    )
    .unwrap();
    let negative_snapshot = temp.path().join("negative-snapshot.json");
    let negative_run = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .args(["--seed", "7", "--ai-only", "--omit-load-report"])
        .arg("--native-migration-score-queries")
        .arg(&negative)
        .arg("--save")
        .arg(&negative_snapshot)
        .output()
        .unwrap();
    assert!(!negative_run.status.success());
    assert!(
        String::from_utf8_lossy(&negative_run.stderr)
            .contains("distance penalties must be nonnegative")
    );
    assert!(!negative_snapshot.exists());
}

#[test]
fn world_cli_rejects_invalid_transfer_requests_before_creating_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let defines_path = fixture.join("common/defines.lua");
    let defines = fs::read_to_string(&defines_path).unwrap();
    fs::write(
        &defines_path,
        format!(
            "{defines}\nNDefines.NCountry.DEBT_LIMIT_DIP=-100\nNDefines.NCountry.TRIBE_MIGRATION_ADM_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_DIP_COST=0\nNDefines.NCountry.TRIBE_MIGRATION_MIL_COST=50\n"
        ),
    )
    .unwrap();
    let source_save = temp.path().join("native.eu4");
    fs::write(
        &source_save,
        "EU4txt\ndate=1444.11.11\nplayer=AAA\ncountries={AAA={capital=2 powers={140 126 350} native={provinces={2} migration_steps=2}}}\nprovinces={-1={tribal_owner=AAA} -2={owner=AAA controller=AAA}}\n",
    )
    .unwrap();
    let request = || {
        serde_json::json!({
            "country":"AAA","destination":1,
            "pricing":{"all_power_cost_modifier_milli":0,"country_migration_cost_modifier_milli":0,"province_migration_cost_modifier_milli":0},
            "expected_current_capital":2,"expected_migration_steps":2,"next_migration_steps":3,
            "native_mode_eligible":true,"owner_eligible":true,"controller_eligible":true,"tribal_owner_eligible":true,"additional_gate_eligible":true
        })
    };

    let unknown = temp.path().join("unknown-transfer.json");
    let mut unknown_request = request();
    unknown_request["unexpected"] = serde_json::json!(true);
    fs::write(
        &unknown,
        serde_json::to_vec(&serde_json::json!([unknown_request])).unwrap(),
    )
    .unwrap();
    let unknown_snapshot = temp.path().join("unknown-transfer-snapshot.json");
    let unknown_run = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .arg("--native-migration-transfers")
        .arg(&unknown)
        .arg("--save")
        .arg(&unknown_snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(!unknown_run.status.success());
    assert!(String::from_utf8_lossy(&unknown_run.stderr).contains("unknown field"));
    assert!(!unknown_snapshot.exists());

    let unknown_country = temp.path().join("unknown-country-transfer.json");
    let mut unknown_country_request = request();
    unknown_country_request["country"] = serde_json::json!("ZZZ");
    fs::write(
        &unknown_country,
        serde_json::to_vec(&serde_json::json!([unknown_country_request])).unwrap(),
    )
    .unwrap();
    let unknown_country_snapshot = temp.path().join("unknown-country-snapshot.json");
    let unknown_country_run = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .arg("--native-migration-transfers")
        .arg(&unknown_country)
        .arg("--save")
        .arg(&unknown_country_snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(!unknown_country_run.status.success());
    assert!(
        String::from_utf8_lossy(&unknown_country_run.stderr).contains("unknown EU4 country ZZZ")
    );
    assert!(!unknown_country_snapshot.exists());
}

#[test]
fn world_cli_roundtrips_explicit_missing_capital_history_value() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("world");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
        &fixture,
    );
    let source_save = temp.path().join("missing-capital.eu4");
    fs::write(
        &source_save,
        r#"EU4txt
date=1444.11.11
player="AAA"
countries={
    AAA={
        capital=4
        original_capital=2
        powers={140 126 150}
        native={provinces={2} migration_steps=1}
        history={1444.11.11={capital=0}}
    }
}
provinces={
    -1={previous_controller="AAA" tribal_owner="AAA"}
    -2={owner="BBB" controller="BBB" tribal_owner="AAA"}
    -4={owner="AAA" controller="AAA" previous_controller="BBB" tribal_owner="AAA"}
}
"#,
    )
    .unwrap();
    let snapshot = temp.path().join("missing-capital.json");
    let first = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--import-native-migration-state")
        .arg("--country")
        .arg("AAA")
        .arg("--save")
        .arg(&snapshot)
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["country"]["original_capital"], 2);
    assert_eq!(
        first["country"]["capital_history"]["1444.11.11"],
        serde_json::Value::Null
    );

    let second = Command::new(env!("CARGO_BIN_EXE_augustus-world"))
        .arg("--game-root")
        .arg(&fixture)
        .arg("--eu4-save")
        .arg(&source_save)
        .arg("--load")
        .arg(&snapshot)
        .arg("--country")
        .arg("AAA")
        .arg("--omit-load-report")
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["country"]["original_capital"], 2);
    assert_eq!(
        second["country"]["capital_history"]["1444.11.11"],
        serde_json::Value::Null
    );
}

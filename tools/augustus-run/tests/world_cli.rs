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

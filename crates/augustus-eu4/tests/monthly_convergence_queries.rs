// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaMonthlyConvergenceQuery, VanillaWorld};
use std::{fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        let target = temp.path().join(entry.path().strip_prefix(&root).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).unwrap();
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
    let path = temp.path().join("common/defines.lua");
    let old = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        old.replace(
            "NCountry = {",
            "NCountry = {\n MIN_LIBERTY_DESIRE = 0,\n MAX_LIBERTY_DESIRE = 100,\n MONTHLY_LIBERTY_DECREASE = 0.1,",
        ),
    )
    .unwrap();
    temp
}

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn monthly_convergence_query_reports_active_and_inactive_accounts_without_mutation() {
    let temp = fixture();
    write(
        temp.path(),
        "common/on_actions/control.txt",
        "on_startup={if={limit={tag=BBB} add_liberty_desire=25} if={limit={tag=AAA} add_liberty_desire=50}} ",
    );
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    let snapshot = loaded.save().unwrap();

    let active = loaded
        .query_vanilla_monthly_convergence(&VanillaMonthlyConvergenceQuery {
            country: "BBB".into(),
            metric: Some("temporary_liberty_desire".into()),
        })
        .unwrap();
    let active_account = &active.accounts["temporary_liberty_desire"];
    assert_eq!(active.day, 0);
    assert_eq!(
        (
            active_account.applies,
            active_account.quantity_before,
            active_account.quantity_after,
            active_account.changed,
        ),
        (Some(true), Some(25_000), Some(24_900), Some(-100))
    );
    assert!(active_account.unresolved.is_empty());

    let inactive = loaded
        .query_vanilla_monthly_convergence(&VanillaMonthlyConvergenceQuery {
            country: "AAA".into(),
            metric: None,
        })
        .unwrap();
    let inactive_account = &inactive.accounts["temporary_liberty_desire"];
    assert_eq!(inactive_account.applies, Some(false));
    assert_eq!(
        (
            inactive_account.quantity_before,
            inactive_account.quantity_after,
            inactive_account.changed,
        ),
        (Some(50_000), Some(0), Some(-50_000))
    );
    assert!(inactive_account.unresolved.is_empty());
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_convergence_query_rejects_unknown_country_metric_and_fields() {
    let temp = fixture();
    let loaded = VanillaWorld::load(temp.path(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_convergence(&VanillaMonthlyConvergenceQuery {
                country: "ZZZ".into(),
                metric: None,
            })
            .is_err()
    );
    assert!(
        loaded
            .query_vanilla_monthly_convergence(&VanillaMonthlyConvergenceQuery {
                country: "AAA".into(),
                metric: Some("missing".into()),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyConvergenceQuery>(value).is_err());
}

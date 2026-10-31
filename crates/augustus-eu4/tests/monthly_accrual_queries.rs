// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::{VanillaMonthlyAccrualQuery, VanillaWorld};
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

#[test]
fn monthly_accrual_query_reports_all_or_selected_metrics_without_mutation() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let snapshot = loaded.save().unwrap();
    let all = loaded
        .query_vanilla_monthly_accrual(&VanillaMonthlyAccrualQuery {
            country: "AAA".into(),
            metric: None,
        })
        .unwrap();
    assert_eq!(all.country, "AAA");
    assert_eq!(all.day, 0);
    assert!(all.accounts.contains_key("adm_power"));
    assert!(all.accounts.contains_key("mil_power"));
    assert_eq!(loaded.save().unwrap(), snapshot);

    let selected = loaded
        .query_vanilla_monthly_accrual(&VanillaMonthlyAccrualQuery {
            country: "AAA".into(),
            metric: Some("adm_power".into()),
        })
        .unwrap();
    assert_eq!(selected.accounts.len(), 1);
    assert!(selected.accounts.contains_key("adm_power"));
    assert_eq!(loaded.save().unwrap(), snapshot);
}

#[test]
fn monthly_accrual_query_rejects_unknown_country_metric_and_fields() {
    let loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    assert!(
        loaded
            .query_vanilla_monthly_accrual(&VanillaMonthlyAccrualQuery {
                country: "ZZZ".into(),
                metric: None,
            })
            .is_err()
    );
    assert!(
        loaded
            .query_vanilla_monthly_accrual(&VanillaMonthlyAccrualQuery {
                country: "AAA".into(),
                metric: Some("missing_metric".into()),
            })
            .is_err()
    );
    let value = serde_json::json!({"country": "AAA", "unexpected": true});
    assert!(serde_json::from_value::<VanillaMonthlyAccrualQuery>(value).is_err());
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::WorldDate;
use augustus_eu4::VanillaWorld;
use jomini::common::{Date, PdsDate};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world")
}

fn copy_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(fixture()) {
        let entry = entry.unwrap();
        let target = temp
            .path()
            .join(entry.path().strip_prefix(fixture()).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(target).unwrap();
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
    temp
}

#[test]
fn snapshots_identify_supported_content_without_binding_to_installation_paths() {
    let mut original = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    original.advance(20).unwrap();
    let snapshot = original.save().unwrap();
    let relocated_root = copy_fixture();
    let mut relocated = VanillaWorld::load(relocated_root.path(), "1444.11.11").unwrap();
    relocated.restore(&snapshot).unwrap();
    assert_eq!(relocated.initial.world(), original.initial.world());
    let province = relocated_root.path().join("history/provinces/2 - East.txt");
    let content = std::fs::read_to_string(&province)
        .unwrap()
        .replace("base_tax = 4", "base_tax = 7");
    std::fs::write(province, content).unwrap();
    let mut changed = VanillaWorld::load(relocated_root.path(), "1444.11.11").unwrap();
    let before = changed.initial.world().clone();
    assert!(changed.restore(&snapshot).is_err());
    assert_eq!(changed.initial.world(), &before);
}

#[test]
fn translated_calendar_matches_existing_parser_and_monthly_tax_keeps_missing_inputs_unknown() {
    let mut loaded = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let parser_date = Date::parse("1444.11.11").unwrap();
    for day in [0, 19, 20, 51, 109, 110, 365] {
        loaded.advance(day - loaded.initial.world().day()).unwrap();
        let expected = parser_date.add_days(day as i32);
        assert_eq!(
            loaded.initial.world().date().unwrap(),
            Some(WorldDate {
                year: expected.year(),
                month: expected.month(),
                day: expected.day(),
            })
        );
    }
    assert_eq!(
        loaded.initial.world().taxation().unwrap().settlement_count,
        12
    );
    let alpha = loaded.country("AAA").unwrap();
    let tax = alpha.last_monthly_tax.unwrap();
    assert!(tax.known_subtotal_milli > 0);
    assert_eq!(tax.income_milli, None);
    assert_eq!(tax.credited_milli, None);
    assert_eq!(alpha.resources.unwrap().treasury_milli, None);
    assert!(tax.unresolved.contains("unknown_treasury"));
    // History selects an opening state, not an instruction to replay future
    // ruler deaths or country changes during a campaign.
    assert_eq!(
        alpha
            .politics
            .unwrap()
            .heir
            .as_ref()
            .unwrap()
            .name
            .as_deref(),
        Some("Rowan")
    );
}

#[test]
fn calendar_snapshot_resumes_without_repeating_settlement_and_rejects_other_content() {
    let mut continuous = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    let mut resumed = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    resumed.advance(45).unwrap();
    let snapshot = resumed.save().unwrap();
    let mut restarted = VanillaWorld::load(fixture(), "1444.11.11").unwrap();
    restarted.restore(&snapshot).unwrap();
    restarted.advance(320).unwrap();
    continuous.advance(365).unwrap();
    assert_eq!(restarted.initial.world(), continuous.initial.world());
    assert_eq!(restarted.save().unwrap(), continuous.save().unwrap());
    let mut different = VanillaWorld::load(fixture(), "1435.1.1").unwrap();
    let before = different.initial.world().clone();
    assert!(different.restore(&snapshot).is_err());
    assert_eq!(different.initial.world(), &before);
    assert!(resumed.restore("{}").is_err());
    assert_eq!(resumed.initial.world().day(), 45);
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use augustus_eu4::{
    VanillaMissionSelectionImport, read_vanilla_mission_selection,
    read_vanilla_mission_selection_with_token_map,
    read_vanilla_mission_selection_with_token_resolver,
};
use jomini::{
    binary::{Token, TokenResolver},
    common::Date,
};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn write_text_save(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, format!("EU4txt\n{body}")).unwrap();
    path
}

fn write_zip_save(directory: &Path, name: &str, game_state: &str, meta: Option<&str>) -> PathBuf {
    let path = directory.join(name);
    let mut archive = ZipWriter::new(fs::File::create(&path).unwrap());
    archive
        .start_file("gamestate", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(game_state.as_bytes()).unwrap();
    if let Some(meta) = meta {
        archive
            .start_file("meta", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(meta.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
    path
}

fn expected(date: &str, countries: &[(&str, &[&str])]) -> VanillaMissionSelectionImport {
    VanillaMissionSelectionImport {
        date: date.into(),
        countries: countries
            .iter()
            .map(|(tag, series)| {
                (
                    (*tag).into(),
                    series.iter().map(|series| (*series).into()).collect(),
                )
            })
            .collect(),
    }
}

#[test]
fn plaintext_import_distinguishes_missing_and_explicitly_empty_country_missions() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_text_save(
        directory.path(),
        "missions.eu4",
        "date=1444.11.11 countries={ AAA={country_missions={mission_slot={series_alpha series_beta} mission_slot={series_gamma}}} BBB={} CCC={country_missions={}} DDD={country_missions={mission_slot={}}} }",
    );

    let report = read_vanilla_mission_selection(path).unwrap();

    assert_eq!(
        report,
        expected(
            "1444.11.11",
            &[
                ("AAA", &["series_alpha", "series_beta", "series_gamma"]),
                ("CCC", &[]),
                ("DDD", &[]),
            ],
        )
    );
    assert!(!report.countries.contains_key("BBB"));
}

#[test]
fn zipped_import_uses_metadata_date_when_gamestate_date_is_absent() {
    let directory = tempfile::tempdir().unwrap();
    let path = write_zip_save(
        directory.path(),
        "missions.zip.eu4",
        "EU4txt\ncountries={ AAA={country_missions={mission_slot={series_alpha}}} }\n",
        Some("EU4txt\ndate=1444.11.12\n"),
    );

    let report = read_vanilla_mission_selection(path).unwrap();

    assert_eq!(
        report,
        expected("1444.11.12", &[("AAA", &["series_alpha"])]),
    );
}

#[test]
fn invalid_or_missing_save_dates_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let invalid = write_text_save(
        directory.path(),
        "invalid-date.eu4",
        "date=not-a-date countries={ AAA={} }",
    );
    let missing = write_zip_save(
        directory.path(),
        "missing-date.eu4",
        "EU4txt\ncountries={ AAA={} }\n",
        None,
    );

    assert!(read_vanilla_mission_selection(invalid).is_err());
    assert!(read_vanilla_mission_selection(missing).is_err());
}

#[test]
fn malformed_country_mission_structures_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let invalid_missions = [
        "country_missions={mission_slot={series_alpha}} country_missions={}",
        "country_missions={mission_slot={series_alpha} unknown_slot={}}",
        "country_missions={mission_slot=series_alpha}",
        "country_missions={mission_slot={series_alpha key=series_beta}}",
        "country_missions={mission_slot!={series_alpha}}",
        "country_missions={mission_slot={{series_alpha}}}",
        "country_missions={mission_slot={series_alpha} mission_slot={series_alpha}}",
        "country_missions={mission_slot={\"\"}}",
        "country_missions!={}",
    ];

    for (index, country_missions) in invalid_missions.iter().enumerate() {
        let path = write_text_save(
            directory.path(),
            &format!("invalid-{index}.eu4"),
            &format!("date=1444.11.11 countries={{ AAA={{{country_missions}}} }}"),
        );
        assert!(
            read_vanilla_mission_selection(path).is_err(),
            "accepted malformed country_missions: {country_missions}"
        );
    }
}

#[test]
fn duplicate_country_blocks_and_root_fields_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let invalid_roots = [
        "date=1444.11.11 countries={ AAA={} AAA={country_missions={mission_slot={series_alpha}}} }",
        "date=1444.11.11 countries={ AAA={country_missions={}} AAA={country_missions={mission_slot={series_alpha}}} }",
        "date=1444.11.11 countries={ AAA={} AAA={} }",
        "date=1444.11.11 date=1444.11.12 countries={ AAA={} }",
        "date=1444.11.11 countries={ AAA={} } countries={ BBB={} }",
    ];

    for (index, body) in invalid_roots.iter().enumerate() {
        let path = write_text_save(directory.path(), &format!("duplicate-{index}.eu4"), body);
        assert!(
            read_vanilla_mission_selection(path).is_err(),
            "accepted duplicate save fields: {body}"
        );
    }
}

struct MissionSelectionTestTokenResolver;

impl TokenResolver for MissionSelectionTestTokenResolver {
    fn resolve(&self, token: u16) -> Option<&str> {
        match token {
            0x3000 => Some("date"),
            0x3001 => Some("countries"),
            0x3002 => Some("AAA"),
            0x3003 => Some("country_missions"),
            0x3004 => Some("mission_slot"),
            _ => None,
        }
    }

    fn lookup(&self, index: u32) -> Option<&str> {
        match index {
            1 => Some("series_alpha"),
            2 => Some("series_beta"),
            _ => None,
        }
    }
}

fn binary_save() -> Vec<u8> {
    let date = Date::parse("1444.11.11").unwrap().to_binary();
    let mut bytes = b"EU4bin".to_vec();
    for token in [
        Token::Id(0x3000),
        Token::Equal,
        Token::I32(date),
        Token::Id(0x3001),
        Token::Equal,
        Token::Open,
        Token::Id(0x3002),
        Token::Equal,
        Token::Open,
        Token::Id(0x3003),
        Token::Equal,
        Token::Open,
        Token::Id(0x3004),
        Token::Equal,
        Token::Open,
        Token::Lookup(1),
        Token::Lookup(2),
        Token::Close,
        Token::Close,
        Token::Close,
        Token::Close,
    ] {
        token.write(&mut bytes).unwrap();
    }
    bytes
}

#[test]
fn binary_import_supports_a_token_map_and_an_integration_resolver() {
    let directory: TempDir = tempfile::tempdir().unwrap();
    let path = directory.path().join("missions.eu4");
    fs::write(&path, binary_save()).unwrap();
    let token_map = directory.path().join("mission-selection-tokens.txt");
    fs::write(
        &token_map,
        "0x3000 date\n0x3001 countries\n0x3002 AAA\n0x3003 country_missions\n0x3004 mission_slot\nlookup 1 series_alpha\nlookup 2 series_beta\n",
    )
    .unwrap();

    let mapped = read_vanilla_mission_selection_with_token_map(&path, &token_map).unwrap();
    let resolved = read_vanilla_mission_selection_with_token_resolver(
        &path,
        MissionSelectionTestTokenResolver,
    )
    .unwrap();

    let selection = expected("1444.11.11", &[("AAA", &["series_alpha", "series_beta"])]);
    assert_eq!(mapped, selection);
    assert_eq!(resolved, selection);
}

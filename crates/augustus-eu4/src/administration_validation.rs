// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use augustus_core::{AreaStatus, CoreType, ProvinceClass, WorldDate};
use jomini::{
    TextToken,
    text::{ObjectReader, ValueReader},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::bootstrap::equal;
use crate::setup::micros;
use crate::{EXPANSIONS, Error, ParsedText, TextDocument, TextEncoding, VanillaWorld};

#[derive(Debug, Default, Serialize)]
pub struct AdministrationCheckCount {
    pub matched: usize,
    pub different: usize,
    pub unknown: usize,
}

#[derive(Debug, Serialize)]
pub struct AdministrationDifference {
    pub province: Option<u32>,
    pub country: Option<String>,
    pub area: Option<String>,
    pub field: String,
    pub expected: Value,
    pub observed: Value,
}

/// Save observations are compared with the existing world, never imported into it.
#[derive(Debug, Default, Serialize)]
pub struct AdministrationComparison {
    pub checked_provinces: usize,
    pub observed_states: usize,
    pub observed_core_claims: usize,
    pub expansion_count: usize,
    pub checks: BTreeMap<String, AdministrationCheckCount>,
    pub differences: Vec<AdministrationDifference>,
}

#[derive(Debug, Default)]
struct SavedProvince {
    owner: Option<String>,
    cores: BTreeMap<String, CoreType>,
    raw_autonomy: i64,
}

type Object<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

fn single<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Field<'a, 'b>>, Error> {
    let mut result = None;
    for (key, op, value) in reader.fields().filter(|(k, _, _)| k.read_string() == name) {
        let fail = |message: &str| {
            Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
        };
        equal(op, &fail)?;
        if result.replace(value).is_some() {
            return Err(fail(&format!("duplicate save field {name}")));
        }
    }
    Ok(result)
}

fn required<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Field<'a, 'b>, Error> {
    single(parsed, reader, name)?
        .ok_or_else(|| Error::message(parsed.source(), format!("missing save field {name}")))
}

fn scalar(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|e| Error::caused_by(parsed.source(), e))
}

fn object<'a, 'b>(parsed: &ParsedText<'_>, value: &Field<'a, 'b>) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. })
        && !(matches!(value.token(), TextToken::Array { mixed: false, .. })
            && value.read_array().is_ok_and(|a| a.is_empty()))
    {
        return Err(Error::message(parsed.source(), "expected save object"));
    }
    value
        .read_object()
        .map_err(|e| Error::caused_by(parsed.source(), e))
}

fn strings(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<Vec<String>, Error> {
    if !matches!(value.token(), TextToken::Array { mixed: false, .. }) {
        return Err(Error::message(parsed.source(), "expected save scalar list"));
    }
    let array = value
        .read_array()
        .map_err(|e| Error::caused_by(parsed.source(), e))?;
    array.values().map(|v| scalar(parsed, &v)).collect()
}

fn check_metadata(parsed: &ParsedText<'_>) -> Result<usize, Error> {
    let root = parsed.reader();
    let fail = |message: &str| Error::message(parsed.source(), message);
    for name in ["date", "start_date"] {
        if scalar(parsed, &required(parsed, &root, name)?)? != "1444.11.11" {
            return Err(fail(
                "administration comparison requires an unadvanced 1444.11.11 start",
            ));
        }
    }
    if scalar(parsed, &required(parsed, &root, "campaign_length")?)? != "0" {
        return Err(fail(
            "administration comparison requires campaign_length = 0",
        ));
    }
    let version = object(parsed, &required(parsed, &root, "savegame_version")?)?;
    for (name, expected) in [
        ("first", "1"),
        ("second", "37"),
        ("third", "5"),
        ("forth", "0"),
    ] {
        if scalar(parsed, &required(parsed, &version, name)?)? != expected {
            return Err(fail("administration comparison requires EU4 1.37.5.0"));
        }
    }
    for name in ["mods_enabled", "mods_enabled_names"] {
        if let Some(value) = single(parsed, &root, name)?
            && !strings(parsed, &value)?.is_empty()
        {
            return Err(fail(
                "administration comparison requires a save with no mods",
            ));
        }
    }
    if let Some(value) = single(parsed, &root, "is_ironman")?
        && scalar(parsed, &value)? != "no"
    {
        return Err(fail(
            "administration comparison requires a non-Ironman save",
        ));
    }
    let expansions = strings(parsed, &required(parsed, &root, "dlc_enabled")?)?;
    let unique: BTreeSet<_> = expansions.iter().map(String::as_str).collect();
    if unique.len() != expansions.len() || EXPANSIONS.iter().any(|name| !unique.contains(name)) {
        return Err(fail(
            "administration comparison requires all 22 baseline expansions",
        ));
    }
    Ok(expansions.len())
}

fn read_provinces(parsed: &ParsedText<'_>) -> Result<BTreeMap<u32, SavedProvince>, Error> {
    let root = object(parsed, &required(parsed, &parsed.reader(), "provinces")?)?;
    let mut result = BTreeMap::new();
    for (key, op, value) in root.fields() {
        let fail = |message: &str| {
            Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
        };
        equal(op, &fail)?;
        let id = key
            .read_string()
            .strip_prefix('-')
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| fail("expected negative save province identifier"))?;
        let province = object(parsed, &value)?;
        let owner = single(parsed, &province, "owner")?
            .map(|v| scalar(parsed, &v))
            .transpose()?;
        let mut cores = BTreeMap::new();
        if let Some(value) = single(parsed, &province, "cores")? {
            for tag in strings(parsed, &value)? {
                if tag.is_empty() || cores.insert(tag, CoreType::Full).is_some() {
                    return Err(fail("empty or duplicate saved core"));
                }
            }
        }
        for (_, op, value) in province
            .fields()
            .filter(|(k, _, _)| k.read_string() == "territorial_core")
        {
            equal(op, &fail)?;
            let tag = scalar(parsed, &value)?;
            let Some(core) = cores.get_mut(&tag) else {
                return Err(fail("territorial core is not listed in saved cores"));
            };
            if *core == CoreType::Territorial {
                return Err(fail("duplicate territorial core"));
            }
            *core = CoreType::Territorial;
        }
        // Plaintext EU4 saves omit zero-valued local_autonomy, as in the save schema.
        let raw_autonomy = single(parsed, &province, "local_autonomy")?
            .map(|v| scalar(parsed, &v))
            .transpose()?
            .map(|s| {
                micros(&s)
                    .filter(|v| *v >= 0 && *v <= 100_000_000 && *v % 100 == 0)
                    .map(|v| v / 100)
                    .ok_or_else(|| fail("invalid saved autonomy percentage"))
            })
            .transpose()?
            .unwrap_or(0);
        if result
            .insert(
                id,
                SavedProvince {
                    owner,
                    cores,
                    raw_autonomy,
                },
            )
            .is_some()
        {
            return Err(fail("duplicate save province"));
        }
    }
    Ok(result)
}

fn read_states(parsed: &ParsedText<'_>) -> Result<BTreeSet<(String, String)>, Error> {
    let root = object(
        parsed,
        &required(parsed, &parsed.reader(), "map_area_data")?,
    )?;
    let mut result = BTreeSet::new();
    let mut areas = BTreeSet::new();
    for (key, op, value) in root.fields() {
        let fail = |message: &str| {
            Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
        };
        equal(op, &fail)?;
        let area = key.read_string();
        if area.is_empty() || !areas.insert(area.clone()) {
            return Err(fail("empty or duplicate saved area"));
        }
        let data = object(parsed, &value)?;
        let Some(state) = single(parsed, &data, "state")? else {
            continue;
        };
        let state = object(parsed, &state)?;
        if scalar(parsed, &required(parsed, &state, "area")?)? != area {
            return Err(fail("saved state area does not match its container"));
        }
        for (_, op, value) in state
            .fields()
            .filter(|(k, _, _)| k.read_string() == "country_state")
        {
            equal(op, &fail)?;
            let country = object(parsed, &value)?;
            let tag = scalar(parsed, &required(parsed, &country, "country")?)?;
            if tag.is_empty() || !result.insert((tag, area.clone())) {
                return Err(fail("empty or duplicate country state"));
            }
        }
    }
    Ok(result)
}

impl AdministrationComparison {
    fn record(
        &mut self,
        province: Option<u32>,
        country: Option<&str>,
        area: Option<&str>,
        field: &str,
        expected: Option<Value>,
        observed: Value,
    ) {
        let count = self.checks.entry(field.into()).or_default();
        match expected {
            None => count.unknown += 1,
            Some(expected) if expected == observed => count.matched += 1,
            Some(expected) => {
                count.different += 1;
                self.differences.push(AdministrationDifference {
                    province,
                    country: country.map(String::from),
                    area: area.map(String::from),
                    field: field.into(),
                    expected,
                    observed,
                });
            }
        }
    }
}

/// Checks a plaintext baseline save without changing either input or the core world.
/// Compressed and binary saves must be saved as uncompressed non-Ironman text first.
pub fn compare_administration_save(
    loaded: &VanillaWorld,
    path: impl AsRef<Path>,
) -> Result<AdministrationComparison, Error> {
    let path = path.as_ref();
    if loaded.report.start_date
        != (WorldDate {
            year: 1444,
            month: 11,
            day: 11,
        })
    {
        return Err(Error::message(
            path,
            "loaded world must use the 1444.11.11 baseline",
        ));
    }
    let mut bytes = std::fs::read(path).map_err(|e| Error::caused_by(path, e))?;
    if !bytes.starts_with(b"EU4txt") {
        return Err(Error::message(
            path,
            "expected uncompressed plaintext EU4txt save",
        ));
    }
    bytes[..6].fill(b' ');
    let document = TextDocument::from_bytes(path, bytes, TextEncoding::Windows1252);
    let parsed = document.parse()?;
    let expansion_count = check_metadata(&parsed)?;
    let provinces = read_provinces(&parsed)?;
    let states = read_states(&parsed)?;
    let mut report = AdministrationComparison {
        expansion_count,
        observed_states: states.len(),
        ..Default::default()
    };
    let world = loaded.initial.world();
    let administration = world.administration();
    let mut checked_areas = BTreeSet::new();
    for province in loaded
        .initial
        .provinces()
        .iter()
        .filter(|p| p.class == ProvinceClass::Land)
    {
        let observed = provinces.get(&province.id).ok_or_else(|| {
            Error::message(path, format!("missing saved province {}", province.id))
        })?;
        let entity = loaded
            .initial
            .province_entity(province.id)
            .expect("registered province");
        let expected = administration.provinces.get(&entity);
        report.checked_provinces += 1;
        report.observed_core_claims += observed.cores.len();
        report.record(
            Some(province.id),
            None,
            None,
            "owner",
            Some(json!(province.owner)),
            json!(observed.owner),
        );
        let cores = expected
            .filter(|p| p.cores_known && !p.cores.values().any(|v| *v == CoreType::Unspecified))
            .map(|expected| {
                json!(
                    expected
                        .cores
                        .iter()
                        .map(|(id, kind)| (
                            world.entity(*id).expect("validated core").name.clone(),
                            kind
                        ))
                        .collect::<BTreeMap<_, _>>()
                )
            });
        report.record(
            Some(province.id),
            None,
            None,
            "cores",
            cores,
            json!(observed.cores),
        );
        report.record(
            Some(province.id),
            None,
            None,
            "raw_autonomy",
            expected
                .and_then(|p| p.raw_autonomy_micros)
                .map(|v| json!(v)),
            json!(observed.raw_autonomy),
        );
        if let (Some(owner), Some(area)) = (&province.owner, expected.and_then(|p| p.area.as_ref()))
        {
            checked_areas.insert((owner.clone(), area.clone()));
        }
    }
    // Include saved states outside current ownership so extra state relationships cannot be hidden.
    checked_areas.extend(states.iter().cloned());
    for (country, area) in checked_areas {
        let expected = loaded
            .initial
            .country_entity(&country)
            .and_then(|id| administration.country_areas.get(&id))
            .and_then(|areas| areas.get(&area));
        let observed = if states.contains(&(country.clone(), area.clone())) {
            AreaStatus::State
        } else {
            AreaStatus::Territory
        };
        report.record(
            None,
            Some(&country),
            Some(&area),
            "area_status",
            expected.map(|v| json!(v)),
            json!(observed),
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> VanillaWorld {
        VanillaWorld::load(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
            "1444.11.11",
        )
        .unwrap()
    }

    fn save() -> String {
        let expansions = EXPANSIONS
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "EU4txt\ndate=1444.11.11\nstart_date=1444.11.11\ncampaign_length=0\nsavegame_version={{ first=1 second=37 third=5 forth=0 }}\ndlc_enabled={{ {expansions} }}\nmap_area_data={{ north_area={{ state={{ area=north_area country_state={{ country=BBB }} }} }} east_area={{ state={{ area=east_area country_state={{ country=AAA }} }} }} }}\nprovinces={{ -1={{ owner=BBB cores={{ BBB }} local_autonomy=30 }} -2={{ owner=AAA cores={{ AAA }} }} }}\n"
        )
    }

    fn compare(loaded: &VanillaWorld, bytes: &[u8]) -> Result<AdministrationComparison, Error> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("self_authored.eu4");
        std::fs::write(&path, bytes).unwrap();
        let before = std::fs::read(&path).unwrap();
        let world_before = loaded.initial.world().clone();
        let result = compare_administration_save(loaded, &path);
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert_eq!(*loaded.initial.world(), world_before);
        result
    }

    #[test]
    fn matches_known_state_core_and_initial_autonomy_inputs() {
        let loaded = fixture();
        let report = compare(&loaded, save().as_bytes()).unwrap();
        assert_eq!(report.checked_provinces, 2);
        assert_eq!(report.expansion_count, 22);
        assert_eq!(report.observed_states, 2);
        assert_eq!(report.observed_core_claims, 2);
        for name in ["owner", "cores", "area_status"] {
            assert_eq!(report.checks[name].matched, 2);
            assert_eq!(report.checks[name].unknown, 0);
        }
        assert_eq!(report.checks["raw_autonomy"].matched, 2);
        assert_eq!(report.checks["raw_autonomy"].unknown, 0);
        assert!(report.differences.is_empty());
        assert_eq!(
            loaded
                .province(1)
                .unwrap()
                .administration
                .unwrap()
                .raw_autonomy_micros,
            Some(300_000)
        );
    }

    #[test]
    fn reports_missing_states_owner_changes_and_territorial_core_types() {
        let loaded = fixture();
        let input = save()
            .replace("state={ area=east_area country_state={ country=AAA } }", "")
            .replace(
                "owner=BBB cores={ BBB }",
                "owner=AAA cores={ BBB } territorial_core=BBB",
            );
        let report = compare(&loaded, input.as_bytes()).unwrap();
        assert_eq!(report.checks["area_status"].different, 1);
        assert_eq!(report.checks["owner"].different, 1);
        assert_eq!(report.checks["cores"].different, 1);
        let difference = report
            .differences
            .iter()
            .find(|d| d.field == "cores")
            .unwrap();
        assert_eq!(difference.province, Some(1));
        assert_eq!(difference.expected["BBB"], "full");
        assert_eq!(difference.observed["BBB"], "territorial");
    }

    #[test]
    fn replaced_administration_is_reported_as_unknown_and_other_dates_are_rejected() {
        let mut loaded = fixture();
        loaded
            .initial
            .set_administration(Default::default())
            .unwrap();
        let report = compare(&loaded, save().as_bytes()).unwrap();
        assert_eq!(report.checks["cores"].unknown, 2);
        assert_eq!(report.checks["raw_autonomy"].unknown, 2);
        assert_eq!(report.checks["area_status"].unknown, 2);
        assert!(report.differences.is_empty());
        let later = VanillaWorld::load(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world"),
            "1445.1.1",
        )
        .unwrap();
        assert!(
            compare(&later, save().as_bytes())
                .unwrap_err()
                .to_string()
                .contains("loaded world")
        );
    }

    #[test]
    fn baseline_conditions_reject_mods_dates_missing_expansions_and_ironman() {
        let loaded = fixture();
        for input in [
            save() + "mods_enabled={ \"mod/example.mod\" }\n",
            save() + "mods_enabled_names={ \"Example\" }\n",
            save() + "is_ironman=yes\n",
            save().replace("date=1444.11.11", "date=1444.11.12"),
            save().replace("campaign_length=0", "campaign_length=1"),
            save().replace("third=5", "third=4"),
            save().replace("\"Domination\"", ""),
            save().replace("\"Domination\"", "\"Domination\" \"Domination\""),
            save() + "date=1444.11.11\n",
        ] {
            assert!(
                compare(&loaded, input.as_bytes()).is_err(),
                "accepted invalid metadata"
            );
        }
        assert!(
            compare(
                &loaded,
                (save() + "mods_enabled={}\nmods_enabled_names={}\nis_ironman=no\n").as_bytes()
            )
            .is_ok()
        );
        assert!(
            compare(&loaded, b"PK compressed save")
                .unwrap_err()
                .to_string()
                .contains("EU4txt")
        );
    }

    #[test]
    fn malformed_references_duplicates_and_autonomy_values_fail() {
        let loaded = fixture();
        for (from, to) in [
            ("local_autonomy=30", "local_autonomy=-1"),
            ("local_autonomy=30", "local_autonomy=101"),
            ("local_autonomy=30", "local_autonomy=0.00001"),
            ("local_autonomy=30", "local_autonomy=25 local_autonomy=26"),
            ("cores={ BBB }", "cores={ BBB BBB }"),
            ("cores={ BBB }", "cores={ BBB } territorial_core=AAA"),
            (
                "cores={ BBB }",
                "cores={ BBB } territorial_core=BBB territorial_core=BBB",
            ),
            ("area=north_area", "area=east_area"),
            ("country=BBB", "country=BBB country=AAA"),
            (
                "country_state={ country=BBB }",
                "country_state={ country=BBB } country_state={ country=BBB }",
            ),
            (
                "-2={ owner=AAA cores={ AAA } }",
                "-1={ owner=AAA cores={ AAA } }",
            ),
            ("-2={ owner=AAA cores={ AAA } }", ""),
            ("owner=AAA cores={ AAA }", "AAA BBB"),
        ] {
            assert!(
                compare(&loaded, save().replace(from, to).as_bytes()).is_err(),
                "accepted invalid structure: {to}"
            );
        }
    }
}

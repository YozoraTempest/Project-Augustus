// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

use augustus_core::{
    Affiliations, AreaStatus, CoreType, EntityId, InitialWorld, Politics, ProvinceClass, WorldDate,
};
use jomini::{
    TextToken,
    binary::{BasicTokenResolver, TokenResolver},
    common::Date,
    text::{ObjectReader, ValueReader},
};
use serde::Serialize;
use serde_json::{Value, json};
use zip::ZipArchive;

use crate::bootstrap::equal;
use crate::setup::micros;
use crate::state_edicts::{VanillaExternalStateEdict, VanillaExternalStateEdicts};
use crate::{EXPANSIONS, Error, ParsedText, TextDocument, TextEncoding, VanillaWorld};

pub(crate) type SaveTokenResolver = Box<dyn TokenResolver + Send>;

struct LocalSaveTokenResolver {
    tokens: BasicTokenResolver,
    lookups: BTreeMap<u32, String>,
}

impl TokenResolver for LocalSaveTokenResolver {
    fn resolve(&self, token: u16) -> Option<&str> {
        self.tokens.resolve(token)
    }

    fn lookup(&self, index: u32) -> Option<&str> {
        self.lookups.get(&index).map(String::as_str)
    }

    fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.lookups.is_empty()
    }
}

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

/// Membership flags imported from a supported unadvanced 1444.11.11 save.
#[derive(Debug, Default, Serialize)]
pub struct SavedImperialMembershipReport {
    pub save_date: String,
    pub provinces_checked: usize,
    pub provinces_in_hre: usize,
    pub provinces_outside_hre: usize,
}

/// Current emperor preferences imported from a supported unadvanced 1444.11.11 save.
#[derive(Debug, Default, Serialize)]
pub struct SavedOfficePreferenceReport {
    pub save_date: String,
    pub preferences_imported: usize,
}

/// HRE membership and emperor preferences imported together from one baseline save.
#[derive(Debug, Default, Serialize)]
pub struct SavedImperialBaselineReport {
    pub save_date: String,
    pub provinces_checked: usize,
    pub provinces_in_hre: usize,
    pub provinces_outside_hre: usize,
    pub preferences_imported: usize,
}

/// Administration facts read directly from an EU4 save.
///
/// These values are kept beside the long-standing external-state contract so
/// callers that construct `VanillaExternalState` remain source-compatible.
#[derive(Debug, Clone, Default, Serialize)]
pub struct VanillaSaveAdministration {
    pub province_core_types: BTreeMap<u32, BTreeMap<String, CoreType>>,
    pub country_area_statuses: BTreeMap<String, BTreeMap<String, AreaStatus>>,
}

const IMPERIAL_EMPEROR_OFFICE: &str = "imperial_emperor";

#[derive(Debug, Default)]
struct SavedProvince {
    owner: Option<String>,
    cores: BTreeMap<String, CoreType>,
    raw_autonomy: i64,
    in_empire: Option<bool>,
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

pub(crate) struct Eu4SaveDocuments {
    pub(crate) game_state: TextDocument,
    pub(crate) meta: Option<TextDocument>,
    pub(crate) zip_container: bool,
}

pub(crate) fn read_eu4_save_documents(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
) -> Result<Eu4SaveDocuments, Error> {
    let mut file = File::open(path).map_err(|error| Error::caused_by(path, error))?;
    let mut prefix = [0; 6];
    let prefix_length = file
        .read(&mut prefix)
        .map_err(|error| Error::caused_by(path, error))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| Error::caused_by(path, error))?;

    let mut bytes = Vec::new();
    if prefix_length == prefix.len() && (&prefix == b"EU4txt" || &prefix == b"EU4bin") {
        file.read_to_end(&mut bytes)
            .map_err(|error| Error::caused_by(path, error))?;
        return Ok(Eu4SaveDocuments {
            game_state: eu4_save_document(path, bytes, "save", token_map_path, token_resolver)?,
            meta: None,
            zip_container: false,
        });
    }

    if prefix.starts_with(b"PK\x03\x04") {
        let mut archive = ZipArchive::new(file).map_err(|error| Error::caused_by(path, error))?;
        let game_state = eu4_save_document(
            path,
            zip_entry_bytes(&mut archive, path, "gamestate")?,
            "gamestate",
            token_map_path,
            token_resolver,
        )?;
        let mut has_meta = false;
        for name in archive.file_names() {
            if name.map_err(|error| Error::caused_by(path, error))? == "meta" {
                has_meta = true;
                break;
            }
        }
        let meta = if has_meta {
            Some(eu4_save_document(
                path,
                zip_entry_bytes(&mut archive, path, "meta")?,
                "meta",
                token_map_path,
                token_resolver,
            )?)
        } else {
            None
        };
        return Ok(Eu4SaveDocuments {
            game_state,
            meta,
            zip_container: true,
        });
    }

    Err(Error::message(
        path,
        "expected an EU4txt/EU4bin save or ZIP-wrapped EU4 save",
    ))
}

fn zip_entry_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &Path,
    name: &str,
) -> Result<Vec<u8>, Error> {
    let mut entry = archive
        .by_name(name)
        .map_err(|error| Error::caused_by(path, error))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| Error::caused_by(path, error))?;
    Ok(bytes)
}

fn eu4_save_document(
    path: &Path,
    mut bytes: Vec<u8>,
    entry: &str,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
) -> Result<TextDocument, Error> {
    if bytes.starts_with(b"EU4txt") {
        bytes[..6].fill(b' ');
        return Ok(TextDocument::from_bytes(
            path,
            bytes,
            TextEncoding::Windows1252,
        ));
    }

    if bytes.starts_with(b"EU4bin") {
        if token_resolver.is_none() {
            let Some(token_map_path) = token_map_path else {
                return Err(Error::message(
                    path,
                    format!(
                        "EU4 save {entry} is binary and requires a local token map; use read_vanilla_save_state_with_token_map"
                    ),
                ));
            };
            let loaded = read_local_save_token_map(token_map_path)?;
            if loaded.is_empty() {
                return Err(Error::message(token_map_path, "EU4 token map is empty"));
            }
            *token_resolver = Some(Box::new(loaded));
        }
        return crate::save_binary::decode_eu4_binary_save(
            path,
            &bytes,
            token_resolver.as_ref().expect("loaded EU4 token map"),
        );
    }

    Err(Error::message(
        path,
        format!("EU4 save {entry} must start with EU4txt or EU4bin"),
    ))
}

fn read_local_save_token_map(path: &Path) -> Result<LocalSaveTokenResolver, Error> {
    let file = File::open(path).map_err(|error| Error::caused_by(path, error))?;
    let mut token_lines = Vec::new();
    let mut lookups = BTreeMap::new();
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| Error::caused_by(path, error))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((kind_or_token, value)) = line.split_once(char::is_whitespace) else {
            return Err(Error::message(
                path,
                format!("invalid EU4 token map line {}", line_index + 1),
            ));
        };
        let value = value.trim_start();
        if kind_or_token == "lookup" {
            let Some((index, value)) = value.split_once(char::is_whitespace) else {
                return Err(Error::message(
                    path,
                    format!("invalid EU4 token lookup line {}", line_index + 1),
                ));
            };
            let index = if let Some(hex) = index.strip_prefix("0x") {
                u32::from_str_radix(hex, 16)
            } else {
                index.parse()
            }
            .map_err(|error| Error::caused_by(path, error))?;
            if value.trim().is_empty() || lookups.insert(index, value.trim().to_owned()).is_some() {
                return Err(Error::message(
                    path,
                    format!(
                        "empty or duplicate EU4 token lookup on line {}",
                        line_index + 1
                    ),
                ));
            }
        } else {
            if value.trim().is_empty() {
                return Err(Error::message(
                    path,
                    format!("empty EU4 token name on line {}", line_index + 1),
                ));
            }
            token_lines.extend_from_slice(kind_or_token.as_bytes());
            token_lines.push(b' ');
            token_lines.extend_from_slice(value.as_bytes());
            token_lines.push(b'\n');
        }
    }
    let tokens = BasicTokenResolver::from_text_lines(Cursor::new(token_lines))
        .map_err(|error| Error::caused_by(path, error))?;
    Ok(LocalSaveTokenResolver { tokens, lookups })
}

pub(crate) fn plaintext_save_document(path: &Path) -> Result<TextDocument, Error> {
    let mut token_resolver = None;
    let documents = read_eu4_save_documents(path, None, &mut token_resolver)?;
    if documents.zip_container {
        return Err(Error::message(
            path,
            "administration comparison requires an uncompressed EU4txt save",
        ));
    }
    Ok(documents.game_state)
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
        let in_empire = if let Some(history) = single(parsed, &province, "history")? {
            let history = object(parsed, &history)?;
            single(parsed, &history, "hre")?
                .map(|value| match scalar(parsed, &value)?.as_str() {
                    "yes" => Ok(true),
                    "no" => Ok(false),
                    _ => Err(fail("invalid saved HRE membership")),
                })
                .transpose()?
        } else {
            None
        };
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
                    in_empire,
                },
            )
            .is_some()
        {
            return Err(fail("duplicate save province"));
        }
    }
    Ok(result)
}

fn read_saved_office_preferences(
    parsed: &ParsedText<'_>,
    initial: &InitialWorld,
) -> Result<BTreeMap<EntityId, EntityId>, Error> {
    let countries = object(parsed, &required(parsed, &parsed.reader(), "countries")?)?;
    let mut seen_tags = BTreeSet::new();
    let mut preferences = BTreeMap::new();
    for (key, op, value) in countries.fields() {
        let fail = |message: &str| {
            Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
        };
        equal(op, &fail)?;
        let tag = key.read_string();
        if !crate::script::country_identifier(&tag) || !seen_tags.insert(tag.clone()) {
            return Err(fail("invalid or duplicate saved country identifier"));
        }
        let country = object(parsed, &value)?;
        let Some(value) = single(parsed, &country, "preferred_emperor")? else {
            continue;
        };
        let preferred_tag = scalar(parsed, &value)?;
        if !crate::script::country_identifier(&preferred_tag) {
            return Err(fail("invalid saved emperor preference country"));
        }
        let voter = initial
            .country_entity(&tag)
            .ok_or_else(|| fail(&format!("unknown country with emperor preference: {tag}")))?;
        let preferred = initial.country_entity(&preferred_tag).ok_or_else(|| {
            fail(&format!(
                "unknown preferred emperor country: {preferred_tag}"
            ))
        })?;
        if preferences.insert(voter, preferred).is_some() {
            return Err(fail("duplicate saved emperor preference"));
        }
    }
    Ok(preferences)
}

fn baseline_save_date(
    parsed: &ParsedText<'_>,
    path: &Path,
    subject: &str,
) -> Result<String, Error> {
    check_metadata(parsed)?;
    let save_date = scalar(parsed, &required(parsed, &parsed.reader(), "date")?)?;
    if save_date != "1444.11.11" {
        return Err(Error::message(
            path,
            format!("{subject} import requires a save dated 1444.11.11"),
        ));
    }
    Ok(save_date)
}

fn saved_hre_membership_state(
    initial: &InitialWorld,
    parsed: &ParsedText<'_>,
    path: &Path,
    save_date: String,
) -> Result<(Affiliations, SavedImperialMembershipReport), Error> {
    let saved = read_provinces(parsed)?;
    if saved.len() != initial.provinces().len() {
        return Err(Error::message(
            path,
            "save province count does not match loaded content",
        ));
    }

    let mut affiliations = initial.world().affiliations().clone();
    let dimension = affiliations
        .0
        .entry(crate::imperial_membership::DIMENSION.into())
        .or_default();
    dimension.groups.entry("hre".into()).or_insert(None);
    let mut report = SavedImperialMembershipReport {
        save_date,
        ..Default::default()
    };
    for province in initial.provinces() {
        let state = saved.get(&province.id).ok_or_else(|| {
            Error::message(path, format!("save is missing province {}", province.id))
        })?;
        let in_empire = state
            .in_empire
            .or_else(|| state.owner.is_none().then_some(false))
            .ok_or_else(|| {
                Error::message(
                    path,
                    format!(
                        "save is missing HRE membership for province {}",
                        province.id
                    ),
                )
            })?;
        let entity = initial
            .province_entity(province.id)
            .expect("registered province");
        if in_empire {
            dimension.members.insert(entity, Some("hre".into()));
            dimension.absent_members.remove(&entity);
            report.provinces_in_hre += 1;
        } else {
            dimension.members.remove(&entity);
            dimension.absent_members.insert(entity);
            report.provinces_outside_hre += 1;
        }
        report.provinces_checked += 1;
    }
    Ok((affiliations, report))
}

fn politics_with_office_preferences(
    initial: &InitialWorld,
    preferences: &BTreeMap<EntityId, EntityId>,
) -> Politics {
    let mut politics = initial.world().politics().clone();
    for country in politics.countries.values_mut() {
        country.office_preferences.remove(IMPERIAL_EMPEROR_OFFICE);
    }
    for (voter, preferred) in preferences {
        politics
            .countries
            .entry(*voter)
            .or_default()
            .office_preferences
            .insert(IMPERIAL_EMPEROR_OFFICE.into(), *preferred);
    }
    politics
}

/// Imports only HRE province-membership flags from an unadvanced 1444.11.11 save.
/// Government reforms and all other campaign state remain sourced from the loaded world.
pub fn apply_saved_hre_membership(
    loaded: &mut VanillaWorld,
    path: impl AsRef<Path>,
) -> Result<SavedImperialMembershipReport, Error> {
    let path = path.as_ref();
    let expected = WorldDate {
        year: 1444,
        month: 11,
        day: 11,
    };
    if loaded.report.start_date != expected {
        return Err(Error::message(
            path,
            "HRE membership import requires a world loaded at 1444.11.11",
        ));
    }
    let document = plaintext_save_document(path)?;
    let parsed = document.parse()?;
    let save_date = baseline_save_date(&parsed, path, "HRE membership")?;
    let (affiliations, report) =
        saved_hre_membership_state(&loaded.initial, &parsed, path, save_date)?;
    loaded
        .initial
        .set_affiliations(affiliations)
        .map_err(|e| Error::caused_by(path, e))?;
    Ok(report)
}

/// Imports only current emperor preferences from an unadvanced 1444.11.11 save.
/// Uncompressed plaintext and the baseline game version, DLC, and mod set are required.
pub fn apply_saved_office_preferences(
    loaded: &mut VanillaWorld,
    path: impl AsRef<Path>,
) -> Result<SavedOfficePreferenceReport, Error> {
    let path = path.as_ref();
    let expected = WorldDate {
        year: 1444,
        month: 11,
        day: 11,
    };
    if loaded.report.start_date != expected {
        return Err(Error::message(
            path,
            "emperor preference import requires a world loaded at 1444.11.11",
        ));
    }
    let document = plaintext_save_document(path)?;
    let parsed = document.parse()?;
    let save_date = baseline_save_date(&parsed, path, "emperor preference")?;
    let preferences = read_saved_office_preferences(&parsed, &loaded.initial)?;
    let politics = politics_with_office_preferences(&loaded.initial, &preferences);
    loaded
        .initial
        .set_politics(politics)
        .map_err(|e| Error::caused_by(path, e))?;
    Ok(SavedOfficePreferenceReport {
        save_date,
        preferences_imported: preferences.len(),
    })
}

/// Imports HRE province membership and emperor preferences from one baseline save pass.
/// The world is replaced only after both state sets validate successfully.
pub fn apply_saved_imperial_baseline(
    loaded: &mut VanillaWorld,
    path: impl AsRef<Path>,
) -> Result<SavedImperialBaselineReport, Error> {
    let path = path.as_ref();
    let expected = WorldDate {
        year: 1444,
        month: 11,
        day: 11,
    };
    if loaded.report.start_date != expected {
        return Err(Error::message(
            path,
            "imperial baseline import requires a world loaded at 1444.11.11",
        ));
    }
    let document = plaintext_save_document(path)?;
    let parsed = document.parse()?;
    let save_date = baseline_save_date(&parsed, path, "imperial baseline")?;
    let (affiliations, hre_report) =
        saved_hre_membership_state(&loaded.initial, &parsed, path, save_date.clone())?;
    let preferences = read_saved_office_preferences(&parsed, &loaded.initial)?;
    let politics = politics_with_office_preferences(&loaded.initial, &preferences);

    let mut initial = loaded.initial.clone();
    initial
        .set_affiliations(affiliations)
        .map_err(|error| Error::caused_by(path, error))?;
    initial
        .set_politics(politics)
        .map_err(|error| Error::caused_by(path, error))?;
    loaded.initial = initial;

    Ok(SavedImperialBaselineReport {
        save_date,
        provinces_checked: hre_report.provinces_checked,
        provinces_in_hre: hre_report.provinces_in_hre,
        provinces_outside_hre: hre_report.provinces_outside_hre,
        preferences_imported: preferences.len(),
    })
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

pub(crate) fn read_save_administration(
    parsed: &ParsedText<'_>,
) -> Result<VanillaSaveAdministration, Error> {
    let provinces = read_saved_core_types(parsed)?;
    let states = single(parsed, &parsed.reader(), "map_area_data")?
        .map(|_| read_states(parsed))
        .transpose()?
        .unwrap_or_default();
    let mut country_area_statuses: BTreeMap<String, BTreeMap<String, AreaStatus>> = BTreeMap::new();
    for (country, area) in states {
        country_area_statuses
            .entry(country)
            .or_default()
            .insert(area, AreaStatus::State);
    }
    Ok(VanillaSaveAdministration {
        province_core_types: provinces,
        country_area_statuses,
    })
}

fn read_saved_core_types(
    parsed: &ParsedText<'_>,
) -> Result<BTreeMap<u32, BTreeMap<String, CoreType>>, Error> {
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
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| fail("expected negative save province identifier"))?;
        let province = object(parsed, &value)?;
        let mut cores = BTreeMap::new();
        if let Some(value) = single(parsed, &province, "cores")? {
            match value.token() {
                TextToken::Array { .. } => {
                    for tag in strings(parsed, &value)? {
                        if tag.is_empty() || cores.insert(tag, CoreType::Full).is_some() {
                            return Err(fail("empty or duplicate saved core"));
                        }
                    }
                }
                TextToken::Object { .. } => {
                    let entries = object(parsed, &value)?;
                    for (key, op, value) in entries.fields() {
                        equal(op, &fail)?;
                        let tag = key.read_string();
                        if tag.is_empty() {
                            return Err(fail("empty saved core"));
                        }
                        if op.is_some() {
                            match scalar(parsed, &value)?.as_str() {
                                "yes" => {
                                    if cores.insert(tag, CoreType::Full).is_some() {
                                        return Err(fail("duplicate saved core"));
                                    }
                                }
                                "no" => {}
                                _ => return Err(fail("invalid saved core value")),
                            }
                        } else if cores.insert(tag, CoreType::Full).is_some() {
                            return Err(fail("duplicate saved core"));
                        }
                    }
                }
                _ => return Err(fail("invalid saved core list")),
            }
        }
        for (_, op, value) in province
            .fields()
            .filter(|(key, _, _)| key.read_string() == "territorial_core")
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
        if result.insert(id, cores).is_some() {
            return Err(fail("duplicate save province"));
        }
    }
    Ok(result)
}

pub(crate) fn read_state_edicts(
    parsed: &ParsedText<'_>,
) -> Result<VanillaExternalStateEdicts, Error> {
    let root = parsed.reader();
    let Some(map_area_data) = single(parsed, &root, "map_area_data")? else {
        return Ok(BTreeMap::new());
    };
    let root = object(parsed, &map_area_data)?;
    let mut result = VanillaExternalStateEdicts::new();
    let mut areas = BTreeSet::new();
    let mut states = BTreeSet::new();
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
            .filter(|(key, _, _)| key.read_string() == "country_state")
        {
            equal(op, &fail)?;
            let country_state = object(parsed, &value)?;
            let country = scalar(parsed, &required(parsed, &country_state, "country")?)?;
            if country.is_empty() || !states.insert((country.clone(), area.clone())) {
                return Err(fail("empty or duplicate country state"));
            }
            let Some(active_edict) = single(parsed, &country_state, "active_edict")? else {
                continue;
            };
            let active_edict = object(parsed, &active_edict)?;
            let identifier = scalar(parsed, &required(parsed, &active_edict, "which")?)?;
            if identifier.is_empty() {
                return Err(fail("empty saved state edict identifier"));
            }
            let enacted_on = scalar(parsed, &required(parsed, &active_edict, "date")?)?;
            Date::parse(&enacted_on).map_err(|error| Error::caused_by(parsed.source(), error))?;
            if result
                .entry(country.clone())
                .or_default()
                .insert(
                    area.clone(),
                    VanillaExternalStateEdict {
                        identifier,
                        enacted_on,
                    },
                )
                .is_some()
            {
                return Err(fail("duplicate saved state edict"));
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
/// Compares an unadvanced EU4 save with the initialized administration state.
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
    let document = plaintext_save_document(path)?;
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

    fn hre_save(loaded: &VanillaWorld) -> String {
        let base = save();
        let prefix = base.split("provinces=").next().unwrap();
        let provinces = loaded
            .initial
            .provinces()
            .iter()
            .map(|province| {
                let owner = province
                    .owner
                    .as_ref()
                    .map(|owner| format!(" owner={owner}"))
                    .unwrap_or_default();
                let membership = if province.id == 1 { "yes" } else { "no" };
                format!("-{}={{{owner} history={{hre={membership}}}}}", province.id)
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{prefix}provinces={{ {provinces} }}\n")
    }

    fn preference_save(loaded: &VanillaWorld, countries: &str) -> String {
        let save = hre_save(loaded);
        let (prefix, provinces) = save.split_once("provinces=").unwrap();
        format!("{prefix}countries={{ {countries} }}\nprovinces={provinces}")
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

    fn save_to_temp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("self_authored.eu4");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn imports_explicit_hre_membership_from_a_supported_unadvanced_save() {
        let mut loaded = fixture();
        let (dir, path) = save_to_temp(hre_save(&loaded).as_bytes());
        let report = apply_saved_hre_membership(&mut loaded, &path).unwrap();
        let north = loaded.initial.province_entity(1).unwrap();
        let east = loaded.initial.province_entity(2).unwrap();
        let province_count = loaded.initial.provinces().len();

        assert_eq!(report.save_date, "1444.11.11");
        assert_eq!(report.provinces_checked, province_count);
        assert_eq!(report.provinces_in_hre, 1);
        assert_eq!(report.provinces_outside_hre, province_count - 1);
        assert_eq!(
            loaded
                .initial
                .world()
                .affiliations()
                .belongs_to("imperial_membership", north, "hre"),
            Some(true)
        );
        assert_eq!(
            loaded
                .initial
                .world()
                .affiliations()
                .belongs_to("imperial_membership", east, "hre"),
            Some(false)
        );
        assert!(dir.path().exists());
    }

    #[test]
    fn imports_and_replaces_saved_imperial_emperor_preferences() {
        let mut loaded = fixture();
        let input = preference_save(&loaded, "AAA={ preferred_emperor=BBB } BBB={}");
        let (dir, path) = save_to_temp(input.as_bytes());
        let before = std::fs::read(&path).unwrap();
        let report = apply_saved_office_preferences(&mut loaded, &path).unwrap();
        let aaa = loaded.initial.country_entity("AAA").unwrap();
        let bbb = loaded.initial.country_entity("BBB").unwrap();

        assert_eq!(report.save_date, "1444.11.11");
        assert_eq!(report.preferences_imported, 1);
        assert_eq!(
            loaded.initial.world().politics().countries[&aaa]
                .office_preferences
                .get(IMPERIAL_EMPEROR_OFFICE),
            Some(&bbb)
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let no_preferences = preference_save(&loaded, "AAA={} BBB={}");
        std::fs::write(&path, no_preferences).unwrap();
        let report = apply_saved_office_preferences(&mut loaded, &path).unwrap();
        assert_eq!(report.preferences_imported, 0);
        assert!(
            loaded.initial.world().politics().countries[&aaa]
                .office_preferences
                .is_empty()
        );
        assert!(dir.path().exists());
    }

    #[test]
    fn imports_hre_membership_and_preferences_as_one_baseline_state() {
        let mut loaded = fixture();
        let input = preference_save(&loaded, "AAA={ preferred_emperor=BBB } BBB={}");
        let (dir, path) = save_to_temp(input.as_bytes());
        let report = apply_saved_imperial_baseline(&mut loaded, &path).unwrap();
        let north = loaded.initial.province_entity(1).unwrap();
        let aaa = loaded.initial.country_entity("AAA").unwrap();
        let bbb = loaded.initial.country_entity("BBB").unwrap();

        assert_eq!(report.save_date, "1444.11.11");
        assert_eq!(report.provinces_checked, 6);
        assert_eq!(report.provinces_in_hre, 1);
        assert_eq!(report.provinces_outside_hre, 5);
        assert_eq!(report.preferences_imported, 1);
        assert_eq!(
            loaded
                .initial
                .world()
                .affiliations()
                .belongs_to("imperial_membership", north, "hre"),
            Some(true)
        );
        assert_eq!(
            loaded.initial.world().politics().countries[&aaa]
                .office_preferences
                .get(IMPERIAL_EMPEROR_OFFICE),
            Some(&bbb)
        );
        assert!(dir.path().exists());

        let invalid = input.replace("preferred_emperor=BBB", "preferred_emperor=XXX");
        let mut unchanged = fixture();
        let before = unchanged.initial.world().clone();
        let (invalid_dir, invalid_path) = save_to_temp(invalid.as_bytes());
        assert!(apply_saved_imperial_baseline(&mut unchanged, &invalid_path).is_err());
        assert_eq!(unchanged.initial.world(), &before);
        assert!(invalid_dir.path().exists());
    }

    #[test]
    fn invalid_saved_emperor_preferences_fail_without_mutating_the_world() {
        let source = fixture();
        let valid = preference_save(&source, "AAA={ preferred_emperor=BBB } BBB={}");
        let cases = [
            valid.replace("preferred_emperor=BBB", "preferred_emperor=XXX"),
            valid.replace(
                "preferred_emperor=BBB",
                "preferred_emperor=BBB preferred_emperor=BBB",
            ),
            valid.replace("AAA={", "aaA={"),
            valid.replace("countries=", "missing_countries="),
        ];
        for input in cases {
            let mut loaded = fixture();
            let before = loaded.initial.world().clone();
            let (dir, path) = save_to_temp(input.as_bytes());
            assert!(apply_saved_office_preferences(&mut loaded, &path).is_err());
            assert_eq!(loaded.initial.world(), &before);
            assert!(dir.path().exists());
        }
    }

    #[test]
    fn invalid_hre_save_imports_fail_without_mutating_the_world() {
        let source = fixture();
        let valid = hre_save(&source);
        let cases = [
            valid.replace("date=1444.11.11", "date=1444.11.12"),
            valid.replace("history={hre=yes}", "history={}"),
            valid.replace("-2={", "-999={"),
            valid.replace("third=5", "third=4"),
            valid.replace("\"Domination\"", ""),
        ];
        for input in cases {
            let mut loaded = fixture();
            let before = loaded.initial.world().clone();
            let (dir, path) = save_to_temp(input.as_bytes());
            assert!(apply_saved_hre_membership(&mut loaded, &path).is_err());
            assert_eq!(loaded.initial.world(), &before);
            assert!(dir.path().exists());
        }
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

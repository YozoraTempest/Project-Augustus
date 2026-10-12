// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
};

use augustus_core::{
    Calendar, CountryState, InitialWorld, ProvinceClass, ProvinceState, Taxation, WorldDate,
};
use jomini::{
    Encoding,
    common::{Date, PdsDate},
    text::{ObjectReader, Operator},
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::administration::{AdministrationDraft, AdministrationReport};
use crate::attributes::{ATTRIBUTE_NAMES, AttributeDraft, AttributeReport};
use crate::country_status::CountryStatusDraft;
use crate::economy::{EconomyDraft, EconomyReport};
use crate::estates::{EstateDraft, EstateReport};
use crate::governments::{GovernmentDraft, GovernmentReport};
use crate::ideas::{IdeaDraft, IdeaReport};
use crate::politics::{self, CountryDraft, RelationSource, RelationView};
use crate::setup::{SetupDraft, SetupReport};
use crate::{ContentRoot, Error, ParsedText, SourceLocation, TextEncoding};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Clone, Serialize)]
pub struct LoadedSource {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Serialize)]
pub struct BootstrapReport {
    pub ruleset: &'static str,
    pub game_version: String,
    pub start_date: WorldDate,
    pub map_width: u32,
    pub map_height: u32,
    pub countries: usize,
    pub active_countries: usize,
    pub provinces: usize,
    pub country_history_files: usize,
    pub province_history_files: usize,
    pub diplomacy_history_files: usize,
    pub people: usize,
    pub diplomatic_relations: usize,
    pub duplicate_relation_records: usize,
    pub person_sources: BTreeMap<String, BTreeMap<String, LoadedSource>>,
    pub relation_sources: Vec<RelationSource>,
    pub office_sources: BTreeMap<String, LoadedSource>,
    pub expired_people: Vec<String>,
    pub not_yet_born_people: Vec<String>,
    pub unapplied_diplomacy_fields: BTreeMap<String, usize>,
    pub missing_province_history: Vec<u32>,
    pub unmapped_definition_ids: Vec<u32>,
    pub unknown_map_colors: Vec<[u8; 3]>,
    pub unbound_active_capitals: Vec<String>,
    pub ambiguous_country_histories: BTreeMap<String, Vec<PathBuf>>,
    pub unapplied_country_fields: BTreeMap<String, usize>,
    pub unapplied_province_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
    pub economy: EconomyReport,
    pub setup: SetupReport,
    pub attributes: AttributeReport,
    pub administration: AdministrationReport,
    pub governments: GovernmentReport,
    pub ideas: IdeaReport,
    pub estates: EstateReport,
    pub religious_schools: crate::ReligiousSchoolReport,
    pub casus_belli: crate::CasusBelliReport,
    pub missions: crate::MissionReport,
}

#[derive(Debug, Serialize)]
pub struct ProvinceView<'a> {
    #[serde(flatten)]
    pub state: &'a ProvinceState,
    pub metrics: Option<&'a BTreeMap<String, Option<i64>>>,
    pub buildings: Option<Vec<&'a str>>,
    pub trade_node: Option<&'a str>,
    pub trade_node_outgoing: Option<Vec<&'a str>>,
    pub trade_good_definition: Option<&'a augustus_core::TradeGoodDefinition>,
    pub modifiers: Vec<&'a str>,
    pub modifier_attachments_known: bool,
    pub declared_modifier_totals_micros: BTreeMap<String, i64>,
    pub attributes: BTreeMap<String, augustus_core::AttributeEvaluation>,
    pub administration: Option<augustus_core::AdministrationEvaluation>,
}

#[derive(Debug, Serialize)]
pub struct CountryView<'a> {
    pub flags: Option<&'a BTreeMap<String, bool>>,
    pub flag_namespace_known: bool,
    pub estates: Option<&'a BTreeMap<String, augustus_core::EstateState>>,
    pub declared_estate_effects: augustus_core::EstateEvaluation,
    pub ideas: Option<&'a augustus_core::IdeaState>,
    pub declared_idea_effects: augustus_core::IdeaEvaluation,
    pub last_monthly_tax: Option<&'a augustus_core::MonthlyTaxAccount>,
    pub government: Option<&'a augustus_core::GovernmentState>,
    pub declared_government_effects: augustus_core::GovernmentEvaluation,
    pub state: &'a CountryState,
    pub owned_provinces: Vec<u32>,
    pub bound_capital: Option<u32>,
    pub politics: Option<&'a augustus_core::CountryPolitics>,
    pub diplomacy: Vec<RelationView<'a>>,
    pub political_offices: Vec<&'a str>,
    pub technology: Option<&'a BTreeMap<String, u16>>,
    pub resources: Option<&'a augustus_core::CountryResources>,
    pub modifiers: Vec<&'a str>,
    pub modifier_attachments_known: bool,
    pub declared_modifier_totals_micros: BTreeMap<String, i64>,
    pub province_attribute_totals: BTreeMap<String, augustus_core::AttributeAggregate>,
    pub administrative_areas: Option<&'a BTreeMap<String, augustus_core::AreaStatus>>,
}

#[derive(Debug, Serialize)]
pub struct MapEntry {
    pub province: u32,
    pub color: [u8; 3],
    pub display_color: [u8; 3],
}

#[derive(Debug)]
pub struct VanillaWorld {
    pub initial: InitialWorld,
    pub report: BootstrapReport,
    pub province_bitmap: PathBuf,
    pub country_sources: BTreeMap<String, Vec<LoadedSource>>,
    pub province_sources: BTreeMap<u32, Vec<LoadedSource>>,
    content: String,
}

impl VanillaWorld {
    /// Loads the supported initial-state fields and selects calendar/tax rules.
    /// Final-edition rules always target all expansion mechanics, independently of launcher switches.
    pub fn load(path: impl AsRef<Path>, date: &str) -> Result<Self, Error> {
        Self::load_session(path, date, None)
    }

    /// A supplied roster defines human countries; the remaining countries use AI control.
    /// An empty roster explicitly requests an AI-only session.
    pub fn load_with_players(
        path: impl AsRef<Path>,
        date: &str,
        players: &std::collections::BTreeSet<String>,
    ) -> Result<Self, Error> {
        Self::load_session(path, date, Some(players))
    }

    fn load_session(
        path: impl AsRef<Path>,
        date: &str,
        players: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<Self, Error> {
        let root = ContentRoot::open(path)?;
        let date = Date::parse(date).map_err(|e| Error::caused_by(root.path(), e))?;
        let metadata_path = root.path().join("launcher-settings.json");
        let metadata = root.read_text("launcher-settings.json", TextEncoding::Utf8)?;
        let metadata: serde_json::Value = serde_json::from_slice(metadata.bytes())
            .map_err(|e| Error::caused_by(&metadata_path, e))?;
        let version = metadata
            .get("rawVersion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::message(&metadata_path, "missing rawVersion"))?;
        if !matches!(version, "v1.37.5.0" | "1.37.5.0" | "1.37.5") {
            return Err(Error::message(
                &metadata_path,
                "world loader targets EU4 1.37.5 final edition",
            ));
        }
        let map_document = root.read_text("map/default.map", ENCODING)?;
        let map_parsed = map_document.parse()?;
        let map = map_parsed.reader();
        let width = required_u32(&map_parsed, map.clone(), "width")?;
        let height = required_u32(&map_parsed, map.clone(), "height")?;
        let max = required_u32(&map_parsed, map.clone(), "max_provinces")?;
        if width == 0 || height == 0 || max < 2 {
            return Err(Error::message(
                map_document.source(),
                "invalid map dimensions or province limit",
            ));
        }
        let definition_file = map_file(&map_parsed, map.clone(), "definitions")?;
        let bitmap_file = map_file(&map_parsed, map.clone(), "provinces")?;
        // Resolve through the same traversal checks used for text inputs.
        let bitmap = root.read_text(&bitmap_file, ENCODING)?;
        let pixels = image::ImageReader::new(std::io::Cursor::new(bitmap.bytes()))
            .with_guessed_format()
            .map_err(|e| Error::caused_by(bitmap.source(), e))?
            .decode()
            .map_err(|e| Error::caused_by(bitmap.source(), e))?
            .into_rgb8();
        if pixels.width() != width || pixels.height() != height {
            return Err(Error::message(
                bitmap.source(),
                "province bitmap dimensions do not match default.map",
            ));
        }
        let used_colors: HashSet<[u8; 3]> = pixels.pixels().map(|p| p.0).collect();
        let sea = ids(&map_parsed, map.clone(), "sea_starts")?;
        let lakes = ids(&map_parsed, map.clone(), "lakes")?;
        let unused = ids(&map_parsed, map.clone(), "only_used_for_random")?;
        let climate_file = map_file(&map_parsed, map.clone(), "climate")?;
        let climate = root.read_text(climate_file, ENCODING)?;
        let climate_parsed = climate.parse()?;
        let wasteland = ids(&climate_parsed, climate_parsed.reader(), "impassable")?;
        let mut report = BootstrapReport {
            ruleset: "eu4-final-all-dlc",
            game_version: version.into(),
            start_date: WorldDate {
                year: date.year(),
                month: date.month(),
                day: date.day(),
            },
            map_width: width,
            map_height: height,
            countries: 0,
            active_countries: 0,
            provinces: 0,
            country_history_files: 0,
            province_history_files: 0,
            diplomacy_history_files: 0,
            people: 0,
            diplomatic_relations: 0,
            duplicate_relation_records: 0,
            person_sources: BTreeMap::new(),
            relation_sources: Vec::new(),
            office_sources: BTreeMap::new(),
            expired_people: Vec::new(),
            not_yet_born_people: Vec::new(),
            unapplied_diplomacy_fields: BTreeMap::new(),
            missing_province_history: Vec::new(),
            unbound_active_capitals: Vec::new(),
            unmapped_definition_ids: Vec::new(),
            unknown_map_colors: Vec::new(),
            ambiguous_country_histories: BTreeMap::new(),
            unapplied_country_fields: BTreeMap::new(),
            unapplied_province_fields: BTreeMap::new(),
            unapplied_examples: BTreeMap::new(),
            economy: EconomyReport::default(),
            setup: SetupReport::default(),
            attributes: AttributeReport::default(),
            administration: AdministrationReport::default(),
            governments: GovernmentReport::default(),
            ideas: IdeaReport::default(),
            estates: EstateReport::default(),
            religious_schools: crate::ReligiousSchoolReport::default(),
            casus_belli: crate::CasusBelliReport::default(),
            missions: crate::MissionReport::default(),
        };
        let mut provinces = BTreeMap::new();
        let mut province_sources = BTreeMap::new();
        let definitions = root.read_text(&definition_file, ENCODING)?;
        let mut csv = csv::ReaderBuilder::new()
            .delimiter(b';')
            .flexible(true)
            .from_reader(definitions.bytes());
        let header = csv
            .byte_headers()
            .map_err(|e| Error::caused_by(definitions.source(), e))?;
        if header.get(0) != Some(b"province".as_slice()) {
            return Err(Error::message(
                definitions.source(),
                "expected province definition header",
            ));
        }
        for row in csv.byte_records() {
            let row = row.map_err(|e| Error::caused_by(definitions.source(), e))?;
            let line = row.position().map_or(1, |p| p.line() as usize);
            let fail = |m: &str| {
                Error::message(definitions.source(), m)
                    .with_location(SourceLocation { line, column: 1 })
            };
            let integer = |index| -> Result<u32, Error> {
                std::str::from_utf8(
                    row.get(index)
                        .ok_or_else(|| fail("missing definition column"))?,
                )
                .map_err(|_| fail("invalid definition integer"))?
                .trim()
                .parse()
                .map_err(|_| fail("invalid definition integer"))
            };
            let id = integer(0)?;
            if id == 0 || id >= max {
                continue;
            }
            let mut color = [0; 3];
            for (i, component) in color.iter_mut().enumerate() {
                *component =
                    u8::try_from(integer(i + 1)?).map_err(|_| fail("map color out of range"))?;
            }
            let name = ENCODING
                .decode(row.get(4).ok_or_else(|| fail("missing province name"))?)
                .into_owned();
            let class = if unused.contains(&id) || !used_colors.contains(&color) {
                ProvinceClass::Unused
            } else if sea.contains(&id) {
                ProvinceClass::Sea
            } else if lakes.contains(&id) {
                ProvinceClass::Lake
            } else if wasteland.contains(&id) {
                ProvinceClass::Wasteland
            } else {
                ProvinceClass::Land
            };
            let province = ProvinceState {
                id,
                name,
                color,
                class,
                owner: None,
                controller: None,
                cores: Vec::new(),
                culture: None,
                religion: None,
                trade_good: None,
                tax_milli: None,
                production_milli: None,
                manpower_milli: None,
                is_city: None,
                in_empire: None,
            };
            if provinces.insert(id, province).is_some() {
                return Err(fail("duplicate province identifier"));
            }
            province_sources.insert(
                id,
                vec![LoadedSource {
                    path: definitions.source().into(),
                    line,
                    column: 1,
                }],
            );
        }
        for id in sea
            .iter()
            .chain(lakes.iter())
            .chain(unused.iter())
            .chain(wasteland.iter())
        {
            if !provinces.contains_key(id) {
                return Err(Error::message(
                    map_document.source(),
                    format!("map category references unknown province {id}"),
                ));
            }
        }
        let defined_colors: HashSet<_> = provinces.values().map(|p| p.color).collect();
        report.unknown_map_colors = used_colors.difference(&defined_colors).copied().collect();
        report.unknown_map_colors.sort();
        report.unmapped_definition_ids = provinces
            .values()
            .filter(|p| p.class == ProvinceClass::Unused)
            .map(|p| p.id)
            .collect();
        let files = root.files()?;
        let mut countries = BTreeMap::new();
        let mut country_sources = BTreeMap::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/country_tags"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let id = key.read_string();
                let location = parsed.field_location(&key);
                let fail = |m: &str| Error::message(doc.source(), m).with_location(location);
                equal(op, &fail)?;
                if id.len() != 3
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                {
                    return Err(fail("invalid country identifier"));
                }
                let file = value
                    .read_string()
                    .map_err(|_| fail("country definition must be a path"))?;
                if !file.starts_with("countries/") {
                    return Err(fail("expected countries/ definition path"));
                }
                let country_doc = root.read_text(format!("common/{file}"), ENCODING)?;
                let country_parsed = country_doc.parse()?;
                let rgb = array_strings(&country_parsed, country_parsed.reader(), "color")?;
                let color: [u8; 3] = rgb
                    .iter()
                    .map(|v| v.parse::<u8>().map_err(|_| fail("invalid country color")))
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .map_err(|_| fail("country color requires three components"))?;
                let country = CountryState {
                    id: id.clone(),
                    name: Path::new(&file)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&id)
                        .into(),
                    color,
                    declared_capital: None,
                    primary_culture: None,
                    religion: None,
                    government: None,
                    technology_group: None,
                };
                if countries.insert(id.clone(), country).is_some() {
                    return Err(fail("duplicate country identifier"));
                }
                country_sources.insert(
                    id,
                    vec![
                        source(&parsed, location),
                        LoadedSource {
                            path: country_doc.source().into(),
                            line: 1,
                            column: 1,
                        },
                    ],
                );
            }
        }
        if countries.is_empty() || provinces.is_empty() {
            return Err(Error::message(root.path(), "empty world definitions"));
        }
        let mut economy = EconomyDraft::load(&root, &files, &mut report.economy)?;
        let mut governments = GovernmentDraft::load(&root, &files, &mut report.governments)?;
        let mut status_draft = CountryStatusDraft::load(&root, &files, &mut report.ideas)?;
        let mut estates = EstateDraft::load(&root, &files, &mut report.estates)?;
        let mut markers = crate::country_markers::CountryMarkers::default();
        let mut global_markers = augustus_core::GlobalMarkers {
            known: true,
            ..Default::default()
        };
        let mut ideas = IdeaDraft::load(&root, &files, &mut report.ideas)?;
        let mut setup = SetupDraft::load(&root, &files, &mut report.setup)?;
        let attribute_draft = AttributeDraft::load(&root, &files, &mut report.attributes)?;
        let mut administration =
            AdministrationDraft::load(&root, &files, &map_parsed, &mut report.administration)?;
        let mut political_drafts = BTreeMap::new();
        let mut religious_schools = crate::religious_schools::ReligiousSchoolsDraft::load(
            &root,
            &files,
            &mut report.religious_schools,
        )?;
        let mut country_histories = BTreeSet::new();
        let mut province_histories = BTreeSet::new();
        let mut unresolved_ownership = BTreeSet::new();
        let mut history_groups: BTreeMap<String, Vec<&Path>> = BTreeMap::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/countries"))
        {
            history_groups
                .entry(history_id(&file.relative_path)?)
                .or_default()
                .push(&file.relative_path);
        }
        for (id, paths) in &history_groups {
            if paths.len() > 1 {
                report.ambiguous_country_histories.insert(
                    id.clone(),
                    paths.iter().map(|p| root.path().join(p)).collect(),
                );
            }
        }
        let mut metric_scoped_history_unknown = false;
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/countries"))
        {
            let id = history_id(&file.relative_path)?;
            let Some(country) = countries.get_mut(&id) else {
                return Err(Error::message(
                    root.path().join(&file.relative_path),
                    format!("history references unknown country {id}"),
                ));
            };
            // Selection of duplicate history files has not been validated. Preserve the
            // ambiguity and leave those history-derived fields unset instead of guessing.
            if history_groups[&id].len() > 1 {
                global_markers.mark_unknown();
                continue;
            }
            country_histories.insert(id.clone());
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            country_sources
                .get_mut(&id)
                .expect("registered country")
                .push(LoadedSource {
                    path: doc.source().into(),
                    line: 1,
                    column: 1,
                });
            visit_history(&parsed, date, |reader| {
                if crate::country_markers::has_global_mutation(&parsed, reader.clone(), 0)? {
                    global_markers.mark_unknown();
                }
                let (direct, scoped) =
                    crate::province_metrics::history_mutations(reader.clone(), 0);
                metric_scoped_history_unknown |= direct || scoped;
                religious_schools.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.religious_schools,
                )?;
                markers.history(&parsed, reader.clone(), &id)?;
                status_draft.country_history(&parsed, reader.clone(), &id, &mut report.ideas)?;
                estates.country_history(&parsed, reader.clone(), &id, &mut report.estates)?;
                ideas.country_history(&parsed, reader.clone(), &id, &mut report.ideas)?;
                governments.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.governments,
                )?;
                administration.country_history(
                    &parsed,
                    reader.clone(),
                    &id,
                    &mut report.administration,
                )?;
                apply_country(
                    &parsed,
                    reader,
                    country,
                    political_drafts
                        .entry(id.clone())
                        .or_insert_with(CountryDraft::default),
                    &mut setup,
                    &mut report,
                )
            })?;
        }
        let mut metric_history_unknown = BTreeSet::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "history/provinces"))
        {
            let id = history_id(&file.relative_path)?
                .parse::<u32>()
                .map_err(|_| {
                    Error::message(&file.relative_path, "invalid province history identifier")
                })?;
            let Some(province) = provinces.get_mut(&id) else {
                return Err(Error::message(
                    root.path().join(&file.relative_path),
                    format!("history references unknown province {id}"),
                ));
            };
            if !province_histories.insert(id) {
                return Err(Error::message(
                    &file.relative_path,
                    "duplicate province history",
                ));
            }
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            province_sources
                .get_mut(&id)
                .expect("registered province")
                .push(LoadedSource {
                    path: doc.source().into(),
                    line: 1,
                    column: 1,
                });
            visit_history(&parsed, date, |reader| {
                if crate::country_markers::has_global_mutation(&parsed, reader.clone(), 0)? {
                    global_markers.mark_unknown();
                }
                if nested_owner(reader.clone(), 0) {
                    unresolved_ownership.insert(id);
                }
                let (direct, scoped) =
                    crate::province_metrics::history_mutations(reader.clone(), 0);
                metric_scoped_history_unknown |= scoped;
                if direct {
                    metric_history_unknown.insert(id);
                }
                administration.province_history(
                    &parsed,
                    reader.clone(),
                    id,
                    &mut report.administration,
                )?;
                apply_province(
                    &parsed,
                    reader,
                    province,
                    &mut economy,
                    &mut setup,
                    &mut report,
                )
            })?;
        }
        report.country_history_files = country_histories.len();
        report.province_history_files = province_histories.len();
        report.missing_province_history = provinces
            .keys()
            .filter(|id| !province_histories.contains(id))
            .copied()
            .collect();
        let mut initial = InitialWorld::new(
            countries.into_values().collect(),
            provinces.into_values().collect(),
        )
        .map_err(|e| Error::caused_by(root.path(), e))?;
        let unowned = initial
            .provinces()
            .iter()
            .filter(|province| {
                province.owner.is_none()
                    && !unresolved_ownership.contains(&province.id)
                    && (province.class != ProvinceClass::Land
                        || province_histories.contains(&province.id))
            })
            .map(|province| {
                initial
                    .province_entity(province.id)
                    .expect("registered province")
            })
            .collect::<Vec<_>>();
        for entity in unowned {
            initial
                .mark_target_absent(entity, augustus_core::Target::Owner)
                .map_err(|e| Error::caused_by(root.path(), e))?;
        }
        initial
            .set_markers(markers.bind(&initial))
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_global_markers(global_markers)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let mut politics = politics::bind_people(&initial, political_drafts, date, &mut report)?;
        politics::load_diplomacy(&root, &files, &initial, date, &mut politics, &mut report)?;
        initial
            .set_politics(politics)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let economy = economy.bind(&initial, &province_histories, &mut report.economy)?;
        initial
            .set_economy(economy)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        setup.bind(&mut initial, date, &mut report.setup)?;
        let governments = governments.bind(
            &initial,
            &attribute_draft,
            &report.start_date,
            &mut report.governments,
        );
        initial
            .set_governments(governments)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let mut affiliations = administration.affiliations(&initial);
        affiliations.0.insert(
            "religious_school".into(),
            religious_schools.bind(&initial, &mut report.religious_schools),
        );
        initial
            .set_affiliations(affiliations)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let area_scopes = administration.ordered_areas();
        initial
            .set_metrics(crate::province_metrics::bind(
                &initial,
                &attribute_draft,
                report.start_date,
                &metric_history_unknown,
                metric_scoped_history_unknown,
            ))
            .map_err(|e| Error::caused_by("province metrics", e))?;
        let attributes = attribute_draft.bind(&initial, &mut report.attributes, &report.setup)?;
        let administration = administration.bind(
            &initial,
            &province_histories,
            &mut report.administration,
            &attributes,
            &report.attributes,
            date,
        )?;
        initial
            .set_administration(administration)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let politics = status_draft.bind(&initial, report.start_date, &mut report.ideas)?;
        initial
            .set_politics(politics)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let casus_belli =
            crate::CasusBelliTypes::from_root(&root, &files, initial.world().schema())?;
        initial
            .set_justifications(augustus_core::Justifications {
                definitions: casus_belli.definitions(),
                unknown_countries: if report.start_date
                    == (WorldDate {
                        year: 1444,
                        month: 11,
                        day: 11,
                    }) {
                    Default::default()
                } else {
                    initial
                        .countries()
                        .iter()
                        .filter_map(|country| initial.country_entity(&country.id))
                        .collect()
                },
                ..Default::default()
            })
            .map_err(|e| Error::caused_by("common/cb_types", e))?;
        let missions =
            crate::missions::MissionDraft::load(&root, &files, initial.world(), &area_scopes)?;
        let startup = crate::estates::EstateStartup::load(
            &root,
            &files,
            initial.world().schema(),
            initial
                .world()
                .modifiers()
                .definitions
                .keys()
                .cloned()
                .collect(),
            initial
                .world()
                .governments()
                .metrics
                .keys()
                .cloned()
                .collect(),
            initial
                .world()
                .metrics()
                .definitions
                .keys()
                .cloned()
                .collect(),
            area_scopes,
            initial.world().affiliations().0["religious_school"]
                .groups
                .clone(),
            casus_belli.types,
            &mut report.estates,
        )?;
        report.casus_belli = casus_belli.report;
        let estates = estates.bind(&initial, &mut report.estates)?;
        initial
            .set_estates(estates)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        let ideas = ideas.bind(&initial, report.start_date, &mut report.ideas);
        initial
            .set_ideas(ideas)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_attributes(attributes)
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_calendar(Calendar {
                start: report.start_date,
                month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
            })
            .map_err(|e| Error::caused_by(root.path(), e))?;
        initial
            .set_taxation(Taxation::new(
                "tax_after_autonomy",
                initial
                    .countries()
                    .iter()
                    .map(|c| initial.country_entity(&c.id).expect("registered country"))
                    .collect(),
            ))
            .map_err(|e| Error::caused_by(root.path(), e))?;
        if let Some(players) = players {
            for player in players {
                if initial.country_entity(player).is_none() {
                    return Err(Error::message(
                        root.path(),
                        format!("unknown player country {player}"),
                    ));
                }
            }
            let countries: Vec<_> = initial
                .countries()
                .iter()
                .map(|country| country.id.clone())
                .collect();
            for country in countries {
                initial
                    .set_country_controller(
                        initial
                            .country_entity(&country)
                            .expect("registered country"),
                        if players.contains(&country) {
                            augustus_core::CountryController::Human
                        } else {
                            augustus_core::CountryController::Ai
                        },
                    )
                    .map_err(|e| Error::caused_by(root.path(), e))?;
            }
        }
        if initial
            .world()
            .date()
            .map_err(|e| Error::caused_by("variables", e))?
            == Some(augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            })
        {
            initial
                .set_named_values(augustus_core::NamedValues {
                    entities: (0..initial.world().entities().len())
                        .map(|id| (augustus_core::EntityId(id), BTreeMap::new()))
                        .collect(),
                    ..Default::default()
                })
                .map_err(|e| Error::caused_by("variables", e))?;
        }
        report.missions = missions.bind(&mut initial)?;
        startup.run(&mut initial, &mut report.estates)?;
        report.missions.known_countries = initial.world().objectives().entities.len();
        report.missions.unresolved.retain(|tag, _| {
            !initial
                .world()
                .objectives()
                .entities
                .contains_key(&initial.country_entity(tag).expect("registered country"))
        });
        if report.missions.series > 0 && !report.estates.startup_unresolved.is_empty() {
            report.missions.known_countries = 0;
            for country in initial.countries() {
                report
                    .missions
                    .unresolved
                    .insert(country.id.clone(), "startup effects are unresolved".into());
            }
        }
        report.countries = initial.countries().len();
        report.provinces = initial.provinces().len();
        for country in initial.countries() {
            if !initial.owned_provinces(&country.id).is_empty() {
                report.active_countries += 1;
                if initial.bound_capital(&country.id).is_none() {
                    report.unbound_active_capitals.push(country.id.clone());
                }
            }
        }
        // Fingerprint canonical supported content. Paths and diagnostics are
        // excluded, and the initial state is not duplicated inside every save.
        let mut digest = Sha256::new();
        digest.update(
            serde_json::to_vec(&(
                "augustus-vanilla-calendar-tax:1",
                report.ruleset,
                &report.game_version,
                initial.countries(),
                initial.provinces(),
            ))
            .map_err(|e| Error::caused_by(root.path(), e))?,
        );
        digest.update(b"\0");
        digest.update(
            initial
                .world()
                .save("")
                .map_err(|e| Error::caused_by(root.path(), e))?
                .as_bytes(),
        );
        let catalog = initial.world().event_catalog();
        if !catalog.definitions().is_empty() {
            digest.update(b"\0events\0");
            let events = if catalog.once_events().is_empty() {
                serde_json::to_vec(&(catalog.definitions(), catalog.conditions()))
            } else {
                serde_json::to_vec(&(
                    catalog.definitions(),
                    catalog.conditions(),
                    catalog.once_events(),
                ))
            };
            digest.update(events.map_err(|e| Error::caused_by(root.path(), e))?);
            if !catalog.option_conditions().is_empty() {
                digest.update(b"\0option-conditions\0");
                digest.update(
                    serde_json::to_vec(catalog.option_conditions())
                        .map_err(|e| Error::caused_by(root.path(), e))?,
                );
            }
            if !catalog.pictures().is_empty() {
                digest.update(b"\0event-pictures\0");
                digest.update(
                    serde_json::to_vec(catalog.pictures())
                        .map_err(|e| Error::caused_by(root.path(), e))?,
                );
            }
        }
        let fingerprint: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let content = format!("augustus-vanilla-calendar-tax:1:sha256:{fingerprint}");
        Ok(Self {
            initial,
            report,
            province_bitmap: bitmap.source().into(),
            country_sources,
            province_sources,
            content,
        })
    }

    pub fn advance(&mut self, days: u64) -> Result<(), augustus_core::Error> {
        self.initial.advance(days)
    }

    pub fn save(&self) -> Result<String, augustus_core::Error> {
        self.initial.world().save(&self.content)
    }

    pub fn restore(&mut self, snapshot: &str) -> Result<(), augustus_core::Error> {
        self.initial.restore(snapshot, &self.content)
    }

    pub fn province(&self, id: u32) -> Option<ProvinceView<'_>> {
        let state = self.initial.province(id)?;
        let economy = self.initial.world().economy();
        let entity = self.initial.province_entity(id)?;
        let economic_state = economy.provinces.get(&entity);
        Some(ProvinceView {
            state,
            metrics: self.initial.world().metrics().entities.get(&entity),
            administration: self
                .initial
                .world()
                .administration()
                .provinces
                .contains_key(&entity)
                .then(|| {
                    self.initial
                        .world()
                        .administration()
                        .evaluate(self.initial.world(), entity)
                        .expect("validated administration")
                }),
            buildings: economic_state.map(|s| s.buildings.iter().map(String::as_str).collect()),
            trade_node: economy.province_node(entity),
            trade_node_outgoing: economy.province_node(entity).map(|name| {
                economy.trade_nodes[name]
                    .outgoing
                    .iter()
                    .map(String::as_str)
                    .collect()
            }),
            trade_good_definition: economic_state
                .and_then(|s| s.trade_good.as_ref())
                .and_then(|name| economy.trade_goods.get(name)),
            modifiers: self.modifier_names(entity),
            modifier_attachments_known: self
                .initial
                .world()
                .modifier_attachments_known(entity)
                .expect("registered province"),
            attributes: self
                .initial
                .world()
                .attributes()
                .formulas
                .get(&entity)
                .map(|formulas| {
                    formulas
                        .keys()
                        .map(|name| {
                            (
                                name.clone(),
                                self.initial
                                    .world()
                                    .attributes()
                                    .evaluate(self.initial.world(), entity, name)
                                    .expect("validated attributes"),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            declared_modifier_totals_micros: self
                .initial
                .world()
                .modifiers()
                .declared_totals(entity)
                .expect("validated modifier totals"),
        })
    }

    pub fn country(&self, id: &str) -> Option<CountryView<'_>> {
        self.initial.country(id).map(|state| CountryView {
            estates: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().estates().countries.get(&entity)),
            declared_estate_effects: self
                .initial
                .world()
                .estates()
                .declared_effects(self.initial.country_entity(id).expect("registered country"))
                .expect("validated estate effects"),
            ideas: self
                .initial
                .country_entity(id)
                .and_then(|entity| self.initial.world().ideas().countries.get(&entity)),
            declared_idea_effects: self
                .initial
                .world()
                .ideas()
                .declared_effects(self.initial.country_entity(id).expect("registered country"))
                .expect("validated idea effects"),
            last_monthly_tax: self
                .initial
                .world()
                .taxation()
                .and_then(|t| t.last_settlement.as_ref())
                .and_then(|last| {
                    last.accounts
                        .get(&self.initial.country_entity(id).expect("registered country"))
                }),
            government: self
                .initial
                .world()
                .governments()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            declared_government_effects: self
                .initial
                .world()
                .governments()
                .declared_effects(self.initial.country_entity(id).expect("registered country"))
                .expect("validated government effects"),
            administrative_areas: self.initial.country_entity(id).and_then(|entity| {
                self.initial
                    .world()
                    .administration()
                    .country_areas
                    .get(&entity)
            }),
            state,
            owned_provinces: self.initial.owned_provinces(id),
            bound_capital: self.initial.bound_capital(id),
            politics: self
                .initial
                .world()
                .politics()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            diplomacy: self
                .diplomacy()
                .into_iter()
                .filter(|r| r.first == id || r.second == id)
                .collect(),
            political_offices: self
                .initial
                .world()
                .politics()
                .offices
                .iter()
                .filter_map(|(name, holder)| {
                    (self
                        .initial
                        .world()
                        .entity(*holder)
                        .expect("validated holder")
                        .name
                        == id)
                        .then_some(name.as_str())
                })
                .collect(),
            technology: self
                .initial
                .world()
                .technology()
                .countries
                .get(&self.initial.country_entity(id).expect("registered country")),
            province_attribute_totals: ATTRIBUTE_NAMES
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        self.initial
                            .world()
                            .attributes()
                            .sum_owned(
                                self.initial.world(),
                                self.initial.country_entity(id).expect("registered country"),
                                name,
                            )
                            .expect("validated attribute totals"),
                    )
                })
                .collect(),
            resources: self
                .initial
                .world()
                .country_resources()
                .get(&self.initial.country_entity(id).expect("registered country")),
            modifiers: self
                .modifier_names(self.initial.country_entity(id).expect("registered country")),
            modifier_attachments_known: self
                .initial
                .world()
                .modifier_attachments_known(
                    self.initial.country_entity(id).expect("registered country"),
                )
                .expect("registered country"),
            declared_modifier_totals_micros: self
                .initial
                .world()
                .modifiers()
                .declared_totals(self.initial.country_entity(id).expect("registered country"))
                .expect("validated modifier totals"),
            flags: self
                .initial
                .world()
                .markers()
                .entities
                .get(&self.initial.country_entity(id).expect("registered country")),
            flag_namespace_known: self
                .initial
                .world()
                .marker_namespace_known(
                    self.initial.country_entity(id).expect("registered country"),
                )
                .expect("registered country"),
        })
    }

    fn modifier_names(&self, entity: augustus_core::EntityId) -> Vec<&str> {
        self.initial
            .world()
            .modifiers()
            .active
            .get(&entity)
            .map(|active| active.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub fn diplomacy(&self) -> Vec<RelationView<'_>> {
        self.initial
            .world()
            .politics()
            .relations
            .iter()
            .map(|r| politics::relation_view(&self.initial, r))
            .collect()
    }

    pub fn palette(&self) -> Vec<MapEntry> {
        let mut entries: Vec<_> = self
            .initial
            .provinces()
            .iter()
            .filter(|p| p.class != ProvinceClass::Unused)
            .map(|p| {
                let color = match p.class {
                    ProvinceClass::Sea | ProvinceClass::Lake => [32, 64, 96],
                    ProvinceClass::Wasteland | ProvinceClass::Unused => [48, 48, 48],
                    ProvinceClass::Land => p
                        .owner
                        .as_deref()
                        .and_then(|id| self.initial.country(id))
                        .map_or([112, 112, 104], |c| c.color),
                };
                MapEntry {
                    province: p.id,
                    color: p.color,
                    display_color: color,
                }
            })
            .collect();
        entries.sort_by_key(|p| p.color);
        entries
    }
}

pub(super) fn direct_txt(path: &Path, directory: &str) -> bool {
    path.parent() == Some(Path::new(directory))
        && path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("txt"))
}

fn history_id(path: &Path) -> Result<String, Error> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split([' ', '-']).next())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| Error::message(path, "invalid history filename"))
}

pub(super) fn source(parsed: &ParsedText<'_>, location: SourceLocation) -> LoadedSource {
    LoadedSource {
        path: parsed.source().into(),
        line: location.line,
        column: location.column,
    }
}

pub(super) fn equal(op: Option<Operator>, fail: &impl Fn(&str) -> Error) -> Result<(), Error> {
    if op.is_some_and(|o| o != Operator::Equal) {
        Err(fail("expected assignment operator"))
    } else {
        Ok(())
    }
}

fn required_u32(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<u32, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_string()
                .map_err(|_| fail("expected integer"))?
                .parse()
                .map_err(|_| fail("invalid integer"));
        }
    }
    Err(Error::message(parsed.source(), format!("missing {name}")))
}

fn map_file(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<String, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_string()
                .map(|v| format!("map/{v}"))
                .map_err(|_| fail("expected map filename"));
        }
    }
    Err(Error::message(
        parsed.source(),
        format!("missing map {name}"),
    ))
}

fn array_strings(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<Vec<String>, Error> {
    for (key, op, value) in reader.fields() {
        if key.read_string() == name {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            return value
                .read_array()
                .map_err(|_| fail("expected scalar array"))?
                .values()
                .map(|v| {
                    v.read_string()
                        .map_err(|_| fail("expected scalar array entry"))
                })
                .collect();
        }
    }
    Ok(Vec::new())
}

fn ids(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    name: &str,
) -> Result<BTreeSet<u32>, Error> {
    array_strings(parsed, reader, name)?
        .into_iter()
        .map(|v| {
            v.parse().map_err(|_| {
                Error::message(
                    parsed.source(),
                    format!("invalid {name} province identifier"),
                )
            })
        })
        .collect()
}

fn visit_history<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
    mut apply: impl FnMut(ObjectReader<'a, 'a, TextEncoding>) -> Result<(), Error>,
) -> Result<(), Error> {
    for (_, reader) in history_blocks(parsed, date)? {
        apply(reader)?;
    }
    Ok(())
}

type HistoryBlock<'a> = (Option<Date>, ObjectReader<'a, 'a, TextEncoding>);

pub(super) fn history_blocks<'a>(
    parsed: &'a ParsedText<'_>,
    date: Date,
) -> Result<Vec<HistoryBlock<'a>>, Error> {
    let mut blocks = vec![(None, parsed.reader())];
    for (key, op, value) in parsed.reader().fields() {
        let name = key.read_string();
        if dated_name(&name) {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            let timestamp = Date::parse(&name).map_err(|_| fail("invalid history date"))?;
            let block = value
                .read_object()
                .map_err(|_| fail("dated history requires a block"))?;
            if timestamp <= date {
                blocks.push((Some(timestamp), block));
            }
        }
    }
    blocks.sort_by_key(|entry| entry.0);
    Ok(blocks)
}

pub(super) fn dated_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_digit) && name.contains('.')
}

fn unapplied(
    parsed: &ParsedText<'_>,
    name: String,
    location: SourceLocation,
    country: bool,
    report: &mut BootstrapReport,
) {
    let fields = if country {
        &mut report.unapplied_country_fields
    } else {
        &mut report.unapplied_province_fields
    };
    *fields.entry(name.clone()).or_default() += 1;
    let key = format!("{}:{name}", if country { "country" } else { "province" });
    report
        .unapplied_examples
        .entry(key)
        .or_insert_with(|| source(parsed, location));
}

fn apply_country(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    country: &mut CountryState,
    politics: &mut CountryDraft,
    setup: &mut SetupDraft,
    report: &mut BootstrapReport,
) -> Result<(), Error> {
    let handled = setup.country_history(parsed, reader.clone(), &country.id, &mut report.setup)?;
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if dated_name(&name) || handled.contains(&name) {
            continue;
        }
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        match name.as_str() {
            "legitimacy"
            | "religious_school"
            | "add_government_reform"
            | "remove_government_reform"
            | "revolution_target" => (),
            "monarch" | "heir" | "queen" | "monarch_consort" => {
                equal(op, &fail)?;
                let person = politics::person(parsed, value, &name, location, report)?;
                *match name.as_str() {
                    "monarch" => &mut politics.ruler,
                    "heir" => &mut politics.heir,
                    _ => &mut politics.consort,
                } = Some(person);
            }
            "capital" => {
                equal(op, &fail)?;
                let id: u32 = value
                    .read_string()
                    .map_err(|_| fail("expected capital identifier"))?
                    .parse()
                    .map_err(|_| fail("invalid capital identifier"))?;
                country.declared_capital = (id != 0).then_some(id);
            }
            "primary_culture" | "religion" | "government" | "technology_group" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected country history scalar"))?;
                let slot = match name.as_str() {
                    "primary_culture" => &mut country.primary_culture,
                    "religion" => &mut country.religion,
                    "government" => &mut country.government,
                    _ => &mut country.technology_group,
                };
                *slot = Some(text);
            }
            _ => unapplied(parsed, name, location, true, report),
        }
    }
    Ok(())
}

// Dated blocks are visited separately. An unhandled conditional owner assignment
// cannot establish that this province is unowned.
fn nested_owner(reader: ObjectReader<'_, '_, TextEncoding>, depth: usize) -> bool {
    if depth > 64 {
        return true;
    }
    reader.fields().any(|(key, _, value)| {
        let name = key.read_string();
        !dated_name(&name)
            && value.read_object().is_ok_and(|block| {
                block
                    .fields()
                    .any(|(key, _, _)| key.read_string() == "owner")
                    || nested_owner(block, depth + 1)
            })
    })
}

fn apply_province(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    province: &mut ProvinceState,
    economy: &mut EconomyDraft,
    setup: &mut SetupDraft,
    report: &mut BootstrapReport,
) -> Result<(), Error> {
    let handled = setup.province_history(parsed, reader.clone(), province.id, &mut report.setup)?;
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if dated_name(&name) || handled.contains(&name) {
            continue;
        }
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        if economy.is_building(&name) {
            equal(op, &fail)?;
            economy.apply_building(
                parsed,
                &name,
                value,
                province.id,
                location,
                &mut report.economy,
            )?;
            continue;
        }
        match name.as_str() {
            "owner" | "controller" | "culture" | "religion" | "trade_goods" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected province history scalar"))?;
                if name == "trade_goods"
                    && text != "---"
                    && !text.is_empty()
                    && !economy.has_trade_good(&text)
                {
                    return Err(fail(&format!(
                        "province {} references unknown trade good {text}",
                        province.id
                    )));
                }
                let slot = match name.as_str() {
                    "owner" => &mut province.owner,
                    "controller" => &mut province.controller,
                    "culture" => &mut province.culture,
                    "religion" => &mut province.religion,
                    _ => &mut province.trade_good,
                };
                *slot = (text != "---" && !text.is_empty()).then_some(text);
            }
            "add_core" | "remove_core" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected core country"))?;
                if name == "add_core" {
                    if !province.cores.contains(&text) {
                        province.cores.push(text);
                    }
                } else {
                    province.cores.retain(|c| c != &text);
                }
            }
            "add_local_autonomy" => (), // Applied and diagnosed by AdministrationDraft.
            "base_tax" | "base_production" | "base_manpower" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected literal base value"))?;
                let number = nonnegative_milli(&text).ok_or_else(|| {
                    fail("expected nonnegative base value with at most three decimal digits")
                })?;
                *match name.as_str() {
                    "base_tax" => &mut province.tax_milli,
                    "base_production" => &mut province.production_milli,
                    _ => &mut province.manpower_milli,
                } = Some(number);
            }
            "is_city" | "hre" => {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected yes or no"))?;
                let flag = match text.as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(fail("expected yes or no")),
                };
                *if name == "is_city" {
                    &mut province.is_city
                } else {
                    &mut province.in_empire
                } = Some(flag);
            }
            _ => unapplied(parsed, name, location, false, report),
        }
    }
    Ok(())
}

pub(super) fn nonnegative_milli(text: &str) -> Option<i64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    whole
        .parse::<i64>()
        .ok()?
        .checked_mul(1000)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32)
        })
}

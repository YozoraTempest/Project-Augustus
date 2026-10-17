// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Translate the state portion of a plaintext EU4 save into the external
//! compatibility boundary. The simulation core never reads EU4 save syntax.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use augustus_core::{CountryResources, NationalFocus, RulerSkill};
use jomini::{TextToken, text::ValueReader};
use serde::Serialize;

use crate::administration_validation::plaintext_save_document;
use crate::bootstrap::{
    VanillaExternalAdvisor, VanillaExternalArmy, VanillaExternalNavalUnit, VanillaExternalState,
    VanillaWorld,
};
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = jomini::text::ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

/// State translated from a legal, uncompressed EU4 text save.
///
/// The parsed state is deliberately kept separate from [`VanillaWorld`]. A
/// caller can inspect it, filter it against a loaded content set, and only
/// then bind it before startup effects run.
#[derive(Debug, Clone, Serialize)]
pub struct VanillaSaveState {
    pub date: String,
    pub player: Option<String>,
    pub players: BTreeSet<String>,
    pub external_state: VanillaExternalState,
}

impl VanillaSaveState {
    /// Translate the save into a state bundle accepted by a loaded content set.
    ///
    /// Fields whose identifiers are not defined by the target content are
    /// omitted, matching the filtering used by direct EU4 save import.
    pub fn state_for_world(&self, world: &VanillaWorld) -> Result<VanillaExternalState, Error> {
        self.for_world(world).map(|(_, state)| state)
    }

    /// Keep only identifiers present in the loaded world and bind the content
    /// declared monarch-power ceiling to the saved power stocks.
    pub(crate) fn for_world(
        &self,
        world: &VanillaWorld,
    ) -> Result<(BTreeSet<String>, VanillaExternalState), Error> {
        let mut state = VanillaExternalState::default();
        let countries: BTreeSet<_> = world
            .initial
            .countries()
            .iter()
            .map(|country| country.id.as_str())
            .collect();
        let definitions = &world.initial.world().metrics().definitions;

        for country in &countries {
            if let Some(values) = self.external_state.country_metric_values.get(*country) {
                let filtered = values
                    .iter()
                    .filter(|(name, _)| definitions.contains_key(name.as_str()))
                    .map(|(name, value)| (name.clone(), *value))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state
                        .country_metric_values
                        .insert((*country).into(), filtered);
                }
            }
            if let Some(resources) = self.external_state.country_resources.get(*country) {
                state
                    .country_resources
                    .insert((*country).into(), resources.clone());
            }
            if let Some(focus) = self.external_state.country_national_focus.get(*country) {
                state
                    .country_national_focus
                    .insert((*country).into(), *focus);
            }
            if let Some(advisors) = self.external_state.advisors.get(*country) {
                state.advisors.insert((*country).into(), advisors.clone());
            }
            if let Some(markers) = self.external_state.country_markers.get(*country) {
                state
                    .country_markers
                    .insert((*country).into(), markers.clone());
            }
            if let Some(estates) = self.external_state.estate_privileges.get(*country) {
                let entity = world.initial.country_entity(country).ok_or_else(|| {
                    Error::message("save state", "save country is missing from the world")
                })?;
                let available = world
                    .initial
                    .world()
                    .estates()
                    .countries
                    .get(&entity)
                    .ok_or_else(|| {
                        Error::message("save state", "save country has no estate state")
                    })?;
                let filtered = estates
                    .iter()
                    .filter(|(estate, _)| available.contains_key(*estate))
                    .map(|(estate, privileges)| (estate.clone(), privileges.clone()))
                    .collect::<BTreeMap<_, _>>();
                if !filtered.is_empty() {
                    state.estate_privileges.insert((*country).into(), filtered);
                }
            }
        }

        for army in &self.external_state.armies {
            if countries.contains(army.owner.as_str())
                && world.initial.province_entity(army.location).is_some()
            {
                state.armies.push(army.clone());
            }
        }

        for naval_unit in &self.external_state.naval_units {
            if !countries.contains(naval_unit.owner.as_str()) {
                continue;
            }
            let Some(entity) = world.initial.province_entity(naval_unit.location) else {
                continue;
            };
            if world
                .initial
                .world()
                .province_has_port(entity)
                .is_ok_and(|has_port| has_port)
            {
                state.naval_units.push(naval_unit.clone());
            }
        }

        if let Some(ceiling) = world.report.setup.monarch_power_base_maximum {
            for country in &countries {
                let Some(values) = state.country_metric_values.get(*country) else {
                    continue;
                };
                let ceilings = ["adm_power", "dip_power", "mil_power"]
                    .into_iter()
                    .filter(|name| values.contains_key(*name) && definitions.contains_key(*name))
                    .map(|name| (name.into(), Some(ceiling)))
                    .collect::<BTreeMap<_, _>>();
                if !ceilings.is_empty() {
                    state
                        .country_metric_ceilings
                        .insert((*country).into(), ceilings);
                }
            }
        }

        for (province, value) in &self.external_state.province_raw_autonomy_milli {
            let Some(entity) = world.initial.province_entity(*province) else {
                continue;
            };
            if world
                .initial
                .world()
                .administration()
                .provinces
                .contains_key(&entity)
            {
                state.province_raw_autonomy_milli.insert(*province, *value);
            }
        }

        let players = self
            .players
            .iter()
            .filter(|tag| countries.contains(tag.as_str()))
            .cloned()
            .collect();
        Ok((players, state))
    }
}

/// Read a plaintext `EU4txt` save without modifying it.
pub fn read_vanilla_save_state(path: impl AsRef<Path>) -> Result<VanillaSaveState, Error> {
    let document = plaintext_save_document(path.as_ref())?;
    let parsed = document.parse()?;
    let root = parsed.reader();
    let date = scalar(&parsed, &required(&parsed, &root, "date")?)?;
    let current_date =
        date_key(&date).ok_or_else(|| Error::message(parsed.source(), "invalid save date"))?;
    let player = optional_scalar(&parsed, &root, "player")?;
    let mut players = BTreeSet::new();
    if let Some(player) = &player
        && is_country_tag(player)
    {
        players.insert(player.clone());
    }

    let mut external_state = VanillaExternalState::default();
    let countries = object(&parsed, &required(&parsed, &root, "countries")?)?;
    for (key, op, value) in countries.fields() {
        let fail = || Error::message(parsed.source(), "invalid saved country block");
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(fail());
        }
        let tag = key.read_string();
        if !is_country_tag(&tag) {
            continue;
        }
        let country = object(&parsed, &value)?;
        let resources = CountryResources {
            treasury_milli: optional_milli(&parsed, &country, "treasury")?,
            manpower_milli: optional_milli(&parsed, &country, "manpower")?,
            mercantilism_milli: optional_milli(&parsed, &country, "mercantilism")?,
        };
        external_state
            .country_resources
            .insert(tag.clone(), resources);

        if let Some(value) = optional_scalar(&parsed, &country, "national_focus")? {
            let focus = match value.as_str() {
                "ADM" => NationalFocus {
                    skill: Some(RulerSkill::Administration),
                },
                "DIP" => NationalFocus {
                    skill: Some(RulerSkill::Diplomacy),
                },
                "MIL" => NationalFocus {
                    skill: Some(RulerSkill::Military),
                },
                "none" | "NONE" => NationalFocus { skill: None },
                _ => {
                    return Err(Error::message(
                        parsed.source(),
                        format!("country {tag} has unsupported national focus {value}"),
                    ));
                }
            };
            external_state
                .country_national_focus
                .insert(tag.clone(), focus);
        }

        let mut metrics = BTreeMap::new();
        if let Some(value) = optional_milli(&parsed, &country, "stability")? {
            if value % 1000 != 0 {
                return Err(Error::message(
                    parsed.source(),
                    format!("country {tag} has fractional stability"),
                ));
            }
            metrics.insert("stability".into(), Some(value / 1000));
        }
        if let Some(value) = optional_milli(&parsed, &country, "current_power_projection")? {
            metrics.insert("power_projection".into(), Some(value));
        }
        for (field, metric) in [
            ("prestige", "prestige"),
            ("legitimacy", "legitimacy"),
            ("republican_tradition", "republican_tradition"),
            ("devotion", "devotion"),
        ] {
            if let Some(value) = optional_milli(&parsed, &country, field)? {
                metrics.insert(metric.into(), Some(value));
            }
        }
        if let Some(values) = last_power_vector(&parsed, &country)? {
            metrics.insert("adm_power".into(), Some(values[0]));
            metrics.insert("dip_power".into(), Some(values[1]));
            metrics.insert("mil_power".into(), Some(values[2]));
        }
        if !metrics.is_empty() {
            external_state
                .country_metric_values
                .insert(tag.clone(), metrics);
        }

        let mut markers = BTreeSet::new();
        if let Some(flags) = optional_object(&parsed, &country, "flags")? {
            for (flag, op, _) in flags.fields() {
                if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        "invalid saved country flag",
                    ));
                }
                markers.insert(flag.read_string());
            }
        }
        if optional_object(&parsed, &country, "flags")?.is_some() {
            external_state
                .country_markers
                .insert(tag.clone(), Some(markers));
        }

        let mut estates = BTreeMap::new();
        for (estate_key, estate_op, estate_value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "estate")
        {
            if estate_op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved estate block",
                ));
            }
            let estate = object(&parsed, &estate_value)?;
            let Some(estate_name) = optional_scalar(&parsed, &estate, "type")? else {
                continue;
            };
            let privileges = optional_field(&parsed, &estate, "granted_privileges")?
                .map(|value| privilege_names(&value))
                .unwrap_or_default();
            estates.insert(estate_name, Some(privileges));
            let _ = estate_key;
        }
        if !estates.is_empty() {
            external_state
                .estate_privileges
                .insert(tag.clone(), estates);
        }

        for (key, op, value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "army")
        {
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(parsed.source(), "invalid saved army block"));
            }
            let (location, name, regiments) = saved_army(&parsed, &value)?;
            external_state.armies.push(VanillaExternalArmy {
                owner: tag.clone(),
                location,
                name,
                regiments,
            });
            let _ = key;
        }

        for (key, op, value) in country
            .fields()
            .filter(|(key, _, _)| key.read_string() == "navy")
        {
            if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
                return Err(Error::message(parsed.source(), "invalid saved navy block"));
            }
            let (location, ships) = saved_navy(&parsed, &value)?;
            for unit_type in ships {
                external_state.naval_units.push(VanillaExternalNavalUnit {
                    owner: tag.clone(),
                    location,
                    unit_type,
                });
            }
            let _ = key;
        }

        if optional_scalar(&parsed, &country, "human")?.as_deref() == Some("yes") {
            players.insert(tag);
        }
    }

    for tag in external_state
        .country_resources
        .keys()
        .cloned()
        .collect::<Vec<_>>()
    {
        external_state.advisors.entry(tag).or_default();
    }

    let provinces = object(&parsed, &required(&parsed, &root, "provinces")?)?;
    for (key, op, value) in provinces.fields() {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved province block",
            ));
        }
        let Some(id) = key
            .read_string()
            .strip_prefix('-')
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|id| *id > 0)
        else {
            return Err(Error::message(
                parsed.source(),
                "expected negative save province identifier",
            ));
        };
        let province = object(&parsed, &value)?;
        if let Some(owner) =
            optional_scalar(&parsed, &province, "owner")?.filter(|owner| is_country_tag(owner))
        {
            let advisors = external_state.advisors.entry(owner).or_default();
            collect_saved_advisors(&parsed, &province, current_date, advisors, 0)?;
        }
        let autonomy = optional_milli(&parsed, &province, "local_autonomy")?.unwrap_or(0);
        if !(0..=100_000).contains(&autonomy) {
            return Err(Error::message(
                parsed.source(),
                "saved autonomy is outside 0..100",
            ));
        }
        external_state
            .province_raw_autonomy_milli
            .insert(id, Some(autonomy));
    }

    Ok(VanillaSaveState {
        date,
        player,
        players,
        external_state,
    })
}

fn is_country_tag(value: &str) -> bool {
    value.len() == 3
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn date_key(value: &str) -> Option<(i32, u8, u8)> {
    let mut parts = value.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn collect_saved_advisors(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    current_date: (i32, u8, u8),
    output: &mut Vec<VanillaExternalAdvisor>,
    depth: usize,
) -> Result<(), Error> {
    if depth > 12 {
        return Ok(());
    }
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved advisor field {name}"),
            ));
        }
        if name == "advisor" {
            let advisor = object(parsed, &value)?;
            let Some(hire_date) = optional_scalar(parsed, &advisor, "hire_date")? else {
                continue;
            };
            let Some(hire_date) = date_key(&hire_date) else {
                return Err(Error::message(
                    parsed.source(),
                    "invalid saved advisor hire date",
                ));
            };
            if hire_date > current_date {
                continue;
            }
            if let Some(death_date) = optional_scalar(parsed, &advisor, "death_date")? {
                let Some(death_date) = date_key(&death_date) else {
                    return Err(Error::message(
                        parsed.source(),
                        "invalid saved advisor death date",
                    ));
                };
                if death_date <= current_date {
                    continue;
                }
            }
            let type_name = optional_scalar(parsed, &advisor, "type")?.ok_or_else(|| {
                Error::message(parsed.source(), "saved advisor is missing its type")
            })?;
            let skill = optional_scalar(parsed, &advisor, "skill")?
                .ok_or_else(|| {
                    Error::message(parsed.source(), "saved advisor is missing its skill")
                })?
                .parse::<u8>()
                .map_err(|error| Error::caused_by(parsed.source(), error))?;
            let id = optional_object(parsed, &advisor, "id")?
                .and_then(|id| optional_scalar(parsed, &id, "id").ok().flatten())
                .and_then(|id| id.parse::<u64>().ok());
            let candidate = VanillaExternalAdvisor {
                type_name,
                skill,
                id,
            };
            if candidate.id.is_none() || !output.iter().any(|existing| existing.id == candidate.id)
            {
                output.push(candidate);
            }
        } else if let Ok(nested) = value.read_object() {
            collect_saved_advisors(parsed, &nested, current_date, output, depth + 1)?;
        }
    }
    Ok(())
}

fn saved_army(
    parsed: &ParsedText<'_>,
    value: &Field<'_, '_>,
) -> Result<(u32, String, Vec<String>), Error> {
    let army = object(parsed, value)?;
    let mut location = None;
    let mut name = String::new();
    let mut regiments = Vec::new();
    for (key, op, value) in army.fields() {
        let field = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved army field {field}"),
            ));
        }
        match field.as_str() {
            "location" => {
                let text = scalar(parsed, &value)?;
                let id = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if id == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved army location must be positive",
                    ));
                }
                location = Some(id);
            }
            "name" => name = scalar(parsed, &value)?,
            "regiment" => {
                let regiment = object(parsed, &value)?;
                let unit_type = optional_scalar(parsed, &regiment, "type")?.ok_or_else(|| {
                    Error::message(parsed.source(), "saved regiment is missing its type")
                })?;
                if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::message(
                        parsed.source(),
                        "saved regiment has an invalid type",
                    ));
                }
                regiments.push(unit_type);
            }
            _ => {}
        }
    }
    let location = location
        .ok_or_else(|| Error::message(parsed.source(), "saved army is missing its location"))?;
    if regiments.is_empty() {
        return Err(Error::message(
            parsed.source(),
            "saved army has no regiments",
        ));
    }
    Ok((location, name, regiments))
}

fn saved_navy(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<(u32, Vec<String>), Error> {
    let navy = object(parsed, value)?;
    let mut location = None;
    let mut ships = Vec::new();
    for (key, op, value) in navy.fields() {
        let field = key.read_string();
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved navy field {field}"),
            ));
        }
        match field.as_str() {
            "location" => {
                let text = scalar(parsed, &value)?;
                let id = text
                    .parse::<u32>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))?;
                if id == 0 {
                    return Err(Error::message(
                        parsed.source(),
                        "saved navy location must be positive",
                    ));
                }
                location = Some(id);
            }
            "ship" => {
                let ship = object(parsed, &value)?;
                let unit_type = optional_scalar(parsed, &ship, "type")?.ok_or_else(|| {
                    Error::message(parsed.source(), "saved ship is missing its type")
                })?;
                if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
                {
                    return Err(Error::message(
                        parsed.source(),
                        "saved ship has an invalid type",
                    ));
                }
                ships.push(unit_type);
            }
            _ => {}
        }
    }
    let location = location
        .ok_or_else(|| Error::message(parsed.source(), "saved navy is missing its location"))?;
    if ships.is_empty() {
        return Err(Error::message(parsed.source(), "saved navy has no ships"));
    }
    Ok((location, ships))
}

fn optional_field<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Field<'a, 'b>>, Error> {
    let mut result = None;
    for (_key, op, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved field {name}"),
            ));
        }
        result = Some(value);
    }
    Ok(result)
}

fn required<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Field<'a, 'b>, Error> {
    optional_field(parsed, reader, name)?
        .ok_or_else(|| Error::message(parsed.source(), format!("missing save field {name}")))
}

fn optional_scalar(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<String>, Error> {
    optional_field(parsed, reader, name)?
        .map(|value| scalar(parsed, &value))
        .transpose()
}

fn scalar(parsed: &ParsedText<'_>, value: &Field<'_, '_>) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn object<'a, 'b>(parsed: &ParsedText<'_>, value: &Field<'a, 'b>) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. })
        && !(matches!(value.token(), TextToken::Array { mixed: false, .. })
            && value
                .read_array()
                .is_ok_and(|array| array.values().next().is_none()))
    {
        return Err(Error::message(parsed.source(), "expected save object"));
    }
    value
        .read_object()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn optional_object<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Object<'a, 'b>>, Error> {
    optional_field(parsed, reader, name)?
        .map(|value| object(parsed, &value))
        .transpose()
}

fn optional_milli(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<i64>, Error> {
    optional_scalar(parsed, reader, name)?
        .map(|value| {
            signed_milli(&value).ok_or_else(|| {
                Error::message(
                    parsed.source(),
                    format!("invalid saved decimal field {name}"),
                )
            })
        })
        .transpose()
}

fn signed_milli(value: &str) -> Option<i64> {
    let (negative, unsigned) = match value.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude =
        whole
            .parse::<i64>()
            .ok()?
            .checked_mul(1000)?
            .checked_add(if fraction.is_empty() {
                0
            } else {
                fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32)
            })?;
    if negative {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
    }
}

fn last_power_vector(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
) -> Result<Option<[i64; 3]>, Error> {
    let mut result = None;
    for (_key, op, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == "powers")
    {
        if op.is_some_and(|operator| operator != jomini::text::Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid saved powers field",
            ));
        }
        let array = value
            .read_array()
            .map_err(|error| Error::caused_by(parsed.source(), error))?;
        let values = array
            .values()
            .map(|value| {
                scalar(parsed, &value)?
                    .parse::<i64>()
                    .map_err(|error| Error::caused_by(parsed.source(), error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 3 {
            return Err(Error::message(
                parsed.source(),
                "saved powers requires three values",
            ));
        }
        result = Some([values[0], values[1], values[2]]);
    }
    Ok(result)
}

fn privilege_names(value: &Field<'_, '_>) -> BTreeSet<String> {
    fn collect(value: &Field<'_, '_>, depth: usize, result: &mut BTreeSet<String>) {
        if depth > 4 {
            return;
        }
        if let Ok(text) = value.read_string() {
            if !text.is_empty() && !text.contains('.') && text.parse::<i64>().is_err() {
                result.insert(text);
            }
            return;
        }
        if let Ok(array) = value.read_array() {
            for item in array.values() {
                collect(&item, depth + 1, result);
            }
            return;
        }
        if let Ok(object) = value.read_object() {
            for (key, _, item) in object.fields() {
                let name = key.read_string();
                if !name.is_empty() && name != "type" && name.parse::<i64>().is_err() {
                    result.insert(name);
                }
                collect(&item, depth + 1, result);
            }
        }
    }

    let mut result = BTreeSet::new();
    collect(value, 0, &mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::{NationalFocus, RulerSkill, read_vanilla_save_state};

    #[test]
    fn reads_startup_state_from_plaintext_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.eu4");
        std::fs::write(
            &path,
            br#"EU4txt
date=1444.11.11
player="AAA"
countries={
	AAA={
		national_focus=DIP
		treasury=59.802
		manpower=9.754
		mercantilism=10.000
		stability=0.000
		current_power_projection=50.000
		prestige=5.125
		legitimacy=100.000
		powers={
			0 0 0
		}
		powers={
			98 98 42
		}
		human=yes
		flags={
			sample_flag=1444.11.11
		}
		estate={
			type="estate_nobles"
			territory=40.000
			granted_privileges={
				{
					estate_nobles_habsburg_disunity 1444.11.11
				}
			}
		}
		army={
			name="Main Army"
			location=1
			regiment={
				type="infantry"
			}
		}
		navy={
			name="Main Navy"
			location=1
			ship={
				type="galley"
			}
		}
	}
}
provinces={
	-1={
		owner="AAA"
		local_autonomy=3.500
		history={
			advisor={
				type=treasurer
				skill=1
				hire_date=1444.11.11
				id={
					id=7
					type=51
				}
			}
		}
	}
}
"#,
        )
        .unwrap();

        let state = read_vanilla_save_state(&path).unwrap();
        assert_eq!(state.date, "1444.11.11");
        assert_eq!(state.player.as_deref(), Some("AAA"));
        assert!(state.players.contains("AAA"));
        assert_eq!(
            state.external_state.country_resources["AAA"].treasury_milli,
            Some(59_802)
        );
        assert_eq!(
            state.external_state.country_resources["AAA"].manpower_milli,
            Some(9_754)
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["adm_power"],
            Some(98)
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["power_projection"],
            Some(50_000)
        );
        assert_eq!(
            state.external_state.country_metric_values["AAA"]["prestige"],
            Some(5_125)
        );
        assert_eq!(
            state.external_state.country_national_focus["AAA"],
            NationalFocus {
                skill: Some(RulerSkill::Diplomacy)
            }
        );
        assert_eq!(state.external_state.armies.len(), 1);
        assert_eq!(state.external_state.armies[0].owner, "AAA");
        assert_eq!(state.external_state.armies[0].location, 1);
        assert_eq!(state.external_state.armies[0].name, "Main Army");
        assert_eq!(
            state.external_state.armies[0].regiments,
            vec!["infantry".to_owned()]
        );
        assert_eq!(state.external_state.naval_units.len(), 1);
        assert_eq!(state.external_state.naval_units[0].owner, "AAA");
        assert_eq!(state.external_state.naval_units[0].location, 1);
        assert_eq!(state.external_state.naval_units[0].unit_type, "galley");
        assert_eq!(state.external_state.advisors["AAA"].len(), 1);
        assert_eq!(
            state.external_state.advisors["AAA"][0].type_name,
            "treasurer"
        );
        assert_eq!(state.external_state.advisors["AAA"][0].skill, 1);
        assert_eq!(state.external_state.advisors["AAA"][0].id, Some(7));
        assert!(
            state.external_state.country_markers["AAA"]
                .as_ref()
                .unwrap()
                .contains("sample_flag")
        );
        assert_eq!(
            state.external_state.province_raw_autonomy_milli[&1],
            Some(3_500)
        );
        assert_eq!(
            state.external_state.estate_privileges["AAA"]["estate_nobles"]
                .as_ref()
                .unwrap(),
            &std::collections::BTreeSet::from(["estate_nobles_habsburg_disunity".into()])
        );
    }
}

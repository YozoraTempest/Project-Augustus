// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Administration, Affiliations, Attributes, CountryResources, Economy, EntityId, EntityKind,
    Error, FieldId, Modifiers, Politics, Schema, Technology, World,
};

/// A date label. Calendar arithmetic is defined by the selected Calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct WorldDate {
    pub year: i16,
    pub month: u8,
    pub day: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvinceClass {
    Land,
    Sea,
    Lake,
    Wasteland,
    Unused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryState {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    pub declared_capital: Option<u32>,
    pub primary_culture: Option<String>,
    pub religion: Option<String>,
    pub government: Option<String>,
    pub technology_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvinceState {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub class: ProvinceClass,
    pub owner: Option<String>,
    pub controller: Option<String>,
    pub cores: Vec<String>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    pub tax_milli: Option<i64>,
    pub production_milli: Option<i64>,
    pub manpower_milli: Option<i64>,
    pub is_city: Option<bool>,
    pub in_empire: Option<bool>,
}

/// Standard world data backed by the same simulation state used by native systems.
/// Missing history values remain explicit in the public records.
#[derive(Debug, Clone)]
pub struct InitialWorld {
    world: World,
    countries: Vec<CountryState>,
    provinces: Vec<ProvinceState>,
    country_index: BTreeMap<String, usize>,
    province_index: BTreeMap<u32, usize>,
}

impl InitialWorld {
    pub fn set_metrics(&mut self, metrics: crate::Metrics) -> Result<(), Error> {
        self.world.set_metrics(metrics)
    }
    pub fn new(
        mut countries: Vec<CountryState>,
        mut provinces: Vec<ProvinceState>,
    ) -> Result<Self, Error> {
        countries.sort_by(|a, b| a.id.cmp(&b.id));
        provinces.sort_by_key(|p| p.id);
        let country_index: BTreeMap<_, _> = countries
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();
        let province_index: BTreeMap<_, _> = provinces
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id, i))
            .collect();
        if country_index.len() != countries.len()
            || province_index.len() != provinces.len()
            || countries.iter().any(|c| c.id.is_empty())
            || provinces.iter().any(|p| p.id == 0)
        {
            return Err(Error::Invalid("empty or duplicate world identifier".into()));
        }
        let mut colors = BTreeSet::new();
        for province in &provinces {
            if province.class != ProvinceClass::Unused && !colors.insert(province.color) {
                return Err(Error::Invalid(format!(
                    "duplicate map color for province {}",
                    province.id
                )));
            }
            for tag in province
                .owner
                .iter()
                .chain(province.controller.iter())
                .chain(province.cores.iter())
            {
                if !country_index.contains_key(tag) {
                    return Err(Error::Invalid(format!(
                        "province {} references unknown country {tag}",
                        province.id
                    )));
                }
            }
            if [
                province.tax_milli,
                province.production_milli,
                province.manpower_milli,
            ]
            .into_iter()
            .flatten()
            .any(|v| v < 0)
            {
                return Err(Error::Invalid(format!(
                    "negative province base value: {}",
                    province.id
                )));
            }
        }
        let schema = Schema::new([
            "province.tax_base",
            "province.production_base",
            "province.manpower_base",
        ])?;
        let mut world = World::new(schema);
        for country in &countries {
            world.add_entity(&country.id, EntityKind::Country);
            if let Some(capital) = country.declared_capital
                && !province_index.contains_key(&capital)
            {
                return Err(Error::Invalid(format!(
                    "country {} references unknown capital {capital}",
                    country.id
                )));
            }
        }
        for province in &provinces {
            let entity = world.add_entity(province.id.to_string(), EntityKind::Province);
            for (index, value) in [
                province.tax_milli,
                province.production_milli,
                province.manpower_milli,
            ]
            .into_iter()
            .enumerate()
            {
                if let Some(value) = value {
                    world.set(entity, FieldId(index), value)?;
                }
            }
            if let Some(owner) = &province.owner {
                world.set_owner(entity, EntityId(country_index[owner]))?;
            }
        }
        for (index, country) in countries.iter().enumerate() {
            if let Some(capital) = country.declared_capital {
                let province = province_index[&capital];
                if provinces[province].owner.as_ref() == Some(&country.id) {
                    world.set_capital(EntityId(index), EntityId(countries.len() + province))?;
                }
            }
        }
        Ok(Self {
            world,
            countries,
            provinces,
            country_index,
            province_index,
        })
    }

    pub fn set_politics(&mut self, politics: Politics) -> Result<(), Error> {
        self.world.set_politics(politics)
    }

    pub fn set_subjects(&mut self, subjects: crate::Subjects) -> Result<(), Error> {
        self.world.set_subjects(subjects)
    }

    pub fn mark_target_absent(
        &mut self,
        current: EntityId,
        target: crate::Target,
    ) -> Result<(), Error> {
        self.world.mark_target_absent(current, target)
    }

    pub fn set_country_controller(
        &mut self,
        country: EntityId,
        controller: crate::CountryController,
    ) -> Result<(), Error> {
        self.world.set_country_controller(country, controller)
    }

    pub fn set_economy(&mut self, economy: Economy) -> Result<(), Error> {
        self.world.set_economy(economy)
    }

    pub fn set_attributes(&mut self, attributes: Attributes) -> Result<(), Error> {
        self.world.set_attributes(attributes)
    }

    pub fn set_affiliations(&mut self, affiliations: Affiliations) -> Result<(), Error> {
        self.world.set_affiliations(affiliations)
    }

    pub fn set_estates(&mut self, estates: crate::Estates) -> Result<(), Error> {
        self.world.set_estates(estates)
    }

    pub fn set_ideas(&mut self, ideas: crate::Ideas) -> Result<(), Error> {
        self.world.set_ideas(ideas)
    }

    pub fn set_governments(&mut self, governments: crate::Governments) -> Result<(), Error> {
        self.world.set_governments(governments)
    }

    pub fn set_administration(&mut self, administration: Administration) -> Result<(), Error> {
        self.world.set_administration(administration)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn execute(&mut self, current: EntityId, plan: &[crate::Action]) -> Result<(), Error> {
        self.world.execute(current, plan)
    }

    pub fn set_event_catalog(&mut self, catalog: crate::EventCatalog) -> Result<(), Error> {
        self.world.set_event_catalog(catalog)
    }

    pub fn set_objectives(&mut self, objectives: crate::Objectives) -> Result<(), Error> {
        self.world.set_objectives(objectives)
    }

    pub fn refresh_objectives(&mut self, entity: EntityId) -> Result<(), Error> {
        self.world.refresh_objectives(entity)
    }

    pub fn set_completions(&mut self, completions: crate::Completions) -> Result<(), Error> {
        self.world.set_completions(completions)
    }

    pub fn set_named_values(&mut self, values: crate::NamedValues) -> Result<(), Error> {
        self.world.set_named_values(values)
    }

    pub fn set_markers(&mut self, markers: crate::Markers) -> Result<(), Error> {
        self.world.set_markers(markers)
    }

    pub fn set_global_markers(&mut self, markers: crate::GlobalMarkers) -> Result<(), Error> {
        self.world.set_global_markers(markers)
    }

    pub fn choose_event(&mut self, choice: &crate::EventChoice) -> Result<(), Error> {
        self.world.choose_event(choice)
    }

    pub fn set_calendar(&mut self, calendar: crate::Calendar) -> Result<(), Error> {
        self.world.set_calendar(calendar)
    }

    pub fn set_taxation(&mut self, taxation: crate::Taxation) -> Result<(), Error> {
        self.world.set_taxation(taxation)
    }

    pub fn advance(&mut self, days: u64) -> Result<(), Error> {
        self.world
            .advance_to(self.world.day().checked_add(days).ok_or(Error::Overflow)?)
    }

    pub fn restore(&mut self, snapshot: &str, content: &str) -> Result<(), Error> {
        let restored = World::restore_with_events(
            snapshot,
            content,
            self.world.schema(),
            Vec::new(),
            self.world.event_catalog().clone(),
        )?;
        if restored.entities().len() != self.world.entities().len()
            || restored.completions().identifiers != self.world.completions().identifiers
            || restored.objectives().groups != self.world.objectives().groups
            || restored.justifications().definitions != self.world.justifications().definitions
            || restored.calendar() != self.world.calendar()
            || restored.monthly_system_id() != self.world.monthly_system_id()
            || restored
                .taxation()
                .map(|t| (&t.annual_attribute, &t.countries))
                != self
                    .world
                    .taxation()
                    .map(|t| (&t.annual_attribute, &t.countries))
            || restored
                .entities()
                .iter()
                .zip(self.world.entities())
                .any(|(a, b)| a.name != b.name || a.kind != b.kind)
        {
            return Err(Error::Invalid(
                "saved entities do not match the loaded world".into(),
            ));
        }
        self.world = restored;
        Ok(())
    }

    pub fn set_technology(&mut self, technology: Technology) -> Result<(), Error> {
        self.world.set_technology(technology)
    }
    pub fn set_modifiers(&mut self, modifiers: Modifiers) -> Result<(), Error> {
        self.world.set_modifiers(modifiers)
    }
    pub fn set_justifications(&mut self, state: crate::Justifications) -> Result<(), Error> {
        self.world.set_justifications(state)
    }
    pub fn set_country_resources(
        &mut self,
        resources: BTreeMap<EntityId, CountryResources>,
    ) -> Result<(), Error> {
        self.world.set_country_resources(resources)
    }
    pub fn countries(&self) -> &[CountryState] {
        &self.countries
    }
    pub fn provinces(&self) -> &[ProvinceState] {
        &self.provinces
    }
    pub fn country(&self, id: &str) -> Option<&CountryState> {
        self.country_index.get(id).map(|&i| &self.countries[i])
    }
    pub fn province(&self, id: u32) -> Option<&ProvinceState> {
        self.province_index.get(&id).map(|&i| &self.provinces[i])
    }
    pub fn country_entity(&self, id: &str) -> Option<EntityId> {
        self.country_index.get(id).copied().map(EntityId)
    }
    pub fn province_entity(&self, id: u32) -> Option<EntityId> {
        self.province_index
            .get(&id)
            .map(|&i| EntityId(self.countries.len() + i))
    }
    pub fn owned_provinces(&self, id: &str) -> Vec<u32> {
        self.provinces
            .iter()
            .filter(|p| p.owner.as_deref() == Some(id))
            .map(|p| p.id)
            .collect()
    }
    pub fn bound_capital(&self, id: &str) -> Option<u32> {
        let entity = self.country_entity(id)?;
        let capital = self.world.entity(entity).ok()?.capital?;
        self.provinces
            .get(capital.0.checked_sub(self.countries.len())?)
            .map(|p| p.id)
    }
}

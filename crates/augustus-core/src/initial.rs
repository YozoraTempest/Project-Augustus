// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    CountryResources, Economy, EntityId, EntityKind, Error, FieldId, Modifiers, Politics, Schema,
    Technology, World,
};

/// A calendar label supplied by the content adapter; no calendar arithmetic is implied.
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
#[derive(Debug)]
pub struct InitialWorld {
    world: World,
    countries: Vec<CountryState>,
    provinces: Vec<ProvinceState>,
    country_index: BTreeMap<String, usize>,
    province_index: BTreeMap<u32, usize>,
}

impl InitialWorld {
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

    pub fn set_economy(&mut self, economy: Economy) -> Result<(), Error> {
        self.world.set_economy(economy)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn set_technology(&mut self, technology: Technology) -> Result<(), Error> {
        self.world.set_technology(technology)
    }
    pub fn set_modifiers(&mut self, modifiers: Modifiers) -> Result<(), Error> {
        self.world.set_modifiers(modifiers)
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

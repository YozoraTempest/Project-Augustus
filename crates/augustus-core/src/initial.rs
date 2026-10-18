// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    Administration, Affiliations, Attributes, CountryLedger, CountryResources, Economy, EntityId,
    EntityKind, Error, FieldId, Modifiers, Politics, Schema, Technology, World,
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
    #[serde(default)]
    pub unit_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvinceState {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub class: ProvinceClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_port: Option<bool>,
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
    /// Retain the input's display order while deriving live cores from World.
    province_core_order: Arc<Vec<Vec<String>>>,
    projected_administration: Arc<Administration>,
    projected_ownership: Arc<crate::ownership::OwnershipIndex>,
    country_index: BTreeMap<String, usize>,
    province_index: BTreeMap<u32, usize>,
}

impl InitialWorld {
    pub fn set_metrics(&mut self, metrics: crate::Metrics) -> Result<(), Error> {
        self.world.set_metrics(metrics)
    }

    /// Applies externally resolved metric values atomically while preserving
    /// all definitions and values that the provider did not mention.
    pub fn set_metric_values(
        &mut self,
        values: BTreeMap<EntityId, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), Error> {
        let mut metrics = self.world.metrics().clone();
        for (&entity, entries) in &values {
            let kind = self.world.entity(entity)?.kind;
            for (name, &value) in entries {
                let definition = metrics
                    .definitions
                    .get(name)
                    .ok_or_else(|| Error::Invalid(format!("unknown metric {name}")))?;
                if definition.kind != kind {
                    return Err(Error::Invalid("metric scope does not match entity".into()));
                }
                definition.validate_value(value)?;
                metrics
                    .entities
                    .entry(entity)
                    .or_default()
                    .insert(name.clone(), value);
            }
        }
        self.world.set_metrics(metrics)
    }

    /// Applies externally resolved growth ceilings atomically while preserving
    /// metric values and definitions that the provider did not mention.
    pub fn set_metric_ceilings(
        &mut self,
        ceilings: BTreeMap<EntityId, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), Error> {
        let mut metrics = self.world.metrics().clone();
        for (&entity, entries) in &ceilings {
            let kind = self.world.entity(entity)?.kind;
            for (name, &value) in entries {
                let definition = metrics
                    .definitions
                    .get(name)
                    .ok_or_else(|| Error::Invalid(format!("unknown metric {name}")))?;
                if !definition.growth_ceiling || definition.kind != kind {
                    return Err(Error::Invalid(
                        "metric growth ceiling scope does not match entity".into(),
                    ));
                }
                definition.validate_value(value)?;
                metrics
                    .ceilings
                    .entry(entity)
                    .or_default()
                    .insert(name.clone(), value);
            }
        }
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
            if let Some(has_port) = province.has_port {
                world.set_province_has_port(entity, has_port)?;
            }
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
            projected_administration: Arc::clone(&world.administration),
            projected_ownership: Arc::clone(&world.ownership),
            world,
            countries,
            province_core_order: Arc::new(provinces.iter().map(|p| p.cores.clone()).collect()),
            provinces,
            country_index,
            province_index,
        })
    }

    pub fn new_with_eu4_seed(
        countries: Vec<CountryState>,
        provinces: Vec<ProvinceState>,
        seed: u32,
    ) -> Result<Self, Error> {
        let mut initial = Self::new(countries, provinces)?;
        initial.world.eu4_random = Some(crate::rng::Eu4Rng::from_seed(seed));
        initial.world.eu4_event_seed = Some(seed);
        Ok(initial)
    }

    pub fn set_eu4_event_seed(&mut self, seed: Option<u32>) {
        self.world.set_eu4_event_seed(seed);
    }

    pub fn set_politics(&mut self, politics: Politics) -> Result<(), Error> {
        self.world.set_politics(politics)
    }

    pub fn set_advisors(&mut self, advisors: crate::Advisors) -> Result<(), Error> {
        self.world.set_advisors(advisors)
    }

    pub fn set_truces(&mut self, truces: crate::Truces) -> Result<(), Error> {
        self.world.set_truces(truces)
    }

    pub fn set_subjects(&mut self, subjects: crate::Subjects) -> Result<(), Error> {
        self.world.set_subjects(subjects)
    }

    pub fn set_subject_country_order(&mut self, order: Vec<EntityId>) -> Result<(), Error> {
        self.world.set_subject_country_order(order)
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

    pub fn set_government_rank(
        &mut self,
        country: EntityId,
        rank: Option<u8>,
    ) -> Result<(), Error> {
        self.world.set_government_rank(country, rank)
    }

    pub fn set_administration(&mut self, administration: Administration) -> Result<(), Error> {
        let result = self.world.set_administration(administration);
        self.refresh_province_relations();
        result
    }

    pub fn update_monthly_autonomy(&mut self) -> Result<bool, Error> {
        self.world.update_monthly_autonomy()
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn content_identity_state(&self) -> Result<String, Error> {
        self.world.content_identity_state()
    }

    pub fn execute(&mut self, current: EntityId, plan: &[crate::Action]) -> Result<(), Error> {
        let result = self.world.execute(current, plan);
        self.refresh_province_relations();
        result
    }

    pub fn execute_in_context(
        &mut self,
        context: crate::ScopeContext,
        plan: &[crate::Action],
    ) -> Result<(), Error> {
        let result = self.world.execute_in_context(context, plan);
        self.refresh_province_relations();
        result
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
        let result = self.world.choose_event(choice);
        self.refresh_province_relations();
        result
    }

    pub fn set_calendar(&mut self, calendar: crate::Calendar) -> Result<(), Error> {
        self.world.set_calendar(calendar)
    }

    pub fn select_monthly_system(
        &mut self,
        system: &dyn crate::MonthlySystem,
    ) -> Result<(), Error> {
        self.world.select_monthly_system(system)
    }

    pub fn monthly_system_id(&self) -> Option<&str> {
        self.world.monthly_system_id()
    }

    pub fn select_daily_system(&mut self, system: &dyn crate::DailySystem) -> Result<(), Error> {
        self.world.select_daily_system(system)
    }

    pub fn daily_system_id(&self) -> Option<&str> {
        self.world.daily_system_id()
    }

    pub fn select_yearly_system(&mut self, system: &dyn crate::YearlySystem) -> Result<(), Error> {
        self.world.select_yearly_system(system)
    }

    pub fn yearly_system_id(&self) -> Option<&str> {
        self.world.yearly_system_id()
    }

    pub fn set_taxation(&mut self, taxation: crate::Taxation) -> Result<(), Error> {
        self.world.set_taxation(taxation)
    }

    pub fn set_monthly_accruals(&mut self, accruals: crate::MonthlyAccruals) -> Result<(), Error> {
        self.world.set_monthly_accruals(accruals)
    }

    pub fn set_monthly_production(
        &mut self,
        production: crate::MonthlyProduction,
    ) -> Result<(), Error> {
        self.world.set_monthly_production(production)
    }

    pub fn set_monthly_production_inputs(
        &mut self,
        inputs: BTreeMap<EntityId, crate::VanillaProductionInputState>,
    ) -> Result<(), Error> {
        self.world.set_monthly_production_inputs(inputs)
    }

    pub fn set_monthly_cashflows(
        &mut self,
        cashflows: Option<crate::MonthlyExternalCashflows>,
    ) -> Result<(), Error> {
        self.world.set_monthly_cashflows(cashflows)
    }

    pub fn set_monthly_cashflow_inputs(
        &mut self,
        amounts_milli: BTreeMap<EntityId, Option<i64>>,
    ) -> Result<(), Error> {
        self.world.set_monthly_cashflow_inputs(amounts_milli)
    }

    pub fn set_monthly_cashflow_breakdowns(
        &mut self,
        breakdowns: BTreeMap<EntityId, crate::CashflowBreakdown>,
    ) -> Result<(), Error> {
        self.world.set_monthly_cashflow_breakdowns(breakdowns)
    }

    pub fn monthly_cashflows(&self) -> Option<&crate::MonthlyExternalCashflows> {
        self.world.monthly_cashflows()
    }

    pub fn monthly_production(&self) -> Option<&crate::MonthlyProduction> {
        self.world.monthly_production()
    }

    pub fn set_monthly_stocks(
        &mut self,
        stocks: Option<crate::MonthlyStocks>,
    ) -> Result<(), Error> {
        self.world.set_monthly_stocks(stocks)
    }

    pub fn set_monthly_convergence(
        &mut self,
        convergence: Option<crate::MonthlyConvergence>,
    ) -> Result<(), Error> {
        self.world.set_monthly_convergence(convergence)
    }

    pub fn advance(&mut self, days: u64) -> Result<(), Error> {
        let day = self.world.day().checked_add(days).ok_or(Error::Overflow)?;
        let result = self.world.advance_to(day);
        self.refresh_province_relations();
        result
    }

    pub fn advance_with_daily_system(
        &mut self,
        days: u64,
        system: &dyn crate::DailySystem,
    ) -> Result<(), Error> {
        let day = self.world.day().checked_add(days).ok_or(Error::Overflow)?;
        let result = self.world.advance_with_daily_system(day, system);
        self.refresh_province_relations();
        result
    }

    pub fn advance_with_yearly_system(
        &mut self,
        days: u64,
        system: &dyn crate::YearlySystem,
    ) -> Result<(), Error> {
        let day = self.world.day().checked_add(days).ok_or(Error::Overflow)?;
        let result = self.world.advance_with_yearly_system(day, system);
        self.refresh_province_relations();
        result
    }

    pub fn advance_with_systems(
        &mut self,
        days: u64,
        daily: Option<&dyn crate::DailySystem>,
        monthly: Option<&dyn crate::MonthlySystem>,
    ) -> Result<(), Error> {
        let day = self.world.day().checked_add(days).ok_or(Error::Overflow)?;
        let result = self.world.advance_with_systems(day, daily, monthly);
        self.refresh_province_relations();
        result
    }

    pub fn advance_with_systems_and_yearly(
        &mut self,
        days: u64,
        daily: Option<&dyn crate::DailySystem>,
        monthly: Option<&dyn crate::MonthlySystem>,
        yearly: Option<&dyn crate::YearlySystem>,
    ) -> Result<(), Error> {
        let day = self.world.day().checked_add(days).ok_or(Error::Overflow)?;
        let result = self
            .world
            .advance_with_systems_and_yearly(day, daily, monthly, yearly);
        self.refresh_province_relations();
        result
    }

    pub fn process_event_commands(&mut self) -> Result<(), Error> {
        let result = self.world.process_event_commands();
        self.refresh_province_relations();
        result
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
            || restored.daily_system_id() != self.world.daily_system_id()
            || restored.yearly_system_id() != self.world.yearly_system_id()
            || restored.monthly_accruals().map(|a| &a.rules)
                != self.world.monthly_accruals().map(|a| &a.rules)
            || restored.monthly_production().map(|p| &p.inputs)
                != self.world.monthly_production().map(|p| &p.inputs)
            || restored.monthly_cashflows().map(|c| &c.amounts_milli)
                != self.world.monthly_cashflows().map(|c| &c.amounts_milli)
            || restored.monthly_cashflows().map(|c| &c.breakdowns)
                != self.world.monthly_cashflows().map(|c| &c.breakdowns)
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
        self.refresh_province_relations();
        Ok(())
    }

    fn refresh_province_relations(&mut self) {
        let cores_changed =
            !Arc::ptr_eq(&self.projected_administration, &self.world.administration);
        let owners_changed = !Arc::ptr_eq(&self.projected_ownership, &self.world.ownership);
        if !cores_changed && !owners_changed {
            return;
        }
        for (index, province) in self.provinces.iter_mut().enumerate() {
            let entity = EntityId(self.countries.len() + index);
            if owners_changed {
                province.owner = self.world.entities()[entity.0]
                    .owner
                    .map(|id| self.countries[id.0].id.clone());
            }
            if !cores_changed {
                continue;
            }
            let Some(state) = self
                .world
                .administration()
                .provinces
                .get(&entity)
                .filter(|state| state.cores_known)
            else {
                province.cores.clone_from(&self.province_core_order[index]);
                continue;
            };
            province.cores = self.province_core_order[index]
                .iter()
                .filter(|tag| {
                    state
                        .cores
                        .contains_key(&EntityId(self.country_index[*tag]))
                })
                .cloned()
                .collect();
            for id in state.cores.keys() {
                let tag = &self.countries[id.0].id;
                if !province.cores.contains(tag) {
                    province.cores.push(tag.clone());
                }
            }
        }
        self.projected_administration = Arc::clone(&self.world.administration);
        self.projected_ownership = Arc::clone(&self.world.ownership);
    }

    pub fn set_technology(&mut self, technology: Technology) -> Result<(), Error> {
        self.world.set_technology(technology)
    }
    pub fn set_unit_templates(&mut self, templates: crate::UnitTemplates) -> Result<(), Error> {
        self.world.set_unit_templates(templates)
    }
    pub fn set_selected_unit(
        &mut self,
        country: EntityId,
        category: &str,
        unit_type: &str,
    ) -> Result<(), Error> {
        self.world.set_selected_unit(country, category, unit_type)
    }
    pub fn clear_selected_unit(&mut self, country: EntityId, category: &str) -> Result<(), Error> {
        self.world.clear_selected_unit(country, category)
    }
    pub fn create_army(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
    ) -> Result<crate::ArmyId, Error> {
        self.world.create_army(owner, location, name, unit_types)
    }
    pub fn armies(&self) -> &[crate::Army] {
        self.world.armies()
    }
    pub fn create_army_from_categories(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        categories: &[String],
    ) -> Result<Option<crate::ArmyId>, Error> {
        self.world
            .create_army_from_categories(owner, location, name, categories)
    }
    pub fn naval_units(&self) -> &[crate::NavalUnit] {
        self.world.naval_units()
    }
    pub fn create_naval_unit(
        &mut self,
        owner: EntityId,
        location: EntityId,
        unit_type: &str,
    ) -> Result<crate::NavalUnitId, Error> {
        self.world.create_naval_unit(owner, location, unit_type)
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

    pub fn set_country_ledgers(
        &mut self,
        ledgers: BTreeMap<EntityId, CountryLedger>,
    ) -> Result<(), Error> {
        self.world.set_country_ledgers(ledgers)
    }

    pub fn consolidate_country_ledgers_monthly(
        &mut self,
        expense_filtered_categories: &BTreeMap<EntityId, BTreeSet<u32>>,
    ) -> Result<(), Error> {
        self.world
            .consolidate_country_ledgers_monthly(expense_filtered_categories)
    }

    pub fn rollover_country_ledgers_annual(&mut self) -> Result<(), Error> {
        self.world.rollover_country_ledgers_annual()
    }

    pub fn flush_country_ledgers_pending(&mut self) -> Result<BTreeMap<EntityId, i64>, Error> {
        self.world.flush_country_ledgers_pending()
    }

    pub fn set_estate_loyalty_targets(
        &mut self,
        targets: BTreeMap<EntityId, BTreeMap<String, Option<i64>>>,
    ) -> Result<(), Error> {
        let mut estates = self.world.estates().clone();
        for (&country, entries) in &targets {
            for (estate, &target) in entries {
                let state = estates
                    .countries
                    .get_mut(&country)
                    .and_then(|states| states.get_mut(estate))
                    .ok_or_else(|| Error::Invalid("unknown estate loyalty target".into()))?;
                state.loyalty_target_milli = target;
            }
        }
        self.world.set_estates(estates)
    }
    pub fn countries(&self) -> &[CountryState] {
        &self.countries
    }
    pub fn provinces(&self) -> &[ProvinceState] {
        &self.provinces
    }

    pub fn set_province_is_city(&mut self, id: u32, is_city: Option<bool>) -> Result<(), Error> {
        let index = self
            .province_index
            .get(&id)
            .copied()
            .ok_or_else(|| Error::Invalid(format!("unknown province {id}")))?;
        self.provinces[index].is_city = is_city;
        Ok(())
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

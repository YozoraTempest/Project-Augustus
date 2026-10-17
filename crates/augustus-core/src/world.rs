// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use serde::{Deserialize, Serialize};

use crate::rng::{Eu4Rng, RandomSource};
use crate::{
    Action, Administration, Advisors, Affiliations, Attributes, Calendar, CountryResources,
    DailySystem, Economy, Error, Governments, Ideas, Modifiers, MonthlySystem, MonthlyTax, Plan,
    Politics, Target, Taxation, Technology, WorldDate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Country,
    Province,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    fields: Vec<String>,
}

impl Schema {
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Result<Self, Error> {
        let fields: Vec<String> = names.into_iter().map(Into::into).collect();
        let mut unique = std::collections::BTreeSet::new();
        if fields
            .iter()
            .any(|name| name.is_empty() || !unique.insert(name))
        {
            return Err(Error::Invalid("empty or duplicate state field".into()));
        }
        Ok(Self { fields })
    }

    pub fn field(&self, name: &str) -> Option<FieldId> {
        self.fields
            .iter()
            .position(|field| field == name)
            .map(FieldId)
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub kind: EntityKind,
    pub owner: Option<EntityId>,
    pub capital: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub province_has_port: Option<bool>,
    values: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Task {
    day: u64,
    sequence: u64,
    current: EntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<EntityId>,
    plan: Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    schema: Schema,
    entities: Vec<Entity>,
    pub(crate) armies: crate::army::Armies,
    pub(crate) naval_units: crate::naval::NavalUnits,
    pub(crate) ownership: Arc<crate::ownership::OwnershipIndex>,
    pub(crate) entity_names: BTreeMap<String, Vec<EntityId>>,
    day: u64,
    next_sequence: u64,
    tasks: BTreeMap<(u64, u64), Task>,
    events: Vec<(u64, EntityId, String)>,
    pub(crate) event_catalog: crate::EventCatalog,
    pub(crate) event_queue: crate::event_choices::EventQueue,
    pub(crate) politics: Politics,
    pub(crate) advisors: Advisors,
    pub(crate) truces: crate::Truces,
    pub(crate) truce_expirations: crate::truces::TruceExpirations,
    pub(crate) subjects: crate::Subjects,
    pub(crate) subject_index: crate::subjects::SubjectIndex,
    pub(crate) subject_country_order: Vec<EntityId>,
    pub(crate) country_controllers: BTreeMap<EntityId, crate::CountryController>,
    pub(crate) global_scopes: BTreeMap<String, EntityId>,
    pub(crate) known_absent_targets: BTreeMap<EntityId, BTreeSet<Target>>,
    economy: Economy,
    technology: Technology,
    pub(crate) unit_selections: crate::unit_selections::UnitSelections,
    unit_templates: crate::UnitTemplates,
    pub(crate) modifiers: Modifiers,
    pub(crate) modifier_expirations: BTreeMap<u64, BTreeSet<(EntityId, String)>>,
    pub(crate) justifications: crate::Justifications,
    pub(crate) country_resources: BTreeMap<EntityId, CountryResources>,
    calendar: Option<Calendar>,
    pub(crate) taxation: Option<Taxation>,
    pub(crate) monthly_accruals: Option<crate::MonthlyAccruals>,
    pub(crate) monthly_convergence: Option<crate::MonthlyConvergence>,
    pub(crate) monthly_stocks: Option<crate::MonthlyStocks>,
    pub(crate) monthly_production: Option<crate::MonthlyProduction>,
    pub(crate) monthly_cashflows: Option<crate::MonthlyExternalCashflows>,
    pub(crate) monthly_system_id: Option<String>,
    pub(crate) daily_system_id: Option<String>,
    attributes: Attributes,
    pub(crate) administration: Arc<Administration>,
    pub(crate) affiliations: Affiliations,
    pub(crate) school_opinions: crate::SchoolOpinions,
    pub(crate) governments: Governments,
    pub(crate) metrics: crate::Metrics,
    ideas: Ideas,
    pub(crate) estates: crate::Estates,
    pub(crate) completions: crate::Completions,
    pub(crate) objectives: crate::Objectives,
    pub(crate) named_values: crate::NamedValues,
    pub(crate) markers: crate::Markers,
    pub(crate) global_markers: crate::GlobalMarkers,
    pub(crate) failed: bool,
    pub(crate) rules: Vec<Plan>,
    pub(crate) random: ChaCha8Rng,
    pub(crate) eu4_random: Option<Eu4Rng>,
    pub(crate) eu4_event_seed: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: u32,
    content: String,
    schema: Schema,
    entities: Vec<Entity>,
    #[serde(default, skip_serializing_if = "crate::army::Armies::is_empty")]
    armies: crate::army::Armies,
    #[serde(default, skip_serializing_if = "crate::naval::NavalUnits::is_empty")]
    naval_units: crate::naval::NavalUnits,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    owned_province_order: BTreeMap<EntityId, Vec<EntityId>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subject_country_order: Vec<EntityId>,
    day: u64,
    next_sequence: u64,
    tasks: Vec<Task>,
    events: Vec<(u64, EntityId, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    random: Option<ChaCha8Rng>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    eu4_random: Option<Eu4Rng>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    eu4_event_seed: Option<u32>,
    #[serde(default, skip_serializing_if = "Politics::is_empty")]
    politics: Politics,
    #[serde(default, skip_serializing_if = "Advisors::is_empty")]
    advisors: Advisors,
    #[serde(default, skip_serializing_if = "crate::Truces::is_empty")]
    truces: crate::Truces,
    #[serde(default, skip_serializing_if = "crate::Subjects::is_empty")]
    subjects: crate::Subjects,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    country_controllers: BTreeMap<EntityId, crate::CountryController>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    global_scopes: BTreeMap<String, EntityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    known_absent_targets: BTreeMap<EntityId, BTreeSet<Target>>,
    #[serde(default, skip_serializing_if = "Economy::is_empty")]
    economy: Economy,
    #[serde(default, skip_serializing_if = "Technology::is_empty")]
    technology: Technology,
    #[serde(
        default,
        skip_serializing_if = "crate::unit_selections::UnitSelections::is_empty"
    )]
    unit_selections: crate::unit_selections::UnitSelections,
    #[serde(default, skip_serializing_if = "crate::UnitTemplates::is_empty")]
    unit_templates: crate::UnitTemplates,
    #[serde(default, skip_serializing_if = "Modifiers::is_empty")]
    modifiers: Modifiers,
    #[serde(default, skip_serializing_if = "crate::Justifications::is_empty")]
    justifications: crate::Justifications,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    country_resources: BTreeMap<EntityId, CountryResources>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    calendar: Option<Calendar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    taxation: Option<Taxation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_accruals: Option<crate::MonthlyAccruals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_convergence: Option<crate::MonthlyConvergence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_stocks: Option<crate::MonthlyStocks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_production: Option<crate::MonthlyProduction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_cashflows: Option<crate::MonthlyExternalCashflows>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly_system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    daily_system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Attributes::is_empty")]
    attributes: Attributes,
    #[serde(default, skip_serializing_if = "Administration::is_empty")]
    administration: Administration,
    #[serde(default, skip_serializing_if = "Affiliations::is_empty")]
    affiliations: Affiliations,
    #[serde(default, skip_serializing_if = "crate::SchoolOpinions::is_empty")]
    school_opinions: crate::SchoolOpinions,
    #[serde(default, skip_serializing_if = "Governments::is_empty")]
    governments: Governments,
    #[serde(default, skip_serializing_if = "crate::Metrics::is_empty")]
    metrics: crate::Metrics,
    #[serde(default, skip_serializing_if = "Ideas::is_empty")]
    ideas: Ideas,
    #[serde(default, skip_serializing_if = "crate::Estates::is_empty")]
    estates: crate::Estates,
    #[serde(default, skip_serializing_if = "crate::Completions::is_empty")]
    completions: crate::Completions,
    #[serde(default, skip_serializing_if = "crate::Objectives::is_empty")]
    objectives: crate::Objectives,
    #[serde(default, skip_serializing_if = "crate::NamedValues::is_empty")]
    named_values: crate::NamedValues,
    #[serde(default, skip_serializing_if = "crate::Markers::is_empty")]
    markers: crate::Markers,
    #[serde(default, skip_serializing_if = "crate::GlobalMarkers::is_empty")]
    global_markers: crate::GlobalMarkers,
    #[serde(
        default,
        skip_serializing_if = "crate::event_choices::EventQueue::is_empty"
    )]
    event_queue: crate::event_choices::EventQueue,
}

impl World {
    pub fn new(schema: Schema) -> Self {
        Self::with_rules(schema, Vec::new())
    }

    /// Rules are immutable session content; saves identify the complete content set.
    pub fn with_rules(schema: Schema, rules: Vec<Plan>) -> Self {
        Self::seeded(schema, rules, [0; 32])
    }

    pub fn seeded(schema: Schema, rules: Vec<Plan>, seed: [u8; 32]) -> Self {
        Self {
            schema,
            entities: Vec::new(),
            armies: crate::army::Armies::default(),
            naval_units: crate::naval::NavalUnits::default(),
            ownership: Arc::default(),
            entity_names: BTreeMap::new(),
            day: 0,
            next_sequence: 0,
            tasks: BTreeMap::new(),
            events: Vec::new(),
            event_catalog: crate::EventCatalog::default(),
            event_queue: crate::event_choices::EventQueue::default(),
            politics: Politics::default(),
            advisors: Advisors::default(),
            truces: crate::Truces::default(),
            truce_expirations: Default::default(),
            subjects: crate::Subjects::default(),
            subject_index: crate::subjects::SubjectIndex::default(),
            subject_country_order: Vec::new(),
            country_controllers: BTreeMap::new(),
            global_scopes: BTreeMap::new(),
            known_absent_targets: BTreeMap::new(),
            economy: Economy::default(),
            technology: Technology::default(),
            unit_selections: crate::unit_selections::UnitSelections::default(),
            unit_templates: crate::UnitTemplates::default(),
            modifiers: Modifiers::default(),
            modifier_expirations: BTreeMap::new(),
            justifications: crate::Justifications::default(),
            country_resources: BTreeMap::new(),
            calendar: None,
            taxation: None,
            monthly_accruals: None,
            monthly_convergence: None,
            monthly_stocks: None,
            monthly_production: None,
            monthly_cashflows: None,
            monthly_system_id: None,
            daily_system_id: None,
            attributes: Attributes::default(),
            administration: Arc::default(),
            affiliations: Affiliations::default(),
            school_opinions: crate::SchoolOpinions::default(),
            governments: Governments::default(),
            metrics: crate::Metrics::default(),
            ideas: Ideas::default(),
            estates: crate::Estates::default(),
            completions: crate::Completions::default(),
            objectives: crate::Objectives::default(),
            named_values: crate::NamedValues::default(),
            markers: crate::Markers::default(),
            global_markers: crate::GlobalMarkers::default(),
            failed: false,
            rules,
            random: ChaCha8Rng::from_seed(seed),
            eu4_random: None,
            eu4_event_seed: None,
        }
    }

    /// Creates a world whose random stream follows the EU4 seed and state rules.
    pub fn eu4_seeded(schema: Schema, rules: Vec<Plan>, seed: u32) -> Self {
        Self::eu4_seeded_from_saved_count(schema, rules, seed, 0)
    }

    /// Reconstructs an EU4 random stream from its saved seed and draw count.
    pub fn eu4_seeded_from_saved_count(
        schema: Schema,
        rules: Vec<Plan>,
        seed: u32,
        count: u32,
    ) -> Self {
        let mut world = Self::seeded(schema, rules, [0; 32]);
        world.eu4_random = Some(Eu4Rng::from_saved_count(seed, count));
        world.eu4_event_seed = Some(seed);
        world
    }

    pub(crate) fn random_source(&mut self) -> RandomSource<'_> {
        match self.eu4_random.as_mut() {
            Some(random) => RandomSource::Eu4(random),
            None => RandomSource::ChaCha(&mut self.random),
        }
    }

    /// Draws one raw random value from the world's selected random stream.
    pub fn draw_random_u32(&mut self) -> u32 {
        use rand_chacha::rand_core::TryRng;

        match self.random_source().try_next_u32() {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    pub fn eu4_random_state(&self) -> Option<(u32, u32, u16)> {
        self.eu4_random
            .as_ref()
            .map(|random| (random.seed(), random.count(), random.index()))
    }

    pub fn eu4_event_seed(&self) -> Option<u32> {
        self.eu4_event_seed
    }

    pub fn set_eu4_event_seed(&mut self, seed: Option<u32>) {
        self.eu4_event_seed = seed;
    }

    pub fn fill_eu4_random_batch(&mut self, forbidden: bool) -> Result<[u32; 1000], Error> {
        self.require_running()?;
        let random = self
            .eu4_random
            .as_mut()
            .ok_or_else(|| Error::Invalid("EU4 random stream is not active".into()))?;
        let mut output = [0; 1000];
        random.fill_batch_1000(&mut output, forbidden);
        Ok(output)
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn economy(&self) -> &Economy {
        &self.economy
    }

    pub fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    pub fn affiliations(&self) -> &Affiliations {
        &self.affiliations
    }

    pub fn estates(&self) -> &crate::Estates {
        &self.estates
    }

    pub fn set_estates(&mut self, estates: crate::Estates) -> Result<(), Error> {
        self.require_running()?;
        let previous = std::mem::replace(&mut self.estates, estates);
        if let Err(error) = self.estates.validate(self) {
            self.estates = previous;
            return Err(error);
        }
        Ok(())
    }

    /// Applies the scheduled loyalty update for one country. Targets are
    /// supplied by the content or compatibility layer; an unknown target does
    /// not turn into the default loyalty value.
    pub fn update_estate_loyalty(&mut self, country: EntityId) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("estate loyalty requires a country".into()));
        }
        let Some(decay) = self.estates.rules.loyalty_decay else {
            return Ok(());
        };
        let mut estates = self.estates.clone();
        if let Some(states) = estates.countries.get_mut(&country) {
            for state in states.values_mut() {
                if state.active != Some(true) {
                    continue;
                }
                let (Some(current), Some(target)) =
                    (state.loyalty_milli, state.loyalty_target_milli)
                else {
                    continue;
                };
                state.loyalty_milli = Some(decay.next_loyalty_milli(current, target)?);
            }
        }
        estates.validate(self)?;
        self.estates = estates;
        Ok(())
    }

    /// Runs the country-indexed daily estate schedule observed in EU4. The
    /// registration order is retained by the loader; native worlds fall back
    /// to entity order when no explicit order was supplied.
    fn update_daily_estate_loyalty(&mut self) -> Result<(), Error> {
        let Some(calendar) = &self.calendar else {
            return Ok(());
        };
        let date = calendar.date(self.day)?;
        let month_length = usize::from(calendar.month_lengths[usize::from(date.month - 1)]);
        let day_in_month = usize::from(date.day - 1);
        let countries = if self.subject_country_order.is_empty() {
            self.entities
                .iter()
                .enumerate()
                .filter(|(_, entity)| entity.kind == EntityKind::Country)
                .map(|(index, _)| EntityId(index))
                .collect::<Vec<_>>()
        } else {
            self.subject_country_order.clone()
        };
        for (index, country) in countries.into_iter().enumerate() {
            if index % month_length == day_in_month {
                self.update_estate_loyalty(country)?;
            }
        }
        Ok(())
    }

    pub fn ideas(&self) -> &Ideas {
        &self.ideas
    }

    pub fn set_ideas(&mut self, ideas: Ideas) -> Result<(), Error> {
        self.require_running()?;
        ideas.validate(self)?;
        self.ideas = ideas;
        Ok(())
    }

    pub fn governments(&self) -> &Governments {
        &self.governments
    }

    pub fn set_governments(&mut self, governments: Governments) -> Result<(), Error> {
        self.require_running()?;
        governments.validate(self)?;
        let previous = std::mem::replace(&mut self.governments, governments);
        let validation = self
            .event_catalog
            .validate(self)
            .and_then(|()| self.validate_event_content(&self.event_catalog))
            .and_then(|()| self.objectives.validate(self))
            .and_then(|()| self.estates.validate(self));
        if let Err(error) = validation {
            self.governments = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_affiliations(&mut self, affiliations: Affiliations) -> Result<(), Error> {
        self.require_running()?;
        affiliations.validate(self)?;
        let previous = std::mem::replace(&mut self.affiliations, affiliations);
        let validation = self
            .school_opinions
            .validate(self)
            .and_then(|()| self.unit_selections.validate(self));
        if let Err(error) = validation {
            self.affiliations = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn administration(&self) -> &Administration {
        &self.administration
    }

    pub fn set_administration(&mut self, administration: Administration) -> Result<(), Error> {
        self.replace_administration(administration).map(|_| ())
    }

    pub(crate) fn replace_administration(
        &mut self,
        administration: Administration,
    ) -> Result<Arc<Administration>, Error> {
        self.require_running()?;
        administration.validate(self)?;
        Ok(std::mem::replace(
            &mut self.administration,
            Arc::new(administration),
        ))
    }

    pub(crate) fn restore_administration(&mut self, administration: Arc<Administration>) {
        self.administration = administration;
    }

    pub fn set_attributes(&mut self, attributes: Attributes) -> Result<(), Error> {
        self.require_running()?;
        attributes.validate(self)?;
        if self.monthly_accruals.as_ref().is_some_and(|accruals| {
            accruals.rules.iter().any(|(entity, rules)| {
                rules.values().any(|rule| {
                    !attributes
                        .formulas
                        .get(entity)
                        .is_some_and(|formulas| formulas.contains_key(&rule.attribute))
                })
            })
        }) {
            return Err(Error::Invalid(
                "attributes omit an accrual calculation".into(),
            ));
        }
        if let Some(stocks) = &self.monthly_stocks {
            stocks.validate_attributes(&attributes)?;
        }
        self.attributes = attributes;
        Ok(())
    }

    pub fn technology(&self) -> &Technology {
        &self.technology
    }
    pub fn modifiers(&self) -> &Modifiers {
        &self.modifiers
    }
    pub fn country_resources(&self) -> &BTreeMap<EntityId, CountryResources> {
        &self.country_resources
    }

    pub fn set_technology(&mut self, technology: Technology) -> Result<(), Error> {
        self.require_running()?;
        technology.validate(self)?;
        let previous = std::mem::replace(&mut self.technology, technology);
        if let Err(error) = self.unit_selections.validate(self) {
            self.technology = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn unit_templates(&self) -> &crate::UnitTemplates {
        &self.unit_templates
    }

    pub fn set_unit_templates(
        &mut self,
        unit_templates: crate::UnitTemplates,
    ) -> Result<(), Error> {
        self.require_running()?;
        unit_templates.validate()?;
        let previous = std::mem::replace(&mut self.unit_templates, unit_templates);
        if let Err(error) = self.unit_selections.validate(self) {
            self.unit_templates = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_modifiers(&mut self, modifiers: Modifiers) -> Result<(), Error> {
        self.require_running()?;
        modifiers.validate(self)?;
        let previous = std::mem::replace(&mut self.modifiers, modifiers);
        let validation = self
            .event_catalog
            .validate(self)
            .and_then(|()| self.validate_event_content(&self.event_catalog));
        if let Err(error) = validation {
            self.modifiers = previous;
            return Err(error);
        }
        self.modifier_expirations = self.modifiers.expiration_index();
        Ok(())
    }

    pub fn set_country_resources(
        &mut self,
        resources: BTreeMap<EntityId, CountryResources>,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.validate_resources(&resources)?;
        self.country_resources = resources;
        Ok(())
    }

    fn validate_resources(
        &self,
        resources: &BTreeMap<EntityId, CountryResources>,
    ) -> Result<(), Error> {
        for (&id, state) in resources {
            if self.entity(id)?.kind != EntityKind::Country
                || state.manpower_milli.is_some_and(|v| v < 0)
                || state.mercantilism_milli.is_some_and(|v| v < 0)
            {
                return Err(Error::Invalid("invalid country resource state".into()));
            }
        }
        Ok(())
    }

    pub fn set_economy(&mut self, economy: Economy) -> Result<(), Error> {
        self.require_running()?;
        economy.validate(self)?;
        self.economy = economy;
        Ok(())
    }
    pub fn day(&self) -> u64 {
        self.day
    }

    pub fn calendar(&self) -> Option<&Calendar> {
        self.calendar.as_ref()
    }

    pub fn date(&self) -> Result<Option<WorldDate>, Error> {
        self.calendar.as_ref().map(|c| c.date(self.day)).transpose()
    }

    pub fn taxation(&self) -> Option<&Taxation> {
        self.taxation.as_ref()
    }

    pub fn monthly_system_id(&self) -> Option<&str> {
        self.monthly_system_id.as_deref()
    }

    pub fn daily_system_id(&self) -> Option<&str> {
        self.daily_system_id.as_deref()
    }

    /// Select a replaceable daily phase before the campaign starts.
    pub fn select_daily_system(&mut self, system: &dyn DailySystem) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0 || system.id().trim().is_empty() {
            return Err(Error::Invalid("invalid daily system selection".into()));
        }
        system.validate(self)?;
        self.daily_system_id = Some(system.id().into());
        Ok(())
    }

    /// Select an entire monthly executor before advancing, discarding the
    /// inactive standard taxation state. The replacement supplies saved state
    /// through the same World components or schema fields.
    pub fn select_monthly_system(&mut self, system: &dyn MonthlySystem) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0
            || self.calendar.is_none()
            || system.id().is_empty()
            || system.id() == MonthlyTax.id()
            || system.id() == crate::MonthlyEconomy.id()
        {
            return Err(Error::Invalid("invalid monthly system selection".into()));
        }
        system.validate(self)?;
        self.taxation = None;
        self.monthly_accruals = None;
        self.monthly_convergence = None;
        self.monthly_stocks = None;
        self.monthly_production = None;
        self.monthly_cashflows = None;
        self.monthly_system_id = Some(system.id().into());
        Ok(())
    }

    pub fn set_calendar(&mut self, calendar: Calendar) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0 {
            return Err(Error::Invalid(
                "calendar must be selected before advancing".into(),
            ));
        }
        calendar.date(0)?;
        let previous = self.calendar.replace(calendar);
        match self.truces.validate(self) {
            Ok(index) => self.truce_expirations = index,
            Err(error) => {
                self.calendar = previous;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn set_taxation(&mut self, taxation: Taxation) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0
            && self.monthly_system_id() != Some(MonthlyTax.id())
            && self.monthly_system_id() != Some(crate::MonthlyEconomy.id())
        {
            return Err(Error::Invalid(
                "monthly system cannot change after advancing".into(),
            ));
        }
        taxation.validate(self)?;
        if self
            .monthly_stocks
            .as_ref()
            .is_some_and(|stocks| stocks.conflicts_with_taxation(&taxation))
        {
            return Err(Error::Invalid("duplicate monthly treasury executor".into()));
        }
        self.taxation = Some(taxation);
        self.monthly_system_id = Some(
            if self.monthly_accruals.is_some()
                || self.monthly_convergence.is_some()
                || self.monthly_stocks.is_some()
                || self.monthly_production.is_some()
                || self.monthly_cashflows.is_some()
            {
                crate::MonthlyEconomy.id()
            } else {
                MonthlyTax.id()
            }
            .into(),
        );
        Ok(())
    }
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
    pub fn events(&self) -> &[(u64, EntityId, String)] {
        &self.events
    }
    pub fn pending_tasks(&self) -> usize {
        self.tasks.len()
    }

    pub fn add_entity(&mut self, name: impl Into<String>, kind: EntityKind) -> EntityId {
        let id = EntityId(self.entities.len());
        let name = name.into();
        self.entity_names.entry(name.clone()).or_default().push(id);
        if kind == EntityKind::Province {
            Arc::make_mut(&mut self.ownership).add_province();
        }
        if kind == EntityKind::Country && !self.subject_country_order.is_empty() {
            self.subject_country_order.push(id);
        }
        self.entities.push(Entity {
            name,
            kind,
            owner: None,
            capital: None,
            province_has_port: None,
            values: vec![0; self.schema.fields.len()],
        });
        id
    }

    pub fn set_owner(&mut self, province: EntityId, country: EntityId) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province
            || self.entity(country)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "owner relation requires province and country".into(),
            ));
        }
        let unknown = self.entities[province.0].owner.is_none()
            && !self
                .known_absent_targets
                .get(&province)
                .is_some_and(|targets| targets.contains(&Target::Owner));
        if self.entities[province.0].owner != Some(country) {
            Arc::make_mut(&mut self.ownership).assign(
                province,
                self.entities[province.0].owner,
                unknown,
                country,
            );
        }
        self.entities[province.0].owner = Some(country);
        self.clear_absent_target(province, Target::Owner);
        Ok(())
    }

    pub fn set_capital(&mut self, country: EntityId, province: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country
            || self.entity(province)?.owner != Some(country)
        {
            return Err(Error::Invalid("capital must be an owned province".into()));
        }
        self.entities[country.0].capital = Some(province);
        self.clear_absent_target(country, Target::Capital);
        Ok(())
    }

    pub fn entity(&self, id: EntityId) -> Result<&Entity, Error> {
        self.entities
            .get(id.0)
            .ok_or_else(|| Error::Invalid(format!("unknown entity {}", id.0)))
    }

    pub fn province_has_port(&self, province: EntityId) -> Result<bool, Error> {
        let entity = self.entity(province)?;
        if entity.kind != EntityKind::Province {
            return Err(Error::Invalid("port query requires a province".into()));
        }
        entity.province_has_port.ok_or_else(|| {
            Error::Invalid(format!("unknown port presence for province {}", province.0))
        })
    }

    pub fn set_province_has_port(
        &mut self,
        province: EntityId,
        has_port: bool,
    ) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid("port property requires a province".into()));
        }
        self.entities[province.0].province_has_port = Some(has_port);
        Ok(())
    }

    pub fn target(&self, current: EntityId, target: Target) -> Result<EntityId, Error> {
        let entity = self.entity(current)?;
        match target {
            Target::Current => Ok(current),
            Target::Owner => entity
                .owner
                .ok_or_else(|| Error::Invalid("entity has no owner".into())),
            Target::Capital => entity
                .capital
                .ok_or_else(|| Error::Invalid("entity has no capital".into())),
        }
    }

    pub fn value(&self, entity: EntityId, field: FieldId) -> Result<i64, Error> {
        self.entity(entity)?
            .values
            .get(field.0)
            .copied()
            .ok_or_else(|| Error::Invalid(format!("unknown field {}", field.0)))
    }

    pub fn set(&mut self, entity: EntityId, field: FieldId, value: i64) -> Result<(), Error> {
        self.entity(entity)?;
        let slot = self.entities[entity.0]
            .values
            .get_mut(field.0)
            .ok_or_else(|| Error::Invalid(format!("unknown field {}", field.0)))?;
        *slot = value;
        Ok(())
    }

    pub(crate) fn emit(&mut self, current: EntityId, message: String) {
        self.events.push((self.day, current, message));
    }

    pub fn schedule(&mut self, current: EntityId, delay: u64, plan: Plan) -> Result<(), Error> {
        self.schedule_in_context(crate::ScopeContext::new(current), delay, plan)
    }

    pub fn schedule_in_context(
        &mut self,
        context: crate::ScopeContext,
        delay: u64,
        plan: Plan,
    ) -> Result<(), Error> {
        self.require_running()?;
        context.validate(self)?;
        validate_plan(
            &plan,
            &self.schema,
            self.rules.len(),
            &self.event_catalog,
            &self.completions,
            &self.modifiers,
            &self.governments,
            &self.metrics,
            &self.justifications,
            &self.subjects,
        )?;
        if delay == 0 {
            return Err(Error::Invalid(
                "scheduled tasks require a positive delay".into(),
            ));
        }
        let day = self.day.checked_add(delay).ok_or(Error::Overflow)?;
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1).ok_or(Error::Overflow)?;
        self.tasks.insert(
            (day, sequence),
            Task {
                day,
                sequence,
                current: context.current,
                root: (context.root != context.current).then_some(context.root),
                plan,
            },
        );
        Ok(())
    }

    /// Jump between due tasks and monthly settlements. Tasks on a boundary day
    /// execute in insertion order before taxation reads the resulting state.
    pub fn advance_to(&mut self, day: u64) -> Result<(), Error> {
        self.require_running()?;
        if self.daily_system_id.is_some() {
            return Err(Error::Invalid(
                "selected daily system requires its matching provider".into(),
            ));
        }
        let monthly = self.builtin_monthly_system()?;
        self.advance_inner(day, None, monthly)
    }

    pub fn advance_with_monthly_system(
        &mut self,
        day: u64,
        system: &dyn MonthlySystem,
    ) -> Result<(), Error> {
        self.advance_with_systems(day, None, Some(system))
    }

    pub fn advance_with_daily_system(
        &mut self,
        day: u64,
        system: &dyn DailySystem,
    ) -> Result<(), Error> {
        self.advance_with_systems(day, Some(system), None)
    }

    /// Advance with the selected daily phase and monthly executor.
    ///
    /// A provider is required exactly when its corresponding system id is
    /// selected. Built-in monthly executors remain available through
    /// [`World::advance_to`] and when no custom daily provider is selected.
    pub fn advance_with_systems(
        &mut self,
        day: u64,
        daily: Option<&dyn DailySystem>,
        monthly: Option<&dyn MonthlySystem>,
    ) -> Result<(), Error> {
        self.require_running()?;
        match (self.daily_system_id.as_deref(), daily) {
            (Some(expected), Some(system)) if expected == system.id() => system.validate(self)?,
            (Some(_), _) => {
                return Err(Error::Invalid(
                    "daily system identifier does not match".into(),
                ));
            }
            (None, Some(_)) => {
                return Err(Error::Invalid("no daily system has been selected".into()));
            }
            (None, None) => {}
        }
        let monthly = match (self.monthly_system_id.as_deref(), monthly) {
            (Some(expected), Some(system)) if expected == system.id() => {
                system.validate(self)?;
                Some(system)
            }
            (Some(expected), None) if expected == MonthlyTax.id() => {
                Some(&MonthlyTax as &dyn MonthlySystem)
            }
            (Some(expected), None) if expected == crate::MonthlyEconomy.id() => {
                Some(&crate::MonthlyEconomy as &dyn MonthlySystem)
            }
            (Some(_), _) => {
                return Err(Error::Invalid(
                    "monthly system identifier does not match".into(),
                ));
            }
            (None, Some(_)) => {
                return Err(Error::Invalid("no monthly system has been selected".into()));
            }
            (None, None) => None,
        };
        self.advance_inner(day, daily, monthly)
    }

    fn builtin_monthly_system(&self) -> Result<Option<&'static dyn MonthlySystem>, Error> {
        match self.monthly_system_id() {
            None => Ok(None),
            Some(id) if id == MonthlyTax.id() => Ok(Some(&MonthlyTax)),
            Some(id) if id == crate::MonthlyEconomy.id() => Ok(Some(&crate::MonthlyEconomy)),
            Some(_) => Err(Error::Invalid(
                "selected monthly system requires its matching provider".into(),
            )),
        }
    }

    fn advance_inner(
        &mut self,
        day: u64,
        daily: Option<&dyn DailySystem>,
        monthly: Option<&dyn MonthlySystem>,
    ) -> Result<(), Error> {
        self.require_running()?;
        if day < self.day {
            return Err(Error::Invalid("cannot advance backwards".into()));
        }
        if let Some(calendar) = &self.calendar {
            calendar.date(day)?;
        }
        self.process_event_commands()?;
        loop {
            let month = monthly
                .map(|_| {
                    self.calendar
                        .as_ref()
                        .expect("validated calendar")
                        .next_month(self.day)
                })
                .transpose()?;
            let task_day = self.tasks.first_key_value().map(|(key, _)| key.0);
            let expiration = self
                .modifier_expirations
                .first_key_value()
                .map(|(day, _)| *day);
            let truce_retirement = self
                .truce_expirations
                .first_key_value()
                .map(|(&day, _)| day);
            let grant_tick = if self.justifications.active.is_empty() {
                None
            } else {
                Some(self.day.checked_add(1).ok_or(Error::Overflow)?)
            };
            let daily_estate_tick = self
                .estates
                .rules
                .loyalty_decay
                .is_some()
                .then(|| self.day.checked_add(1).ok_or(Error::Overflow))
                .transpose()?;
            let daily_system_tick = daily
                .is_some()
                .then(|| self.day.checked_add(1).ok_or(Error::Overflow))
                .transpose()?;
            let next = task_day
                .into_iter()
                .chain(month)
                .chain(expiration)
                .chain(truce_retirement)
                .chain(grant_tick)
                .chain(daily_estate_tick)
                .chain(daily_system_tick)
                .min();
            let Some(next) = next.filter(|&next| next <= day) else {
                break;
            };
            self.day = next;
            if let Some(system) = daily
                && let Err(error) = system.tick(self)
            {
                self.failed = true;
                return Err(error);
            }
            self.update_daily_estate_loyalty()?;
            self.expire_modifiers(next);
            self.retire_truces(next, false);
            if let Err(error) = self.maintain_justifications(false) {
                self.failed = true;
                return Err(error);
            }
            while self
                .tasks
                .first_key_value()
                .is_some_and(|(key, _)| key.0 == next)
            {
                let (_, task) = self.tasks.pop_first().expect("checked nonempty queue");
                self.execute_in_context(
                    crate::ScopeContext {
                        current: task.current,
                        root: task.root.unwrap_or(task.current),
                    },
                    &task.plan,
                )?;
            }
            self.process_event_commands()?;
            self.retire_truces(next, true);
            if !self.justifications.active.is_empty()
                && self
                    .calendar
                    .as_ref()
                    .map(|calendar| calendar.date(next))
                    .transpose()?
                    .is_some_and(|date| date.day == 1)
            {
                self.maintain_justifications(true)?;
            }
            if month == Some(next) {
                let result = monthly.expect("selected monthly provider").settle(self);
                if let Err(error) = result {
                    self.failed = true;
                    return Err(error);
                }
            }
            self.process_event_commands()?;
        }
        self.day = day;
        Ok(())
    }

    pub fn save(&self, content: &str) -> Result<String, Error> {
        self.require_running()?;
        let eu4_random = self.eu4_random.clone();
        Ok(serde_json::to_string_pretty(&Snapshot {
            format: if eu4_random.is_some() { 2 } else { 1 },
            content: content.into(),
            schema: self.schema.clone(),
            entities: self.entities.clone(),
            armies: self.armies.clone(),
            naval_units: self.naval_units.clone(),
            owned_province_order: self.ownership.order_overrides(),
            subject_country_order: self.subject_country_order.clone(),
            day: self.day,
            next_sequence: self.next_sequence,
            tasks: self.tasks.values().cloned().collect(),
            events: self.events.clone(),
            random: eu4_random.is_none().then(|| self.random.clone()),
            eu4_random,
            eu4_event_seed: self.eu4_event_seed,
            politics: self.politics.clone(),
            advisors: self.advisors.clone(),
            truces: self.truces.clone(),
            subjects: self.subjects.clone(),
            country_controllers: self.country_controllers.clone(),
            global_scopes: self.global_scopes.clone(),
            known_absent_targets: self.known_absent_targets.clone(),
            economy: self.economy.clone(),
            technology: self.technology.clone(),
            unit_selections: self.unit_selections.clone(),
            unit_templates: self.unit_templates.clone(),
            modifiers: self.modifiers.clone(),
            justifications: self.justifications.clone(),
            country_resources: self.country_resources.clone(),
            calendar: self.calendar.clone(),
            taxation: self.taxation.clone(),
            monthly_accruals: self.monthly_accruals.clone(),
            monthly_convergence: self.monthly_convergence.clone(),
            monthly_stocks: self.monthly_stocks.clone(),
            monthly_production: self.monthly_production.clone(),
            monthly_cashflows: self.monthly_cashflows.clone(),
            monthly_system_id: self.monthly_system_id.clone(),
            daily_system_id: self.daily_system_id.clone(),
            attributes: self.attributes.clone(),
            administration: self.administration.as_ref().clone(),
            affiliations: self.affiliations.clone(),
            school_opinions: self.school_opinions.clone(),
            governments: self.governments.clone(),
            metrics: self.metrics.clone(),
            ideas: self.ideas.clone(),
            estates: self.estates.clone(),
            completions: self.completions.clone(),
            objectives: self.objectives.clone(),
            named_values: self.named_values.clone(),
            markers: self.markers.clone(),
            global_markers: self.global_markers.clone(),
            event_queue: self.event_queue.clone(),
        })?)
    }

    pub(crate) fn content_identity_state(&self) -> Result<String, Error> {
        let mut world = self.clone();
        world.random = ChaCha8Rng::from_seed([0; 32]);
        world.eu4_random = None;
        world.eu4_event_seed = None;
        // These fields describe the running session, not immutable content. In
        // particular, AI event choices can vary with the session seed and must
        // not make an otherwise identical save refuse to load.
        world.day = 0;
        world.next_sequence = 0;
        world.tasks.clear();
        world.events.clear();
        world.event_queue = crate::event_choices::EventQueue::default();
        world.save("")
    }

    pub fn restore(json: &str, content: &str, schema: &Schema) -> Result<Self, Error> {
        Self::restore_with_rules(json, content, schema, Vec::new())
    }

    pub fn restore_with_rules(
        json: &str,
        content: &str,
        schema: &Schema,
        rules: Vec<Plan>,
    ) -> Result<Self, Error> {
        Self::restore_with_events(json, content, schema, rules, crate::EventCatalog::default())
    }

    /// Reattach the same immutable event content when restoring pending choices.
    pub fn restore_with_events(
        json: &str,
        content: &str,
        schema: &Schema,
        rules: Vec<Plan>,
        catalog: crate::EventCatalog,
    ) -> Result<Self, Error> {
        let snapshot: Snapshot = serde_json::from_str(json)?;
        if !matches!(snapshot.format, 1 | 2) {
            return Err(Error::Invalid("unsupported save format".into()));
        }
        if snapshot.content != content {
            return Err(Error::Invalid("save content does not match".into()));
        }
        if &snapshot.schema != schema {
            return Err(Error::Invalid("save schema does not match".into()));
        }
        let (random, eu4_random) = match (snapshot.format, snapshot.random, snapshot.eu4_random) {
            (1, Some(random), None) => (random, None),
            (2, None, Some(random)) if random.is_valid() => {
                (ChaCha8Rng::from_seed([0; 32]), Some(random))
            }
            _ => {
                return Err(Error::Invalid(
                    "save random state does not match its format".into(),
                ));
            }
        };
        let mut world = Self {
            schema: snapshot.schema,
            entities: snapshot.entities,
            armies: snapshot.armies,
            naval_units: snapshot.naval_units,
            ownership: Arc::default(),
            entity_names: BTreeMap::new(),
            day: snapshot.day,
            next_sequence: snapshot.next_sequence,
            tasks: BTreeMap::new(),
            events: snapshot.events,
            event_catalog: catalog,
            event_queue: snapshot.event_queue,
            failed: false,
            rules,
            random,
            eu4_random,
            eu4_event_seed: snapshot.eu4_event_seed,
            politics: snapshot.politics,
            advisors: snapshot.advisors,
            truces: snapshot.truces,
            truce_expirations: Default::default(),
            subjects: snapshot.subjects,
            subject_index: crate::subjects::SubjectIndex::default(),
            subject_country_order: Vec::new(),
            country_controllers: snapshot.country_controllers,
            global_scopes: snapshot.global_scopes,
            known_absent_targets: snapshot.known_absent_targets,
            economy: snapshot.economy,
            technology: snapshot.technology,
            unit_selections: snapshot.unit_selections,
            unit_templates: snapshot.unit_templates,
            modifier_expirations: snapshot.modifiers.expiration_index(),
            modifiers: snapshot.modifiers,
            justifications: snapshot.justifications,
            country_resources: snapshot.country_resources,
            calendar: snapshot.calendar,
            taxation: snapshot.taxation,
            monthly_accruals: snapshot.monthly_accruals,
            monthly_convergence: snapshot.monthly_convergence,
            monthly_stocks: snapshot.monthly_stocks,
            monthly_production: snapshot.monthly_production,
            monthly_cashflows: snapshot.monthly_cashflows,
            monthly_system_id: snapshot.monthly_system_id,
            daily_system_id: snapshot.daily_system_id,
            attributes: snapshot.attributes,
            administration: Arc::new(snapshot.administration),
            affiliations: snapshot.affiliations,
            school_opinions: snapshot.school_opinions,
            governments: snapshot.governments,
            metrics: snapshot.metrics,
            ideas: snapshot.ideas,
            estates: snapshot.estates,
            completions: snapshot.completions,
            objectives: snapshot.objectives,
            named_values: snapshot.named_values,
            markers: snapshot.markers,
            global_markers: snapshot.global_markers,
        };
        world.rebuild_ownership();
        world.subject_index = crate::subjects::SubjectIndex::from_politics(&world.politics);
        world.set_subject_country_order(snapshot.subject_country_order)?;
        Arc::make_mut(&mut world.ownership).restore_order(snapshot.owned_province_order)?;
        for (id, entity) in world.entities.iter().enumerate() {
            world
                .entity_names
                .entry(entity.name.clone())
                .or_default()
                .push(EntityId(id));
        }
        world.armies.validate(&world.entities)?;
        world.naval_units.validate(&world.entities)?;
        world.unit_templates.validate()?;
        world.unit_selections.validate(&world)?;
        for rule in &world.rules {
            validate_plan(
                rule,
                schema,
                world.rules.len(),
                &world.event_catalog,
                &world.completions,
                &world.modifiers,
                &world.governments,
                &world.metrics,
                &world.justifications,
                &world.subjects,
            )?;
        }
        world.event_catalog.validate(&world)?;
        world.truce_expirations = world.truces.validate(&world)?;
        world.event_queue.validate(&world)?;
        world.validate_absent_targets()?;
        for &country in world.country_controllers.keys() {
            world.country_controller(country)?;
        }
        for (name, &entity) in &world.global_scopes {
            crate::scopes::validate_name(name)?;
            world.entity(entity)?;
        }
        for (index, entity) in world.entities.iter().enumerate() {
            if entity.values.len() != world.schema.fields.len() {
                return Err(Error::Invalid("invalid state field count".into()));
            }
            if let Some(owner) = entity.owner
                && (entity.kind != EntityKind::Province
                    || world.entity(owner)?.kind != EntityKind::Country)
            {
                return Err(Error::Invalid("invalid saved owner".into()));
            }
            if let Some(capital) = entity.capital
                && (entity.kind != EntityKind::Country
                    || world.entity(capital)?.owner != Some(EntityId(index)))
            {
                return Err(Error::Invalid("invalid saved capital".into()));
            }
        }
        world.politics.validate(&world)?;
        world.advisors.validate(&world)?;
        world.subjects.validate(&world.politics)?;
        world.subjects.validate_counts(&world)?;
        world.economy.validate(&world)?;
        world.technology.validate(&world)?;
        world.modifiers.validate(&world)?;
        world.justifications.validate(&world)?;
        world.validate_resources(&world.country_resources)?;
        world.administration.validate(&world)?;
        world.affiliations.validate(&world)?;
        world.school_opinions.validate(&world)?;
        world.governments.validate(&world)?;
        world.metrics.validate(&world)?;
        world.ideas.validate(&world)?;
        world.estates.validate(&world)?;
        world.completions.validate(&world)?;
        world.objectives.validate(&world)?;
        world.named_values.validate(&world)?;
        world.markers.validate(&world)?;
        world.global_markers.validate()?;
        world.attributes.validate(&world)?;
        if let Some(calendar) = &world.calendar {
            calendar.date(world.day)?;
        }
        if let Some(taxation) = &world.taxation {
            taxation.validate(&world)?;
        }
        if let Some(accruals) = &world.monthly_accruals {
            accruals.validate(&world)?;
        }
        if let Some(convergence) = &world.monthly_convergence {
            convergence.validate(&world)?;
        }
        if let Some(stocks) = &world.monthly_stocks {
            stocks.validate(&world)?;
        }
        if let Some(production) = &world.monthly_production {
            production.validate(&world)?;
        }
        if let Some(cashflows) = &world.monthly_cashflows {
            cashflows.validate(&world)?;
        }
        match world.monthly_system_id() {
            None if world.taxation.is_none()
                && world.monthly_accruals.is_none()
                && world.monthly_convergence.is_none()
                && world.monthly_stocks.is_none()
                && world.monthly_production.is_none()
                && world.monthly_cashflows.is_none() => {}
            Some(id)
                if id == crate::MonthlyEconomy.id()
                    && (world.monthly_accruals.is_some()
                        || world.monthly_convergence.is_some()
                        || world.monthly_stocks.is_some()
                        || world.monthly_production.is_some()
                        || world.monthly_cashflows.is_some()) => {}
            Some(id)
                if !id.is_empty()
                    && world.calendar.is_some()
                    && world.monthly_accruals.is_none()
                    && world.monthly_convergence.is_none()
                    && world.monthly_stocks.is_none()
                    && world.monthly_production.is_none()
                    && world.monthly_cashflows.is_none()
                    && (world.taxation.is_some() == (id == MonthlyTax.id())) => {}
            _ => {
                return Err(Error::Invalid(
                    "invalid saved monthly system selection".into(),
                ));
            }
        }
        if world
            .daily_system_id()
            .is_some_and(|id| id.trim().is_empty())
        {
            return Err(Error::Invalid(
                "invalid saved daily system selection".into(),
            ));
        }
        let mut sequences = std::collections::BTreeSet::new();
        for task in snapshot.tasks {
            if task.day <= world.day
                || task.sequence >= world.next_sequence
                || !sequences.insert(task.sequence)
            {
                return Err(Error::Invalid("invalid saved task order".into()));
            }
            world.entity(task.current)?;
            if let Some(root) = task.root {
                world.entity(root)?;
            }
            validate_plan(
                &task.plan,
                schema,
                world.rules.len(),
                &world.event_catalog,
                &world.completions,
                &world.modifiers,
                &world.governments,
                &world.metrics,
                &world.justifications,
                &world.subjects,
            )?;
            world.tasks.insert((task.day, task.sequence), task);
        }
        for (day, entity, _) in &world.events {
            if *day > world.day {
                return Err(Error::Invalid("saved event is in the future".into()));
            }
            world.entity(*entity)?;
        }
        Ok(world)
    }

    pub(crate) fn require_running(&self) -> Result<(), Error> {
        if self.failed {
            Err(Error::Invalid(
                "simulation stopped after a runtime error".into(),
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn validate_event_content(
        &self,
        catalog: &crate::EventCatalog,
    ) -> Result<(), Error> {
        self.justifications.validate_definitions(self)?;
        self.objectives.validate(self)?;
        for rule in &self.rules {
            validate_plan(
                rule,
                &self.schema,
                self.rules.len(),
                catalog,
                &self.completions,
                &self.modifiers,
                &self.governments,
                &self.metrics,
                &self.justifications,
                &self.subjects,
            )?;
        }
        for task in self.tasks.values() {
            validate_plan(
                &task.plan,
                &self.schema,
                self.rules.len(),
                catalog,
                &self.completions,
                &self.modifiers,
                &self.governments,
                &self.metrics,
                &self.justifications,
                &self.subjects,
            )?;
        }
        Ok(())
    }

    pub fn validate_condition(&self, condition: &crate::Condition) -> Result<(), Error> {
        validate_condition(
            condition,
            &self.schema,
            &self.completions,
            &self.modifiers,
            &self.governments,
            &self.metrics,
            &self.justifications,
            &self.subjects,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_plan(
    plan: &[Action],
    schema: &Schema,
    rule_count: usize,
    events: &crate::EventCatalog,
    completions: &crate::Completions,
    modifiers: &Modifiers,
    governments: &Governments,
    metrics: &crate::Metrics,
    justifications: &crate::Justifications,
    subjects: &crate::Subjects,
) -> Result<(), Error> {
    for action in plan {
        match action {
            Action::ChangeGovernment { kind, before, .. } => {
                governments.form_definition(kind)?;
                validate_plan(
                    before,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::AddGovernmentReform { id, .. } | Action::RemoveGovernmentReform { id, .. } => {
                governments.reform_definition(id)?;
            }
            Action::SetGovernmentRank { rank, .. } if !(1..=3).contains(rank) => {
                return Err(Error::Invalid(
                    "government rank must be between 1 and 3".into(),
                ));
            }
            Action::SetOfficeHolder { office } if office.is_empty() => {
                return Err(Error::Invalid("empty political office".into()));
            }
            Action::SetOwner { owner, .. } => owner.validate()?,
            Action::CreateNavalUnit { owner, unit_type } => {
                owner.validate()?;
                crate::naval::NavalUnits::validate_type(unit_type)?;
            }
            Action::CreateArmy {
                owner,
                location,
                regiments,
                ..
            } => {
                owner.validate()?;
                location.validate()?;
                crate::army::Armies::validate_regiment_types(regiments)?;
            }
            Action::CreateArmyFromCategories {
                owner,
                location,
                categories,
                ..
            } => {
                owner.validate()?;
                location.validate()?;
                crate::army::Armies::validate_categories(categories)?;
            }
            Action::ForEachSubjectCountry {
                condition, plan, ..
            }
            | Action::ForEachOwnedProvince {
                condition, plan, ..
            }
            | Action::RandomOwnedProvince {
                condition, plan, ..
            }
            | Action::RandomCountry { condition, plan } => {
                validate_condition(
                    condition,
                    schema,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
                validate_plan(
                    plan,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::ForEach {
                selectors,
                condition,
                plan,
                ..
            } => {
                for selector in selectors {
                    selector.validate()?;
                }
                validate_condition(
                    condition,
                    schema,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
                validate_plan(
                    plan,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::SetAffiliation {
                dimension, member, ..
            } => {
                crate::affiliations::validate_condition(dimension, member.as_deref())?;
            }
            Action::SetSchoolOpinion {
                target: crate::SchoolOpinionTarget::Country(target),
                ..
            } => target.validate()?,
            Action::SetSchoolOpinion { .. } => {}
            Action::GrantJustification {
                issuer,
                target,
                name,
                term,
            } => {
                issuer.validate()?;
                target.validate()?;
                justifications.identifier(name)?;
                term.validate()?;
            }
            Action::GrantTruce {
                first,
                second,
                term,
            } => {
                first.validate()?;
                second.validate()?;
                term.validate()?;
            }
            Action::ClearTruce { first, second } => {
                first.validate()?;
                second.validate()?;
            }
            Action::RemoveJustification {
                issuer,
                target,
                name,
            } => {
                issuer.validate()?;
                target.validate()?;
                justifications.identifier(name)?;
            }
            Action::SetProvinceCore {
                province, country, ..
            } => {
                province.validate()?;
                country.validate()?;
            }
            Action::Description { key, .. } => crate::plan::validate_description_key(key)?,
            Action::SetMarker { name, .. } | Action::SetGlobalMarker { name, .. } => {
                crate::markers::validate_name(name)?
            }
            Action::AttachModifier {
                name,
                duration,
                duration_label,
                ..
            } => {
                crate::modifiers::validate_duration_label(duration_label.as_deref())?;
                modifiers.identifier(name)?;
                if *duration == Some(0) {
                    return Err(Error::Invalid("modifier duration must be positive".into()));
                }
            }
            Action::RemoveModifier { name, .. } => modifiers.identifier(name)?,
            Action::UpdateNumeric {
                slot, operation, ..
            } => {
                slot.validate(schema, governments, metrics)?;
                slot.validate_write()?;
                operation.validate()?;
            }
            Action::RecordCompletion { id } => completions.identifier(id)?,
            Action::RefreshObjectives => (),
            Action::InScope {
                selector,
                kind,
                plan,
            } => {
                selector.validate()?;
                if matches!(selector, crate::ScopeSelector::Office(_))
                    && *kind != EntityKind::Country
                {
                    return Err(Error::Invalid("office scope requires country type".into()));
                }
                validate_plan(
                    plan,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::BindGlobalScope { name } | Action::ClearGlobalScope { name } => {
                crate::scopes::validate_name(name)?;
            }
            Action::TriggerEvent { event } | Action::TryEvent { event }
                if !events.definitions().contains_key(event) =>
            {
                return Err(Error::Invalid(format!("unknown event in plan {event}")));
            }
            Action::EventCandidates { events: candidates } => {
                for event in candidates {
                    if !events.definitions().contains_key(event) {
                        return Err(Error::Invalid(format!("unknown event in plan {event}")));
                    }
                }
            }
            Action::Add { field, .. } | Action::Set { field, .. }
                if field.0 >= schema.fields.len() =>
            {
                return Err(Error::Invalid("invalid saved plan field".into()));
            }
            Action::If {
                condition,
                then,
                otherwise,
            } => {
                validate_condition(
                    condition,
                    schema,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
                validate_plan(
                    then,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
                validate_plan(
                    otherwise,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::RandomBranch { then, otherwise } => {
                validate_plan(
                    then,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
                validate_plan(
                    otherwise,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::With { plan, .. } | Action::Preview { plan } => validate_plan(
                plan,
                schema,
                rule_count,
                events,
                completions,
                modifiers,
                governments,
                metrics,
                justifications,
                subjects,
            )?,
            Action::Schedule { delay, plan } => {
                if *delay == 0 {
                    return Err(Error::Invalid("invalid saved task delay".into()));
                }
                validate_plan(
                    plan,
                    schema,
                    rule_count,
                    events,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Action::Accumulate { field, source, .. }
                if field.0 >= schema.fields.len() || source.0 >= schema.fields.len() =>
            {
                return Err(Error::Invalid("invalid saved arithmetic field".into()));
            }
            Action::Run(rule) if rule.0 >= rule_count => {
                return Err(Error::Invalid("invalid saved rule".into()));
            }
            _ => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_condition(
    condition: &crate::Condition,
    schema: &Schema,
    completions: &crate::Completions,
    modifiers: &Modifiers,
    governments: &Governments,
    metrics: &crate::Metrics,
    justifications: &crate::Justifications,
    subjects: &crate::Subjects,
) -> Result<(), Error> {
    match condition {
        crate::Condition::ProvinceCore {
            province, country, ..
        } => {
            province.validate()?;
            country.validate()
        }
        crate::Condition::SubjectOf { subject, overlord } => {
            subject.validate()?;
            overlord.validate()
        }
        crate::Condition::TruceWith { first, second } => {
            first.validate()?;
            second.validate()
        }
        crate::Condition::NumericCompareEntities {
            first,
            second,
            slot,
            ..
        } => {
            first.validate()?;
            second.validate()?;
            slot.validate(schema, governments, metrics)
        }
        crate::Condition::HasJustification {
            issuer,
            target,
            name,
        } => {
            issuer.validate()?;
            target.validate()?;
            if let Some(name) = name {
                justifications.identifier(name)?;
            }
            Ok(())
        }
        crate::Condition::AffiliationPresent { dimension, .. } => {
            crate::affiliations::validate_condition(dimension, None)
        }
        crate::Condition::AffiliationMember {
            dimension, member, ..
        } => crate::affiliations::validate_condition(dimension, Some(member)),
        crate::Condition::AffiliationGroup {
            dimension, group, ..
        } => crate::affiliations::validate_condition(dimension, Some(group)),
        crate::Condition::SubjectType { subject_type, .. } => subjects.identifier(subject_type),
        crate::Condition::GovernmentKind { kind, .. } => {
            crate::governments::validate_identifier(kind)
        }
        crate::Condition::GovernmentRank { rank, .. } if !(1..=3).contains(rank) => Err(
            Error::Invalid("government rank must be between 1 and 3".into()),
        ),
        crate::Condition::GovernmentReform { id, .. } => {
            governments.reform_definition(id).map(|_| ())
        }
        crate::Condition::GovernmentFeature { name, .. } => {
            crate::governments::validate_identifier(name)
        }
        crate::Condition::GovernmentMetricActive { name, .. }
        | crate::Condition::GovernmentMetricStored { name, .. } => {
            governments.metric_definition(name).map(|_| ())
        }
        crate::Condition::EstatePrivilege { privilege, .. } if privilege.is_empty() => {
            Err(Error::Invalid("empty estate privilege identifier".into()))
        }
        crate::Condition::HasIdea { id, .. } if id.is_empty() => {
            Err(Error::Invalid("empty idea identifier".into()))
        }
        crate::Condition::EstateLoyaltyAtLeast {
            estate,
            loyalty_milli,
            ..
        } if estate.is_empty() || !(0..=100_000).contains(loyalty_milli) => {
            Err(Error::Invalid("invalid estate loyalty condition".into()))
        }
        crate::Condition::EstateInfluenceAtLeast {
            estate,
            influence_milli,
            ..
        } if estate.is_empty() || *influence_milli < 0 => {
            Err(Error::Invalid("invalid estate influence condition".into()))
        }
        crate::Condition::HasMarker { name, .. } | crate::Condition::HasGlobalMarker { name } => {
            crate::markers::validate_name(name)
        }
        crate::Condition::PoliticalStatus { name, .. } if name.is_empty() => {
            Err(Error::Invalid("empty political status name".into()))
        }
        crate::Condition::HasModifier { name, .. } => modifiers.identifier(name),
        crate::Condition::NumericCompare { slot, .. } => {
            slot.validate(schema, governments, metrics)
        }
        crate::Condition::CompletionRecorded { id, .. }
        | crate::Condition::ObjectiveAvailable { id, .. } => completions.identifier(id),
        crate::Condition::Unavailable(reason) => {
            if reason.is_empty() {
                Err(Error::Invalid("empty unavailable condition reason".into()))
            } else {
                Ok(())
            }
        }
        crate::Condition::EntityNamed { name, .. } if name.is_empty() => {
            Err(Error::Invalid("empty entity condition name".into()))
        }
        crate::Condition::YearAtLeast(year) if *year < 1 => Err(Error::Invalid(
            "year condition requires a positive year".into(),
        )),
        crate::Condition::ScopeIsCurrent(selector) => selector.validate(),
        crate::Condition::AnySubjectCountry { condition, .. }
        | crate::Condition::With { condition, .. }
        | crate::Condition::AnyOwnedProvince { condition, .. } => validate_condition(
            condition,
            schema,
            completions,
            modifiers,
            governments,
            metrics,
            justifications,
            subjects,
        ),
        crate::Condition::InScope {
            selector,
            condition,
            ..
        } => {
            selector.validate()?;
            validate_condition(
                condition,
                schema,
                completions,
                modifiers,
                governments,
                metrics,
                justifications,
                subjects,
            )
        }
        crate::Condition::GeographicMember { name, .. } if name.trim().is_empty() => Err(
            Error::Invalid("empty geographic condition identifier".into()),
        ),
        crate::Condition::AtLeast { field, .. } if field.0 >= schema.fields.len() => {
            Err(Error::Invalid("invalid saved condition field".into()))
        }
        crate::Condition::If {
            condition,
            then,
            otherwise,
        } => {
            for child in [condition.as_ref(), then.as_ref(), otherwise.as_ref()] {
                validate_condition(
                    child,
                    schema,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Ok(())
        }
        crate::Condition::All(conditions) | crate::Condition::Any(conditions) => {
            for condition in conditions {
                validate_condition(
                    condition,
                    schema,
                    completions,
                    modifiers,
                    governments,
                    metrics,
                    justifications,
                    subjects,
                )?;
            }
            Ok(())
        }
        crate::Condition::Not(condition) => validate_condition(
            condition,
            schema,
            completions,
            modifiers,
            governments,
            metrics,
            justifications,
            subjects,
        ),
        _ => Ok(()),
    }
}

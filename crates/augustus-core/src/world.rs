// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{
    Action, Administration, Affiliations, Attributes, Calendar, CountryResources, Economy, Error,
    Governments, Ideas, Modifiers, MonthlySystem, MonthlyTax, Plan, Politics, Target, Taxation,
    Technology, WorldDate,
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
    values: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Task {
    day: u64,
    sequence: u64,
    current: EntityId,
    plan: Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    schema: Schema,
    entities: Vec<Entity>,
    pub(crate) ownership: crate::ownership::OwnershipIndex,
    pub(crate) entity_names: BTreeMap<String, Vec<EntityId>>,
    day: u64,
    next_sequence: u64,
    tasks: BTreeMap<(u64, u64), Task>,
    events: Vec<(u64, EntityId, String)>,
    pub(crate) event_catalog: crate::EventCatalog,
    pub(crate) event_queue: crate::event_choices::EventQueue,
    pub(crate) politics: Politics,
    pub(crate) country_controllers: BTreeMap<EntityId, crate::CountryController>,
    pub(crate) global_scopes: BTreeMap<String, EntityId>,
    pub(crate) known_absent_targets: BTreeMap<EntityId, BTreeSet<Target>>,
    economy: Economy,
    technology: Technology,
    pub(crate) modifiers: Modifiers,
    pub(crate) modifier_expirations: BTreeMap<u64, BTreeSet<(EntityId, String)>>,
    pub(crate) justifications: crate::Justifications,
    pub(crate) country_resources: BTreeMap<EntityId, CountryResources>,
    calendar: Option<Calendar>,
    pub(crate) taxation: Option<Taxation>,
    monthly_system_id: Option<String>,
    attributes: Attributes,
    administration: Administration,
    pub(crate) affiliations: Affiliations,
    pub(crate) governments: Governments,
    pub(crate) metrics: crate::Metrics,
    ideas: Ideas,
    pub(crate) estates: crate::Estates,
    pub(crate) completions: crate::Completions,
    pub(crate) objectives: crate::Objectives,
    pub(crate) named_values: crate::NamedValues,
    pub(crate) markers: crate::Markers,
    pub(crate) failed: bool,
    pub(crate) rules: Vec<Plan>,
    pub(crate) random: ChaCha8Rng,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: u32,
    content: String,
    schema: Schema,
    entities: Vec<Entity>,
    day: u64,
    next_sequence: u64,
    tasks: Vec<Task>,
    events: Vec<(u64, EntityId, String)>,
    random: ChaCha8Rng,
    #[serde(default, skip_serializing_if = "Politics::is_empty")]
    politics: Politics,
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
    monthly_system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Attributes::is_empty")]
    attributes: Attributes,
    #[serde(default, skip_serializing_if = "Administration::is_empty")]
    administration: Administration,
    #[serde(default, skip_serializing_if = "Affiliations::is_empty")]
    affiliations: Affiliations,
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
            ownership: crate::ownership::OwnershipIndex::default(),
            entity_names: BTreeMap::new(),
            day: 0,
            next_sequence: 0,
            tasks: BTreeMap::new(),
            events: Vec::new(),
            event_catalog: crate::EventCatalog::default(),
            event_queue: crate::event_choices::EventQueue::default(),
            politics: Politics::default(),
            country_controllers: BTreeMap::new(),
            global_scopes: BTreeMap::new(),
            known_absent_targets: BTreeMap::new(),
            economy: Economy::default(),
            technology: Technology::default(),
            modifiers: Modifiers::default(),
            modifier_expirations: BTreeMap::new(),
            justifications: crate::Justifications::default(),
            country_resources: BTreeMap::new(),
            calendar: None,
            taxation: None,
            monthly_system_id: None,
            attributes: Attributes::default(),
            administration: Administration::default(),
            affiliations: Affiliations::default(),
            governments: Governments::default(),
            metrics: crate::Metrics::default(),
            ideas: Ideas::default(),
            estates: crate::Estates::default(),
            completions: crate::Completions::default(),
            objectives: crate::Objectives::default(),
            named_values: crate::NamedValues::default(),
            markers: crate::Markers::default(),
            failed: false,
            rules,
            random: ChaCha8Rng::from_seed(seed),
        }
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
        estates.validate(self)?;
        self.estates = estates;
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
            .and_then(|()| self.validate_event_content(&self.event_catalog));
        if let Err(error) = validation {
            self.governments = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_affiliations(&mut self, affiliations: Affiliations) -> Result<(), Error> {
        self.require_running()?;
        affiliations.validate(self)?;
        self.affiliations = affiliations;
        Ok(())
    }

    pub fn administration(&self) -> &Administration {
        &self.administration
    }

    pub fn set_administration(&mut self, administration: Administration) -> Result<(), Error> {
        self.require_running()?;
        administration.validate(self)?;
        self.administration = administration;
        Ok(())
    }

    pub fn set_attributes(&mut self, attributes: Attributes) -> Result<(), Error> {
        self.require_running()?;
        attributes.validate(self)?;
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
        self.technology = technology;
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

    /// Select an entire monthly executor before advancing, discarding the
    /// inactive standard taxation state. The replacement supplies saved state
    /// through the same World components or schema fields.
    pub fn select_monthly_system(&mut self, system: &dyn MonthlySystem) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0
            || self.calendar.is_none()
            || system.id().is_empty()
            || system.id() == MonthlyTax.id()
        {
            return Err(Error::Invalid("invalid monthly system selection".into()));
        }
        system.validate(self)?;
        self.taxation = None;
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
        self.calendar = Some(calendar);
        Ok(())
    }

    pub fn set_taxation(&mut self, taxation: Taxation) -> Result<(), Error> {
        self.require_running()?;
        if self.day != 0 && self.monthly_system_id() != Some(MonthlyTax.id()) {
            return Err(Error::Invalid(
                "monthly system cannot change after advancing".into(),
            ));
        }
        taxation.validate(self)?;
        self.taxation = Some(taxation);
        self.monthly_system_id = Some(MonthlyTax.id().into());
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
            self.ownership.add_province();
        }
        self.entities.push(Entity {
            name,
            kind,
            owner: None,
            capital: None,
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
        self.ownership
            .assign(self.entities[province.0].owner, unknown, country);
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
        self.require_running()?;
        self.entity(current)?;
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
                current,
                plan,
            },
        );
        Ok(())
    }

    /// Jump between due tasks and monthly settlements. Tasks on a boundary day
    /// execute in insertion order before taxation reads the resulting state.
    pub fn advance_to(&mut self, day: u64) -> Result<(), Error> {
        self.require_running()?;
        match self.monthly_system_id() {
            None => self.advance_inner(day, None),
            Some(id) if id == MonthlyTax.id() => self.advance_with_monthly_system(day, &MonthlyTax),
            Some(_) => Err(Error::Invalid(
                "selected monthly system requires its matching provider".into(),
            )),
        }
    }

    pub fn advance_with_monthly_system(
        &mut self,
        day: u64,
        system: &dyn MonthlySystem,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.monthly_system_id() != Some(system.id()) {
            return Err(Error::Invalid(
                "monthly system identifier does not match".into(),
            ));
        }
        system.validate(self)?;
        self.advance_inner(day, Some(system))
    }

    fn advance_inner(&mut self, day: u64, system: Option<&dyn MonthlySystem>) -> Result<(), Error> {
        self.require_running()?;
        if day < self.day {
            return Err(Error::Invalid("cannot advance backwards".into()));
        }
        if let Some(calendar) = &self.calendar {
            calendar.date(day)?;
        }
        loop {
            let month = system
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
            let grant_tick = if self.justifications.active.is_empty() {
                None
            } else {
                Some(self.day.checked_add(1).ok_or(Error::Overflow)?)
            };
            let next = task_day
                .into_iter()
                .chain(month)
                .chain(expiration)
                .chain(grant_tick)
                .min();
            let Some(next) = next.filter(|&next| next <= day) else {
                break;
            };
            self.day = next;
            self.expire_modifiers(next);
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
                self.execute(task.current, &task.plan)?;
            }
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
                let result = system.expect("selected monthly provider").settle(self);
                if let Err(error) = result {
                    self.failed = true;
                    return Err(error);
                }
            }
        }
        self.day = day;
        Ok(())
    }

    pub fn save(&self, content: &str) -> Result<String, Error> {
        self.require_running()?;
        Ok(serde_json::to_string_pretty(&Snapshot {
            format: 1,
            content: content.into(),
            schema: self.schema.clone(),
            entities: self.entities.clone(),
            day: self.day,
            next_sequence: self.next_sequence,
            tasks: self.tasks.values().cloned().collect(),
            events: self.events.clone(),
            random: self.random.clone(),
            politics: self.politics.clone(),
            country_controllers: self.country_controllers.clone(),
            global_scopes: self.global_scopes.clone(),
            known_absent_targets: self.known_absent_targets.clone(),
            economy: self.economy.clone(),
            technology: self.technology.clone(),
            modifiers: self.modifiers.clone(),
            justifications: self.justifications.clone(),
            country_resources: self.country_resources.clone(),
            calendar: self.calendar.clone(),
            taxation: self.taxation.clone(),
            monthly_system_id: self.monthly_system_id.clone(),
            attributes: self.attributes.clone(),
            administration: self.administration.clone(),
            affiliations: self.affiliations.clone(),
            governments: self.governments.clone(),
            metrics: self.metrics.clone(),
            ideas: self.ideas.clone(),
            estates: self.estates.clone(),
            completions: self.completions.clone(),
            objectives: self.objectives.clone(),
            named_values: self.named_values.clone(),
            markers: self.markers.clone(),
            event_queue: self.event_queue.clone(),
        })?)
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
        if snapshot.format != 1 || snapshot.content != content || &snapshot.schema != schema {
            return Err(Error::Invalid(
                "save format, content or schema does not match".into(),
            ));
        }
        let mut world = Self {
            schema: snapshot.schema,
            entities: snapshot.entities,
            ownership: crate::ownership::OwnershipIndex::default(),
            entity_names: BTreeMap::new(),
            day: snapshot.day,
            next_sequence: snapshot.next_sequence,
            tasks: BTreeMap::new(),
            events: snapshot.events,
            event_catalog: catalog,
            event_queue: snapshot.event_queue,
            failed: false,
            rules,
            random: snapshot.random,
            politics: snapshot.politics,
            country_controllers: snapshot.country_controllers,
            global_scopes: snapshot.global_scopes,
            known_absent_targets: snapshot.known_absent_targets,
            economy: snapshot.economy,
            technology: snapshot.technology,
            modifier_expirations: snapshot.modifiers.expiration_index(),
            modifiers: snapshot.modifiers,
            justifications: snapshot.justifications,
            country_resources: snapshot.country_resources,
            calendar: snapshot.calendar,
            taxation: snapshot.taxation,
            monthly_system_id: snapshot.monthly_system_id,
            attributes: snapshot.attributes,
            administration: snapshot.administration,
            affiliations: snapshot.affiliations,
            governments: snapshot.governments,
            metrics: snapshot.metrics,
            ideas: snapshot.ideas,
            estates: snapshot.estates,
            completions: snapshot.completions,
            objectives: snapshot.objectives,
            named_values: snapshot.named_values,
            markers: snapshot.markers,
        };
        world.rebuild_ownership();
        for (id, entity) in world.entities.iter().enumerate() {
            world
                .entity_names
                .entry(entity.name.clone())
                .or_default()
                .push(EntityId(id));
        }
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
            )?;
        }
        world.event_catalog.validate(&world)?;
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
        world.economy.validate(&world)?;
        world.technology.validate(&world)?;
        world.modifiers.validate(&world)?;
        world.justifications.validate(&world)?;
        world.validate_resources(&world.country_resources)?;
        world.administration.validate(&world)?;
        world.affiliations.validate(&world)?;
        world.governments.validate(&world)?;
        world.metrics.validate(&world)?;
        world.ideas.validate(&world)?;
        world.estates.validate(&world)?;
        world.completions.validate(&world)?;
        world.objectives.validate(&world)?;
        world.named_values.validate(&world)?;
        world.markers.validate(&world)?;
        world.attributes.validate(&world)?;
        if let Some(calendar) = &world.calendar {
            calendar.date(world.day)?;
        }
        if let Some(taxation) = &world.taxation {
            taxation.validate(&world)?;
        }
        match world.monthly_system_id() {
            None if world.taxation.is_none() => {}
            Some(id)
                if !id.is_empty()
                    && world.calendar.is_some()
                    && (world.taxation.is_some() == (id == MonthlyTax.id())) => {}
            _ => {
                return Err(Error::Invalid(
                    "invalid saved monthly system selection".into(),
                ));
            }
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
            )?;
        }
        Ok(())
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
) -> Result<(), Error> {
    for action in plan {
        match action {
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
                )?;
            }
            Action::SetAffiliation {
                dimension, member, ..
            } => {
                crate::affiliations::validate_condition(dimension, member.as_deref())?;
            }
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
            Action::RemoveJustification {
                issuer,
                target,
                name,
            } => {
                issuer.validate()?;
                target.validate()?;
                justifications.identifier(name)?;
            }
            Action::Description { key, .. } => crate::plan::validate_description_key(key)?,
            Action::SetMarker { name, .. } => crate::markers::validate_name(name)?,
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

pub(crate) fn validate_condition(
    condition: &crate::Condition,
    schema: &Schema,
    completions: &crate::Completions,
    modifiers: &Modifiers,
    governments: &Governments,
    metrics: &crate::Metrics,
    justifications: &crate::Justifications,
) -> Result<(), Error> {
    match condition {
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
        crate::Condition::GovernmentKind { kind, .. } => {
            crate::governments::validate_identifier(kind)
        }
        crate::Condition::GovernmentReform { id, .. } => {
            governments.reform_definition(id).map(|_| ())
        }
        crate::Condition::GovernmentFeature { name, .. } => {
            crate::governments::validate_identifier(name)
        }
        crate::Condition::GovernmentMetricActive { name, .. } => {
            governments.metric_definition(name).map(|_| ())
        }
        crate::Condition::HasMarker { name, .. } => crate::markers::validate_name(name),
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
        crate::Condition::With { condition, .. } => validate_condition(
            condition,
            schema,
            completions,
            modifiers,
            governments,
            metrics,
            justifications,
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
            )
        }
        crate::Condition::GeographicMember { name, .. } if name.trim().is_empty() => Err(
            Error::Invalid("empty geographic condition identifier".into()),
        ),
        crate::Condition::AtLeast { field, .. } if field.0 >= schema.fields.len() => {
            Err(Error::Invalid("invalid saved condition field".into()))
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
        ),
        _ => Ok(()),
    }
}

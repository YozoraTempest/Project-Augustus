// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use rand::RngExt;
use rand_chacha::rand_core::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{EntityId, EntityKind, Error, FieldId, RuleId, ScopeContext, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    Current,
    Owner,
    Capital,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchoolOpinionTarget {
    Previous,
    Country(crate::EntityRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Condition {
    NumericCompareEntities {
        first: crate::EntityRef,
        second: crate::EntityRef,
        slot: crate::NumericSlot,
        comparison: crate::NumericComparison,
    },
    TruceWith {
        first: crate::EntityRef,
        second: crate::EntityRef,
    },
    TrucePresent {
        target: Target,
    },
    ProvinceCore {
        province: crate::EntityRef,
        country: crate::EntityRef,
        /// None matches either known type; a typed query rejects an unknown type.
        core: Option<crate::CoreType>,
    },
    ProvinceHasPort,
    NationalFocus {
        target: Target,
        skill: crate::RulerSkill,
    },
    HasJustification {
        issuer: crate::EntityRef,
        target: crate::EntityRef,
        name: Option<String>,
    },
    SubjectOf {
        subject: crate::EntityRef,
        overlord: crate::EntityRef,
    },
    SubjectPresent {
        target: Target,
    },
    HasOwnedProvince {
        target: Target,
    },
    AnyOwnedProvince {
        target: Target,
        condition: Box<Condition>,
    },
    AnySubjectCountry {
        target: Target,
        condition: Box<Condition>,
    },
    SubjectType {
        target: Target,
        subject_type: String,
    },
    With {
        target: Target,
        kind: crate::EntityKind,
        condition: Box<Condition>,
    },
    InScope {
        selector: crate::ScopeSelector,
        kind: crate::EntityKind,
        condition: Box<Condition>,
    },
    GeographicMember {
        target: Target,
        level: crate::GeographicLevel,
        name: String,
    },
    CountryControlled {
        target: Target,
        controller: crate::CountryController,
    },
    /// Read an authoritative named boolean fact for a country.
    PoliticalStatus {
        target: Target,
        name: String,
    },
    AffiliationPresent {
        target: Target,
        dimension: String,
    },
    /// Compare an entity's live membership in a content-defined dimension.
    AffiliationMember {
        target: Target,
        dimension: String,
        member: String,
    },
    /// Compare the live membership's group; unknown membership or grouping is an error.
    AffiliationGroup {
        target: Target,
        dimension: String,
        group: String,
    },
    GovernmentKind {
        target: Target,
        kind: String,
    },
    GovernmentRank {
        target: Target,
        rank: u8,
    },
    GovernmentReform {
        target: Target,
        id: String,
    },
    GovernmentFeature {
        target: Target,
        name: String,
    },
    GovernmentMetricActive {
        target: Target,
        name: String,
    },
    GovernmentMetricStored {
        target: Target,
        name: String,
    },
    HasIdea {
        target: Target,
        id: String,
    },
    EstatePrivilege {
        target: Target,
        privilege: String,
    },
    EstateLoyaltyAtLeast {
        target: Target,
        estate: String,
        loyalty_milli: i64,
    },
    EstateInfluenceAtLeast {
        target: Target,
        estate: String,
        influence_milli: i64,
    },
    HasMarker {
        target: Target,
        name: String,
    },
    HasGlobalMarker {
        name: String,
    },
    HasModifier {
        target: Target,
        name: String,
    },
    NumericCompare {
        target: Target,
        slot: crate::NumericSlot,
        comparison: crate::NumericComparison,
        value: i64,
    },
    /// A predicate that content translation could not resolve.
    Unavailable(String),
    ObjectiveAvailable {
        target: Target,
        id: String,
    },
    CompletionRecorded {
        target: Target,
        id: String,
    },
    /// Compare a target's stable entity name with a content identifier.
    EntityNamed {
        target: Target,
        name: String,
    },
    /// Read the current simulation calendar; a calendar is required.
    YearAtLeast(i16),
    /// Compare the current entity with a live office or global reference.
    ScopeIsCurrent(crate::ScopeSelector),
    Always(bool),
    AtLeast {
        target: Target,
        field: FieldId,
        value: i64,
    },
    /// Evaluate the selector once, then only the selected predicate.
    If {
        condition: Box<Condition>,
        then: Box<Condition>,
        otherwise: Box<Condition>,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    GrantTruce {
        first: crate::EntityRef,
        second: crate::EntityRef,
        term: crate::GrantTerm,
    },
    ClearTruce {
        first: crate::EntityRef,
        second: crate::EntityRef,
    },
    SetProvinceCore {
        province: crate::EntityRef,
        country: crate::EntityRef,
        core: Option<crate::CoreType>,
    },
    /// Capture the current direct subjects in relation order, then filter live state.
    ForEachSubjectCountry {
        country: Target,
        condition: Condition,
        plan: Plan,
    },
    /// Capture the owner's current ordered members, then filter each live state.
    ForEachOwnedProvince {
        country: Target,
        condition: Condition,
        plan: Plan,
    },
    /// Filter current owned members and execute one uniformly sampled body.
    RandomOwnedProvince {
        country: Target,
        condition: Condition,
        plan: Plan,
    },
    /// Select one country from the current world after filtering its live state.
    RandomCountry {
        condition: Condition,
        plan: Plan,
    },
    /// Update the owner edge. Systems using this primitive manage their own
    /// dependent state; it does not perform a complete territorial transfer.
    SetOwner {
        target: Target,
        owner: crate::EntityRef,
    },
    CreateNavalUnit {
        owner: crate::EntityRef,
        unit_type: String,
    },
    CreateArmy {
        owner: crate::EntityRef,
        location: crate::EntityRef,
        name: String,
        regiments: Vec<String>,
    },
    CreateArmyFromCategories {
        owner: crate::EntityRef,
        location: crate::EntityRef,
        name: String,
        categories: Vec<String>,
    },
    /// Capture an ordered selection, then check each member's live condition
    /// immediately before its body. Every iteration shares the caller's budget.
    ForEach {
        selectors: Vec<crate::ScopeSelector>,
        kind: crate::EntityKind,
        condition: Condition,
        plan: Plan,
    },
    GrantJustification {
        issuer: crate::EntityRef,
        target: crate::EntityRef,
        name: String,
        term: crate::GrantTerm,
    },
    RemoveJustification {
        issuer: crate::EntityRef,
        target: crate::EntityRef,
        name: String,
    },
    /// Localisation content for effect descriptions; no simulation side effects.
    Description {
        key: String,
        #[serde(default, skip_serializing_if = "is_false")]
        hidden: bool,
    },
    /// Describe effects without executing the body, resolving scopes, or drawing randomness.
    Preview {
        plan: Plan,
    },
    SetMarker {
        target: Target,
        name: String,
        present: bool,
    },
    SetGlobalMarker {
        name: String,
        present: bool,
    },
    AttachModifier {
        target: Target,
        name: String,
        duration: Option<u64>,
        hidden: Option<bool>,
        renewal: crate::ModifierRenewal,
        /// Localisation key for the effect's duration text; not attachment state.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_label: Option<String>,
    },
    RemoveModifier {
        target: Target,
        name: String,
    },
    SetAffiliation {
        target: Target,
        dimension: String,
        member: Option<String>,
    },
    SetSchoolOpinion {
        target: SchoolOpinionTarget,
        opinion: i64,
    },
    UpdateNumeric {
        target: Target,
        slot: crate::NumericSlot,
        operation: crate::NumericOperation,
        value: i64,
    },
    ChangeRulerSkill {
        target: Target,
        skill: crate::RulerSkill,
        delta: i64,
    },
    ChangeNationalFocus {
        target: Target,
        focus: crate::NationalFocus,
    },
    RefreshObjectives,
    RecordCompletion {
        id: String,
    },
    /// Filter the entire list before executing any selected event.
    EventCandidates {
        events: Vec<String>,
    },
    /// Check the event's condition against the current state, then dispatch.
    TryEvent {
        event: String,
    },
    /// Execute in a live named scope; an absent scope skips the body.
    InScope {
        selector: crate::ScopeSelector,
        kind: crate::EntityKind,
        plan: Plan,
    },
    BindGlobalScope {
        name: String,
    },
    ClearGlobalScope {
        name: String,
    },
    /// Dispatch an event in the current scope, within the caller's execution limits.
    TriggerEvent {
        event: String,
    },
    ChangeEstateMetric {
        estate: String,
        metric: crate::EstateMetric,
        amount: i64,
        minimum: Option<i64>,
    },
    SetEstatePrivilege {
        estate: String,
        privilege: String,
        granted: bool,
    },
    ResolveEstatePrivilegeEffect {
        privilege: String,
        effect: String,
    },
    /// Two equally likely branches; exactly one u64 draw, in instruction order.
    RandomBranch {
        then: Plan,
        otherwise: Plan,
    },
    Accumulate {
        target: Target,
        field: FieldId,
        source: FieldId,
    },
    Run(RuleId),
    Add {
        target: Target,
        field: FieldId,
        value: i64,
    },
    Set {
        target: Target,
        field: FieldId,
        value: i64,
    },
    If {
        condition: Condition,
        then: Plan,
        otherwise: Plan,
    },
    With {
        target: Target,
        plan: Plan,
    },
    /// Execute the supplied callback in country scope before applying the form.
    /// Requests for the current form still execute the callback.
    ChangeGovernment {
        target: Target,
        kind: String,
        before: Plan,
    },
    SetGovernmentRank {
        target: Target,
        rank: u8,
    },
    /// Assign the current country as the holder of a political office.
    SetOfficeHolder {
        office: String,
    },
    AddGovernmentReform {
        target: Target,
        id: String,
    },
    RemoveGovernmentReform {
        target: Target,
        id: String,
    },
    Schedule {
        delay: u64,
        plan: Plan,
    },
    Emit(String),
}

pub type Plan = Vec<Action>;

impl Action {
    /// Return the global marker names read by this action and its nested plans.
    pub fn global_marker_references(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        self.collect_global_marker_references(&mut names);
        names
    }

    fn collect_global_marker_references(&self, names: &mut BTreeSet<String>) {
        let collect_plan = |plan: &[Action], names: &mut BTreeSet<String>| {
            for action in plan {
                action.collect_global_marker_references(names);
            }
        };
        match self {
            Self::ForEachSubjectCountry {
                condition, plan, ..
            }
            | Self::ForEachOwnedProvince {
                condition, plan, ..
            }
            | Self::RandomOwnedProvince {
                condition, plan, ..
            }
            | Self::RandomCountry { condition, plan }
            | Self::ForEach {
                condition, plan, ..
            } => {
                names.extend(condition.global_marker_references());
                collect_plan(plan, names);
            }
            Self::Preview { plan }
            | Self::InScope { plan, .. }
            | Self::With { plan, .. }
            | Self::Schedule { plan, .. } => collect_plan(plan, names),
            Self::RandomBranch { then, otherwise } => {
                collect_plan(then, names);
                collect_plan(otherwise, names);
            }
            Self::If {
                condition,
                then,
                otherwise,
            } => {
                names.extend(condition.global_marker_references());
                collect_plan(then, names);
                collect_plan(otherwise, names);
            }
            Self::ChangeGovernment { before, .. } => collect_plan(before, names),
            _ => (),
        }
    }
}

impl Condition {
    /// Return the global marker names read anywhere in this condition tree.
    pub fn global_marker_references(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        self.collect_global_marker_references(&mut names);
        names
    }

    fn collect_global_marker_references(&self, names: &mut BTreeSet<String>) {
        match self {
            Self::HasGlobalMarker { name } => {
                names.insert(name.clone());
            }
            Self::AnyOwnedProvince { condition, .. }
            | Self::AnySubjectCountry { condition, .. }
            | Self::With { condition, .. }
            | Self::InScope { condition, .. }
            | Self::Not(condition) => condition.collect_global_marker_references(names),
            Self::If {
                condition,
                then,
                otherwise,
            } => {
                condition.collect_global_marker_references(names);
                then.collect_global_marker_references(names);
                otherwise.collect_global_marker_references(names);
            }
            Self::All(conditions) | Self::Any(conditions) => {
                for condition in conditions {
                    condition.collect_global_marker_references(names);
                }
            }
            _ => (),
        }
    }

    pub fn evaluate(&self, world: &World, current: EntityId) -> Result<bool, Error> {
        self.evaluate_in_context(world, crate::ScopeContext::new(current))
    }

    pub fn evaluate_in_context(
        &self,
        world: &World,
        context: crate::ScopeContext,
    ) -> Result<bool, Error> {
        let current = context.current;
        match self {
            Self::NumericCompareEntities {
                first,
                second,
                slot,
                comparison,
            } => {
                let (Some(first), Some(second)) = (
                    first.resolve_in_context(world, context)?,
                    second.resolve_in_context(world, context)?,
                ) else {
                    return Ok(false);
                };
                let first = world.numeric_value(first, slot)?;
                let second = world.numeric_value(second, slot)?;
                Ok(match comparison {
                    crate::NumericComparison::AtLeast => first >= second,
                    crate::NumericComparison::Equal => first == second,
                })
            }
            Self::TruceWith { first, second } => {
                let (Some(first), Some(second)) = (
                    first.resolve_in_context(world, context)?,
                    second.resolve_in_context(world, context)?,
                ) else {
                    return Ok(false);
                };
                world.truce_with(first, second)
            }
            Self::TrucePresent { target } => world.has_truce(world.target(current, *target)?),
            Self::ProvinceCore {
                province,
                country,
                core,
            } => {
                let (Some(province), Some(country)) = (
                    province.resolve_in_context(world, context)?,
                    country.resolve_in_context(world, context)?,
                ) else {
                    return Ok(false);
                };
                match (world.province_core(province, country)?, core) {
                    (None, _) => Ok(false),
                    (Some(_), None) => Ok(true),
                    (Some(crate::CoreType::Unspecified), Some(_)) => {
                        Err(Error::Invalid("unknown province core type".into()))
                    }
                    (Some(actual), Some(expected)) => Ok(actual == *expected),
                }
            }
            Self::ProvinceHasPort => world.province_has_port(current),
            Self::SubjectOf { subject, overlord } => {
                let (Some(subject), Some(overlord)) = (
                    subject.resolve_in_context(world, context)?,
                    overlord.resolve_in_context(world, context)?,
                ) else {
                    return Ok(false);
                };
                if world.entity(overlord)?.kind != EntityKind::Country {
                    return Ok(false);
                }
                world.is_subject_of(subject, overlord)
            }
            Self::SubjectPresent { target } => {
                world.is_subject_country(world.target(current, *target)?)
            }
            Self::NationalFocus { target, skill } => {
                Ok(world.national_focus(world.target(current, *target)?)?.skill == Some(*skill))
            }
            Self::HasJustification {
                issuer,
                target,
                name,
            } => {
                let (Some(issuer), Some(target)) = (
                    issuer.resolve_in_context(world, context)?,
                    target.resolve_in_context(world, context)?,
                ) else {
                    return Ok(false);
                };
                world.has_justification(issuer, target, name.as_deref())
            }
            Self::HasOwnedProvince { target } => {
                world.has_owned_province(world.target(current, *target)?)
            }
            Self::AnyOwnedProvince { target, condition } => world.any_owned_province_in_context(
                world.target(current, *target)?,
                condition,
                context,
            ),
            Self::AnySubjectCountry { target, condition } => world.any_subject_country_in_context(
                world.target(current, *target)?,
                condition,
                context,
            ),
            Self::SubjectType {
                target,
                subject_type,
            } => world.is_subject_of_type(world.target(current, *target)?, subject_type),
            Self::With {
                target,
                kind,
                condition,
            } => {
                let Some(selected) = world.condition_target(current, *target)? else {
                    return Ok(false);
                };
                if world.entity(selected)?.kind != *kind {
                    return Err(Error::Invalid(
                        "condition scope has the wrong entity type".into(),
                    ));
                }
                condition.evaluate_in_context(world, context.with_current(selected))
            }
            Self::InScope {
                selector,
                kind,
                condition,
            } => {
                let Some(selected) = world.resolve_scope_in_context(selector, context)? else {
                    return Ok(false);
                };
                if world.entity(selected)?.kind != *kind {
                    return Err(Error::Invalid(
                        "condition scope has the wrong entity type".into(),
                    ));
                }
                condition.evaluate_in_context(world, context.with_current(selected))
            }
            Self::GeographicMember {
                target,
                level,
                name,
            } => {
                let province = world.target(current, *target)?;
                if world.entity(province)?.kind != crate::EntityKind::Province {
                    return Err(Error::Invalid(
                        "geographic condition requires a province".into(),
                    ));
                }
                world
                    .administration()
                    .geographic_member(province, *level, name)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown geographic membership {name} for entity {}",
                            province.0
                        ))
                    })
            }
            Self::CountryControlled { target, controller } => {
                let country = world.target(current, *target)?;
                let actual = world.country_controller(country)?.ok_or_else(|| {
                    Error::Invalid(format!(
                        "unknown country controller for entity {}",
                        country.0
                    ))
                })?;
                Ok(actual == *controller)
            }
            Self::PoliticalStatus { target, name } => {
                if name.is_empty() {
                    return Err(Error::Invalid("empty political status name".into()));
                }
                let entity = world.target(current, *target)?;
                if world.entity(entity)?.kind != EntityKind::Country {
                    return Err(Error::Invalid("political status requires a country".into()));
                }
                world.politics().status(entity, name).ok_or_else(|| {
                    Error::Invalid(format!(
                        "unknown political status {name} for entity {}",
                        entity.0
                    ))
                })
            }
            Self::AffiliationPresent { target, dimension } => {
                crate::affiliations::validate_condition(dimension, None)?;
                let entity = world.target(current, *target)?;
                world
                    .affiliations()
                    .present(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation presence {dimension} for entity {}",
                            entity.0
                        ))
                    })
            }
            Self::AffiliationMember {
                target,
                dimension,
                member,
            } => {
                crate::affiliations::validate_condition(dimension, Some(member))?;
                let entity = world.target(current, *target)?;
                if world.affiliations().present(dimension, entity) == Some(false) {
                    return Ok(false);
                }
                let actual = world
                    .affiliations()
                    .member(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation member {dimension} for entity {}",
                            entity.0
                        ))
                    })?;
                Ok(actual == member)
            }
            Self::AffiliationGroup {
                target,
                dimension,
                group,
            } => {
                crate::affiliations::validate_condition(dimension, Some(group))?;
                let entity = world.target(current, *target)?;
                if world.affiliations().present(dimension, entity) == Some(false) {
                    return Ok(false);
                }
                let actual = world
                    .affiliations()
                    .group(dimension, entity)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "unknown affiliation group {dimension} for entity {}",
                            entity.0
                        ))
                    })?;
                Ok(actual == group)
            }
            Self::GovernmentKind { target, kind } => {
                crate::governments::validate_identifier(kind)?;
                Ok(world.government_kind(world.target(current, *target)?)? == kind)
            }
            Self::GovernmentRank { target, rank } => {
                if !(1..=3).contains(rank) {
                    return Err(Error::Invalid(
                        "government rank must be between 1 and 3".into(),
                    ));
                }
                Ok(world.government_rank(world.target(current, *target)?)? == *rank)
            }
            Self::GovernmentReform { target, id } => {
                world.government_reform_selected(world.target(current, *target)?, id)
            }
            Self::GovernmentFeature { target, name } => {
                world.government_feature(world.target(current, *target)?, name)
            }
            Self::GovernmentMetricActive { target, name } => {
                world.government_metric_active(world.target(current, *target)?, name)
            }
            Self::GovernmentMetricStored { target, name } => {
                world.government_metric_stored(world.target(current, *target)?, name)
            }
            Self::HasIdea { target, id } => {
                world.ideas().has_idea(world.target(current, *target)?, id)
            }
            Self::EstatePrivilege { target, privilege } => {
                world.estate_has_privilege(world.target(current, *target)?, privilege)
            }
            Self::EstateLoyaltyAtLeast {
                target,
                estate,
                loyalty_milli,
            } => world.estate_loyalty_at_least(
                world.target(current, *target)?,
                estate,
                *loyalty_milli,
            ),
            Self::EstateInfluenceAtLeast {
                target,
                estate,
                influence_milli,
            } => world.estate_influence_at_least(
                world.target(current, *target)?,
                estate,
                *influence_milli,
            ),
            Self::HasMarker { target, name } => {
                world.has_marker(world.target(current, *target)?, name)
            }
            Self::HasGlobalMarker { name } => world.has_global_marker(name),
            Self::HasModifier { target, name } => {
                world.has_modifier(world.target(current, *target)?, name)
            }
            Self::NumericCompare {
                target,
                slot,
                comparison,
                value,
            } => {
                let actual = world.numeric_value(world.target(current, *target)?, slot)?;
                Ok(match comparison {
                    crate::NumericComparison::AtLeast => actual >= *value,
                    crate::NumericComparison::Equal => actual == *value,
                })
            }
            Self::Unavailable(reason) => Err(Error::Invalid(reason.clone())),
            Self::ObjectiveAvailable { target, id } => {
                world.objective_available(world.target(current, *target)?, id)
            }
            Self::CompletionRecorded { target, id } => {
                world.completion_recorded(world.target(current, *target)?, id)
            }
            Self::EntityNamed { target, name } => {
                if name.is_empty() {
                    return Err(Error::Invalid("empty entity condition name".into()));
                }
                Ok(world.entity(world.target(current, *target)?)?.name == *name)
            }
            Self::YearAtLeast(year) => {
                if *year < 1 {
                    return Err(Error::Invalid(
                        "year condition requires a positive year".into(),
                    ));
                }
                let date = world
                    .date()?
                    .ok_or_else(|| Error::Invalid("year condition requires a calendar".into()))?;
                Ok(date.year >= *year)
            }
            Self::ScopeIsCurrent(selector) => {
                world.entity(current)?;
                Ok(world.resolve_scope_in_context(selector, context)? == Some(current))
            }
            Self::Always(value) => Ok(*value),
            Self::AtLeast {
                target,
                field,
                value,
            } => Ok(world.value(world.target(current, *target)?, *field)? >= *value),
            Self::If {
                condition,
                then,
                otherwise,
            } => {
                if condition.evaluate_in_context(world, context)? {
                    then.evaluate_in_context(world, context)
                } else {
                    otherwise.evaluate_in_context(world, context)
                }
            }
            Self::All(conditions) => {
                for condition in conditions {
                    if !condition.evaluate_in_context(world, context)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(conditions) => {
                for condition in conditions {
                    if condition.evaluate_in_context(world, context)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not(condition) => Ok(!condition.evaluate_in_context(world, context)?),
        }
    }
}

impl World {
    /// Writes are visible to the next instruction, including condition evaluation.
    /// A runtime error preserves preceding writes and stops execution and saving.
    pub fn execute(&mut self, current: EntityId, plan: &[Action]) -> Result<(), Error> {
        self.execute_phases(current, &[plan])
    }

    pub fn execute_in_context(
        &mut self,
        context: ScopeContext,
        plan: &[Action],
    ) -> Result<(), Error> {
        self.execute_context_phases(context, &[plan])
    }

    pub(crate) fn execute_phases(
        &mut self,
        current: EntityId,
        phases: &[&[Action]],
    ) -> Result<(), Error> {
        self.execute_context_phases(ScopeContext::new(current), phases)
    }

    fn execute_context_phases(
        &mut self,
        context: ScopeContext,
        phases: &[&[Action]],
    ) -> Result<(), Error> {
        self.require_running()?;
        let mut budget = 100_000;
        let result = phases
            .iter()
            .try_for_each(|plan| self.execute_inner(context, plan, &mut budget, 0, None));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn execute_selected(
        &mut self,
        context: ScopeContext,
        selected: impl IntoIterator<Item = Option<EntityId>>,
        condition: &Condition,
        plan: &[Action],
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), Error> {
        for entity in selected {
            if *budget == 0 {
                return Err(Error::Invalid("instruction limit exceeded".into()));
            }
            *budget -= 1;
            let Some(entity) = entity else {
                continue;
            };
            let selected = context.with_current(entity);
            if condition.evaluate_in_context(self, selected)? {
                self.execute_inner(selected, plan, budget, depth + 1, Some(context.current))?;
            }
        }
        Ok(())
    }

    pub(crate) fn execute_inner(
        &mut self,
        context: ScopeContext,
        plan: &[Action],
        budget: &mut usize,
        depth: usize,
        previous: Option<EntityId>,
    ) -> Result<(), Error> {
        if depth >= 64 {
            return Err(Error::Invalid("plan call depth exceeded".into()));
        }
        context.validate(self)?;
        let current = context.current;
        for action in plan {
            if *budget == 0 {
                return Err(Error::Invalid("plan instruction limit exceeded".into()));
            }
            *budget -= 1;
            match action {
                Action::GrantTruce {
                    first,
                    second,
                    term,
                } => {
                    if let (Some(first), Some(second)) = (
                        first.resolve_in_context(self, context)?,
                        second.resolve_in_context(self, context)?,
                    ) {
                        self.grant_truce(first, second, *term)?;
                    }
                }
                Action::ClearTruce { first, second } => {
                    if let (Some(first), Some(second)) = (
                        first.resolve_in_context(self, context)?,
                        second.resolve_in_context(self, context)?,
                    ) {
                        self.clear_truce(first, second)?;
                    }
                }
                Action::SetProvinceCore {
                    province,
                    country,
                    core,
                } => {
                    if let (Some(province), Some(country)) = (
                        province.resolve_in_context(self, context)?,
                        country.resolve_in_context(self, context)?,
                    ) {
                        self.set_province_core(province, country, *core)?;
                    }
                }
                Action::ChangeGovernment {
                    target,
                    kind,
                    before,
                } => {
                    let entity = self.target(current, *target)?;
                    self.change_government(entity, kind, before, budget, depth, previous)?;
                }
                Action::SetOfficeHolder { office } => {
                    self.set_office_holder(office, current)?;
                }
                Action::SetGovernmentRank { target, rank } => {
                    self.set_government_rank(self.target(current, *target)?, Some(*rank))?;
                }
                Action::AddGovernmentReform { target, id } => {
                    self.add_government_reform(self.target(current, *target)?, id)?;
                }
                Action::RemoveGovernmentReform { target, id } => {
                    self.remove_government_reform(self.target(current, *target)?, id)?;
                }
                Action::SetOwner { target, owner } => {
                    let province = self.target(current, *target)?;
                    let country = owner
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("owner reference is absent".into()))?;
                    self.set_owner(province, country)?;
                }
                Action::CreateNavalUnit { owner, unit_type } => {
                    let owner = owner
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("naval unit owner is absent".into()))?;
                    self.create_naval_unit(owner, current, unit_type)?;
                }
                Action::CreateArmy {
                    owner,
                    location,
                    name,
                    regiments,
                } => {
                    let owner = owner
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("army owner is absent".into()))?;
                    let location = location
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("army location is absent".into()))?;
                    self.create_army(owner, location, name, regiments)?;
                }
                Action::CreateArmyFromCategories {
                    owner,
                    location,
                    name,
                    categories,
                } => {
                    let owner = owner
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("army owner is absent".into()))?;
                    let location = location
                        .resolve_in_context(self, context)?
                        .ok_or_else(|| Error::Invalid("army location is absent".into()))?;
                    self.create_army_from_categories(owner, location, name, categories)?;
                }
                Action::ForEachSubjectCountry {
                    country,
                    condition,
                    plan,
                } => {
                    let country = self.target(current, *country)?;
                    let selected = self.subject_countries(country)?.to_vec();
                    self.execute_selected(
                        context,
                        selected.into_iter().map(Some),
                        condition,
                        plan,
                        budget,
                        depth,
                    )?;
                }
                Action::ForEachOwnedProvince {
                    country,
                    condition,
                    plan,
                } => {
                    let country = self.target(current, *country)?;
                    let selected = self.owned_provinces(country)?.to_vec();
                    self.execute_selected(
                        context,
                        selected.into_iter().map(Some),
                        condition,
                        plan,
                        budget,
                        depth,
                    )?;
                }
                Action::RandomOwnedProvince {
                    country,
                    condition,
                    plan,
                } => {
                    let country = self.target(current, *country)?;
                    let members = self.owned_provinces(country)?.to_vec();
                    let mut selected = Vec::new();
                    for province in members {
                        if *budget == 0 {
                            return Err(Error::Invalid("instruction limit exceeded".into()));
                        }
                        *budget -= 1;
                        if condition.evaluate_in_context(self, context.with_current(province))? {
                            selected.push(province);
                        }
                    }
                    if !selected.is_empty() {
                        let index = self.random_source().random_range(0..selected.len());
                        self.execute_inner(
                            context.with_current(selected[index]),
                            plan,
                            budget,
                            depth + 1,
                            Some(context.current),
                        )?;
                    }
                }
                Action::RandomCountry { condition, plan } => {
                    let mut selected = Vec::new();
                    for (index, entity) in self.entities().iter().enumerate() {
                        if entity.kind != EntityKind::Country {
                            continue;
                        }
                        if *budget == 0 {
                            return Err(Error::Invalid("instruction limit exceeded".into()));
                        }
                        *budget -= 1;
                        let country = EntityId(index);
                        if condition.evaluate_in_context(self, context.with_current(country))? {
                            selected.push(country);
                        }
                    }
                    if !selected.is_empty() {
                        let index = self.random_source().random_range(0..selected.len());
                        self.execute_inner(
                            context.with_current(selected[index]),
                            plan,
                            budget,
                            depth + 1,
                            Some(context.current),
                        )?;
                    }
                }
                Action::ForEach {
                    selectors,
                    kind,
                    condition,
                    plan,
                } => {
                    let mut selected = Vec::with_capacity(selectors.len());
                    for selector in selectors {
                        let entity = self.resolve_scope_in_context(selector, context)?;
                        if let Some(entity) = entity
                            && self.entity(entity)?.kind != *kind
                        {
                            return Err(Error::Invalid(
                                "iteration scope does not match entity".into(),
                            ));
                        }
                        selected.push(entity);
                    }
                    self.execute_selected(context, selected, condition, plan, budget, depth)?;
                }
                Action::GrantJustification {
                    issuer,
                    target,
                    name,
                    term,
                } => {
                    if let (Some(issuer), Some(target)) = (
                        issuer.resolve_in_context(self, context)?,
                        target.resolve_in_context(self, context)?,
                    ) {
                        self.grant_justification(issuer, target, name, *term)?;
                    }
                }
                Action::RemoveJustification {
                    issuer,
                    target,
                    name,
                } => {
                    if let (Some(issuer), Some(target)) = (
                        issuer.resolve_in_context(self, context)?,
                        target.resolve_in_context(self, context)?,
                    ) {
                        self.remove_justification(issuer, target, name)?;
                    }
                }
                Action::Description { key, .. } => validate_description_key(key)?,
                Action::Preview { .. } => (),
                Action::SetMarker {
                    target,
                    name,
                    present,
                } => {
                    let entity = self.target(current, *target)?;
                    self.set_marker(entity, name, *present)?;
                }
                Action::SetGlobalMarker { name, present } => {
                    self.set_global_marker(name, *present)?
                }
                Action::AttachModifier {
                    target,
                    name,
                    duration,
                    hidden,
                    renewal,
                    duration_label,
                } => {
                    crate::modifiers::validate_duration_label(duration_label.as_deref())?;
                    let entity = self.target(current, *target)?;
                    self.attach_modifier(entity, name, *duration, *hidden, *renewal)?;
                }
                Action::RemoveModifier { target, name } => {
                    let entity = self.target(current, *target)?;
                    self.remove_modifier(entity, name)?;
                }
                Action::SetAffiliation {
                    target,
                    dimension,
                    member,
                } => {
                    let entity = self.target(current, *target)?;
                    self.set_affiliation_member(entity, dimension, member.as_deref())?;
                }
                Action::SetSchoolOpinion { target, opinion } => {
                    let target = match target {
                        SchoolOpinionTarget::Previous => previous.ok_or_else(|| {
                            Error::Invalid("previous country scope is unbound".into())
                        })?,
                        SchoolOpinionTarget::Country(target) => {
                            target.resolve_in_context(self, context)?.ok_or_else(|| {
                                Error::Invalid("school opinion target is absent".into())
                            })?
                        }
                    };
                    if self.entity(target)?.kind != EntityKind::Country {
                        return Err(Error::Invalid(
                            "school opinion target must be a country".into(),
                        ));
                    }
                    let school = self
                        .affiliations()
                        .member("religious_school", target)
                        .ok_or_else(|| {
                            Error::Invalid("school opinion target has unknown school".into())
                        })?
                        .to_owned();
                    self.set_school_opinion(current, &school, *opinion)?;
                }
                Action::UpdateNumeric {
                    target,
                    slot,
                    operation,
                    value,
                } => {
                    let entity = self.target(current, *target)?;
                    self.update_numeric(entity, slot, *operation, *value)?;
                }
                Action::RefreshObjectives => self.refresh_objectives(current)?,
                Action::ChangeRulerSkill {
                    target,
                    skill,
                    delta,
                } => {
                    let entity = self.target(current, *target)?;
                    self.change_ruler_skill(entity, *skill, *delta)?;
                }
                Action::ChangeNationalFocus { target, focus } => {
                    let entity = self.target(current, *target)?;
                    self.change_national_focus(entity, *focus)?;
                }
                Action::RecordCompletion { id } => {
                    self.record_completion(current, id)?;
                }
                Action::EventCandidates { events } => {
                    self.event_candidates_inner(events, current, budget, depth + 1)?;
                }
                Action::TryEvent { event } => {
                    self.try_event_inner(event, current, budget, depth + 1)?;
                }
                Action::InScope {
                    selector,
                    kind,
                    plan,
                } => {
                    if let Some(selected) = self.resolve_scope_in_context(selector, context)? {
                        if self.entity(selected)?.kind != *kind {
                            return Err(Error::Invalid(
                                "named scope entity type does not match".into(),
                            ));
                        }
                        self.execute_inner(
                            context.with_current(selected),
                            plan,
                            budget,
                            depth + 1,
                            Some(context.current),
                        )?;
                    }
                }
                Action::BindGlobalScope { name } => self.bind_global_scope(name, current)?,
                Action::ClearGlobalScope { name } => self.clear_global_scope(name)?,
                Action::TriggerEvent { event } => {
                    self.trigger_event_inner(event, current, budget, depth + 1)?;
                }
                Action::ChangeEstateMetric {
                    estate,
                    metric,
                    amount,
                    minimum,
                } => self.change_estate_metric(current, estate, *metric, *amount, *minimum)?,
                Action::SetEstatePrivilege {
                    estate,
                    privilege,
                    granted,
                } => self.set_estate_privilege(current, estate, privilege, *granted)?,
                Action::ResolveEstatePrivilegeEffect { privilege, effect } => {
                    self.resolve_estate_privilege_effect(current, privilege, effect)?
                }
                Action::RandomBranch { then, otherwise } => {
                    let branch = if self.random_source().next_u64() & 1 == 0 {
                        then
                    } else {
                        otherwise
                    };
                    self.execute_inner(context, branch, budget, depth + 1, previous)?;
                }
                Action::Accumulate {
                    target,
                    field,
                    source,
                } => {
                    let entity = self.target(current, *target)?;
                    let value = self
                        .value(entity, *field)?
                        .checked_add(self.value(entity, *source)?)
                        .ok_or(Error::Overflow)?;
                    self.set(entity, *field, value)?;
                }
                Action::Run(rule) => {
                    let plan = self
                        .rules
                        .get(rule.0)
                        .ok_or_else(|| Error::Invalid(format!("unknown rule {}", rule.0)))?
                        .clone();
                    self.execute_inner(context, &plan, budget, depth + 1, previous)?;
                }
                Action::Add {
                    target,
                    field,
                    value,
                } => {
                    let entity = self.target(current, *target)?;
                    let result = self
                        .value(entity, *field)?
                        .checked_add(*value)
                        .ok_or(Error::Overflow)?;
                    self.set(entity, *field, result)?;
                }
                Action::Set {
                    target,
                    field,
                    value,
                } => self.set(self.target(current, *target)?, *field, *value)?,
                Action::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    let branch = if condition.evaluate_in_context(self, context)? {
                        then
                    } else {
                        otherwise
                    };
                    self.execute_inner(context, branch, budget, depth + 1, previous)?;
                }
                Action::With { target, plan } => self.execute_inner(
                    context.with_current(self.target(current, *target)?),
                    plan,
                    budget,
                    depth + 1,
                    Some(context.current),
                )?,
                Action::Schedule { delay, plan } => {
                    self.schedule_in_context(context, *delay, plan.clone())?
                }
                Action::Emit(message) => self.emit(current, message.clone()),
            }
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

pub(crate) fn validate_description_key(key: &str) -> Result<(), Error> {
    if key.trim().is_empty() {
        return Err(Error::Invalid("empty description key".into()));
    }
    Ok(())
}

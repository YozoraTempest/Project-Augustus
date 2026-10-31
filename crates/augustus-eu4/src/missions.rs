// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use augustus_core::{
    Action, Completions, Condition, EntityKind, InitialWorld, Objectives, Plan, World,
};
use jomini::TextToken;
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal};
use crate::localisation::{EventTextKeys, load_event_texts};
use crate::{
    ContentView, Error, FileEntry, ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding,
    VanillaWorld,
};

#[derive(Debug, Default, Serialize)]
pub struct MissionReport {
    pub series: usize,
    pub objectives: usize,
    pub known_countries: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved: BTreeMap<String, String>,
}

/// Current mission definitions and independently known selection/completion state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaCountryMissions {
    pub country: String,
    pub selection_known: bool,
    pub completions_known: bool,
    pub missions: Vec<VanillaMissionView>,
}

/// A selected mission is claimable only when its live requirements and reward
/// are supported and its known completion record is still absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMissionView {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub required_missions: BTreeSet<String>,
    pub selected: Option<bool>,
    pub completed: Option<bool>,
    pub requirements_met: Option<bool>,
    pub reward_supported: bool,
    pub claimable: bool,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct MissionProgram {
    id: String,
    series: String,
    title: String,
    description: Option<String>,
    required_missions: BTreeSet<String>,
    trigger: Option<Condition>,
    effect: Option<Plan>,
    event_roots: BTreeSet<String>,
    executed_event_roots: BTreeSet<String>,
    conditioned_event_roots: BTreeSet<String>,
    immediate_event_roots: BTreeSet<String>,
    unresolved: Vec<String>,
    reward_supported: bool,
    has_execution_rules: bool,
}

pub(crate) struct MissionDraft {
    ids: BTreeSet<String>,
    objectives: Objectives,
    documents: Vec<TextDocument>,
    library: ScriptLibrary,
    geographic_scopes: BTreeMap<String, Vec<u32>>,
}

pub(crate) struct BoundMissions {
    pub report: MissionReport,
    pub programs: BTreeMap<String, MissionProgram>,
    pub identity: Option<Vec<u8>>,
}

impl MissionProgram {
    pub(crate) fn global_marker_references(&self) -> BTreeSet<String> {
        let mut names = self
            .trigger
            .as_ref()
            .map(Condition::global_marker_references)
            .unwrap_or_default();
        if let Some(effect) = &self.effect {
            for action in effect {
                names.extend(action.global_marker_references());
            }
        }
        names
    }
}

impl MissionDraft {
    pub(crate) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        world: &World,
        geographic_scopes: &BTreeMap<String, Vec<u32>>,
    ) -> Result<Self, Error> {
        let ids = identifiers(root, files)?;
        let library = ScriptLibrary::from_root(root, files, TextEncoding::Windows1252)?;
        let documents = files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "missions"))
            .map(|file| root.read_text(&file.relative_path, TextEncoding::Windows1252))
            .collect::<Result<Vec<_>, _>>()?;
        let mut objectives = ScriptCompiler::for_world(world, &library, true)
            .with_mission_ids(ids.clone())
            .with_geographic_scopes(geographic_scopes.clone())
            .compile_missions(&documents)?;
        for source_root in root.source_roots() {
            let prefix = format!("{}{}", source_root.display(), std::path::MAIN_SEPARATOR);
            for group in &mut objectives.groups {
                relative_reasons(&mut group.condition, &prefix);
            }
        }
        Ok(Self {
            ids,
            objectives,
            documents,
            library,
            geographic_scopes: geographic_scopes.clone(),
        })
    }

    pub(crate) fn global_marker_references(&self) -> BTreeSet<String> {
        self.objectives
            .groups
            .iter()
            .flat_map(|group| group.condition.global_marker_references())
            .collect()
    }

    pub(crate) fn register_identifiers(&self, initial: &mut InitialWorld) -> Result<(), Error> {
        initial
            .set_completions(Completions {
                identifiers: self.ids.clone(),
                ..Default::default()
            })
            .map_err(|e| Error::caused_by("missions", e))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind(
        self,
        initial: &mut InitialWorld,
        saved_completions: Option<&BTreeMap<String, BTreeSet<String>>>,
        root: &impl ContentView,
        files: &[FileEntry],
        startup: &mut crate::estates::EstateStartup,
        truce_months: Option<u32>,
        advisor_types: BTreeMap<String, augustus_core::RulerSkill>,
        estate_report: &mut crate::EstateReport,
    ) -> Result<BoundMissions, Error> {
        let mut report = MissionReport {
            series: self.objectives.groups.len(),
            objectives: self.ids.len(),
            ..Default::default()
        };
        if self.ids.is_empty() && self.objectives.groups.is_empty() {
            return Ok(BoundMissions {
                report,
                programs: BTreeMap::new(),
                identity: None,
            });
        }
        let fixed_start = initial
            .world()
            .date()
            .map_err(|e| Error::caused_by("missions", e))?
            == Some(augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            });
        let mut entities = BTreeMap::new();
        if let Some(saved_completions) = saved_completions {
            for (tag, missions) in saved_completions {
                let entity = initial.country_entity(tag).ok_or_else(|| {
                    Error::message(
                        "saved mission completions",
                        format!("unknown country {tag}"),
                    )
                })?;
                entities.insert(entity, missions.clone());
            }
        }
        if fixed_start {
            for country in initial.countries() {
                let entity = initial
                    .country_entity(&country.id)
                    .expect("registered country");
                entities.entry(entity).or_default();
            }
        }
        initial
            .set_completions(Completions {
                identifiers: self.ids.clone(),
                entities,
            })
            .map_err(|e| Error::caused_by("missions", e))?;
        initial
            .set_objectives(self.objectives)
            .map_err(|e| Error::caused_by("missions", e))?;
        let countries: Vec<_> = initial.countries().iter().map(|c| c.id.clone()).collect();
        for tag in countries {
            let entity = initial.country_entity(&tag).expect("registered country");
            match fixed_start.then(|| initial.refresh_objectives(entity)) {
                Some(Ok(())) => report.known_countries += 1,
                Some(Err(error)) => {
                    report.unresolved.insert(tag, error.to_string());
                }
                None => {
                    report.unresolved.insert(
                        tag,
                        "mission selection at this date is not initialized".into(),
                    );
                }
            }
        }

        let mut library = self.library.with_advisor_types(advisor_types);
        let ids = self.ids;
        let mut programs = compile_programs(
            &self.documents,
            &ids,
            initial.world(),
            &mut library,
            &self.geographic_scopes,
            truce_months,
            root,
        )?;
        let roots_by_mission = programs
            .iter()
            .map(|(id, program)| (id.clone(), program.event_roots.clone()))
            .collect::<BTreeMap<_, _>>();
        let failed_events = startup.include_mission_events(
            root,
            files,
            &roots_by_mission,
            initial.world(),
            &self.geographic_scopes,
            truce_months,
            estate_report,
        );
        for program in programs.values_mut() {
            for event in &program.event_roots {
                if let Some(reason) = failed_events.get(event) {
                    program.unresolved.push(format!(
                        "reward event {event} could not be compiled: {reason}"
                    ));
                    program.reward_supported = false;
                }
            }
            if program
                .effect
                .as_ref()
                .is_some_and(|effect| plan_reads_completion(effect, &program.id))
            {
                program.unresolved.push(
                    "reward reads this mission's mission_completed status; reward ordering is unresolved".into(),
                );
                program.reward_supported = false;
            }
            if let Some(reason) = mission_event_issue(program, startup.event_catalog()) {
                program.unresolved.push(reason);
                program.reward_supported = false;
            }
        }

        let text_keys = programs
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    EventTextKeys {
                        title: Some(format!("{id}_title")),
                        description: Some(format!("{id}_desc")),
                        ..Default::default()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let texts = load_event_texts(root, files, &text_keys);
        for (id, program) in &mut programs {
            if let Some(text) = texts.get(id) {
                program.title.clone_from(&text.title);
                program.description.clone_from(&text.description);
            }
        }

        let identities = programs
            .values()
            .filter(|program| program.has_execution_rules)
            .map(|program| MissionRuleIdentity {
                id: &program.id,
                series: &program.series,
                required_missions: &program.required_missions,
                trigger: &program.trigger,
                effect: &program.effect,
                blocked: !program.unresolved.is_empty() || !program.reward_supported,
            })
            .collect::<Vec<_>>();
        let identity = if identities.is_empty() {
            None
        } else {
            Some(serde_json::to_vec(&identities).map_err(|e| Error::caused_by("missions", e))?)
        };
        Ok(BoundMissions {
            report,
            programs,
            identity,
        })
    }
}

#[derive(Serialize)]
struct MissionRuleIdentity<'a> {
    id: &'a str,
    series: &'a str,
    required_missions: &'a BTreeSet<String>,
    trigger: &'a Option<Condition>,
    effect: &'a Option<Plan>,
    blocked: bool,
}

fn compile_programs(
    documents: &[TextDocument],
    ids: &BTreeSet<String>,
    world: &World,
    library: &mut ScriptLibrary,
    geographic_scopes: &BTreeMap<String, Vec<u32>>,
    truce_months: Option<u32>,
    root: &impl ContentView,
) -> Result<BTreeMap<String, MissionProgram>, Error> {
    let mut programs = BTreeMap::new();
    for document in documents {
        let parsed = document.parse()?;
        for (series_key, operator, value) in parsed.reader().fields() {
            let series_id = series_key.read_string();
            if series_id.starts_with('@') {
                continue;
            }
            let series_at = parsed.field_location(&series_key);
            equal(operator, &|message| {
                Error::message(parsed.source(), message).with_location(series_at)
            })?;
            let series = value.read_object().map_err(|_| {
                Error::message(parsed.source(), "expected mission series block")
                    .with_location(series_at)
            })?;
            let mut series_problems = Vec::new();
            for (key, _, value) in series.fields() {
                let name = key.read_string();
                if name == "potential_on_load" {
                    let empty = match value.token() {
                        TextToken::Object { mixed: false, .. } => value
                            .read_object()
                            .is_ok_and(|object| object.fields().next().is_none()),
                        TextToken::Array { mixed: false, .. } => value
                            .read_array()
                            .is_ok_and(|array| array.values().next().is_none()),
                        _ => false,
                    };
                    if !empty {
                        let at = parsed.field_location(&key);
                        series_problems.push(field_problem(
                            parsed.source(),
                            at,
                            "mission series potential_on_load is not evaluated",
                            root,
                        ));
                    }
                } else if !name.starts_with('@')
                    && !matches!(
                        name.as_str(),
                        "slot" | "generic" | "ai" | "has_country_shield" | "potential"
                    )
                    && value.read_object().is_err()
                {
                    let at = parsed.field_location(&key);
                    series_problems.push(field_problem(
                        parsed.source(),
                        at,
                        &format!("unsupported mission series field {name}"),
                        root,
                    ));
                }
            }
            for (key, _, value) in series.fields() {
                let id = key.read_string();
                if id.starts_with('@')
                    || matches!(
                        id.as_str(),
                        "slot"
                            | "generic"
                            | "ai"
                            | "has_country_shield"
                            | "potential"
                            | "potential_on_load"
                    )
                    || value.read_object().is_err()
                {
                    continue;
                }
                let at = parsed.field_location(&key);
                let fail = |message| Error::message(parsed.source(), message).with_location(at);
                let mission = value
                    .read_object()
                    .map_err(|_| fail("expected mission block"))?;
                let mut unresolved = series_problems.clone();
                let mut required_missions = BTreeSet::new();
                let mut trigger = None;
                let mut effect = Some(Plan::new());
                let mut has_semantic_field = !series_problems.is_empty();
                let mut critical_fields = BTreeMap::<String, usize>::new();
                let mut trigger_count = 0;
                let mut effect_count = 0;
                let mut required_count = 0;
                let mut reward_supported = true;

                for (field_key, field_operator, field_value) in mission.fields() {
                    let name = field_key.read_string();
                    let field_at = parsed.field_location(&field_key);
                    if matches!(
                        name.as_str(),
                        "trigger"
                            | "effect"
                            | "required_missions"
                            | "completed_by"
                            | "position"
                            | "_title"
                            | "_desc"
                    ) {
                        *critical_fields.entry(name.clone()).or_default() += 1;
                    }
                    if matches!(name.as_str(), "trigger" | "effect" | "required_missions") {
                        equal(field_operator, &|message| {
                            Error::message(parsed.source(), message).with_location(field_at)
                        })?;
                    }
                    match name.as_str() {
                        "trigger" => {
                            trigger_count += 1;
                            let result = field_value
                                .read_object()
                                .map_err(|_| fail("mission trigger requires a block"));
                            match result {
                                Ok(block) => {
                                    let compiler = mission_compiler(
                                        world,
                                        library,
                                        geographic_scopes,
                                        truce_months,
                                    );
                                    match compiler.compile_condition_reader(
                                        &parsed,
                                        block,
                                        EntityKind::Country,
                                    ) {
                                        Ok(condition) => trigger = Some(condition),
                                        Err(error) => {
                                            unresolved.push(relative_error(&error, root));
                                            trigger = None;
                                        }
                                    }
                                }
                                Err(error) => {
                                    unresolved.push(relative_error(&error, root));
                                    trigger = None;
                                }
                            }
                        }
                        "effect" => {
                            effect_count += 1;
                            let result = field_value
                                .read_object()
                                .map_err(|_| fail("mission effect requires a block"));
                            match result {
                                Ok(block) => {
                                    let compiler = mission_compiler(
                                        world,
                                        library,
                                        geographic_scopes,
                                        truce_months,
                                    );
                                    match compiler.compile_effect_block_reader(
                                        &parsed,
                                        block,
                                        EntityKind::Country,
                                    ) {
                                        Ok(plan) => {
                                            if let Some(effect) = &mut effect {
                                                effect.extend(plan);
                                            }
                                        }
                                        Err(error) => {
                                            unresolved.push(relative_error(&error, root));
                                            reward_supported = false;
                                            effect = None;
                                        }
                                    }
                                }
                                Err(error) => {
                                    unresolved.push(relative_error(&error, root));
                                    reward_supported = false;
                                    effect = None;
                                }
                            }
                        }
                        "required_missions" => {
                            required_count += 1;
                            let fail_required = || {
                                Error::message(
                                    parsed.source(),
                                    "required_missions requires a scalar array",
                                )
                                .with_location(field_at)
                            };
                            let required = if matches!(
                                field_value.token(),
                                TextToken::Array { mixed: false, .. }
                            ) {
                                field_value.read_array().map_err(|_| fail_required()).and_then(
                                    |array| {
                                        let mut values = BTreeSet::new();
                                        for item in array.values() {
                                            let id = item
                                                .read_string()
                                                .map_err(|_| fail_required())?;
                                            if id.is_empty() || !values.insert(id) {
                                                return Err(Error::message(
                                                    parsed.source(),
                                                    "required_missions contains an empty or duplicate identifier",
                                                )
                                                .with_location(field_at));
                                            }
                                        }
                                        Ok(values)
                                    },
                                )
                            } else if matches!(
                                field_value.token(),
                                TextToken::Object { mixed: false, .. }
                            ) && field_value
                                .read_object()
                                .is_ok_and(|object| object.fields().next().is_none())
                            {
                                Ok(BTreeSet::new())
                            } else {
                                Err(fail_required())
                            };
                            match required {
                                Ok(required) => required_missions.extend(required),
                                Err(error) => unresolved.push(relative_error(&error, root)),
                            }
                        }
                        "completed_by" => {
                            has_semantic_field = true;
                            unresolved.push(field_problem(
                                parsed.source(),
                                field_at,
                                "completed_by completion semantics are unsupported",
                                root,
                            ));
                        }
                        "position"
                        | "_title"
                        | "_desc"
                        | "icon"
                        | "ai"
                        | "has_country_shield"
                        | "highlight"
                        | "provinces_to_highlight" => (),
                        name if name.starts_with('@') => (),
                        _ => {
                            has_semantic_field = true;
                            unresolved.push(field_problem(
                                parsed.source(),
                                field_at,
                                &format!("unsupported mission field {name}"),
                                root,
                            ));
                        }
                    }
                }
                if trigger_count == 0 {
                    unresolved.push(field_problem(
                        parsed.source(),
                        at,
                        "mission trigger is missing",
                        root,
                    ));
                }
                if effect_count == 0 {
                    reward_supported = false;
                    unresolved.push(field_problem(
                        parsed.source(),
                        at,
                        "mission effect is missing",
                        root,
                    ));
                }
                if trigger_count > 1 {
                    trigger = None;
                    unresolved.push(field_problem(
                        parsed.source(),
                        at,
                        "duplicate mission trigger fields",
                        root,
                    ));
                }
                if effect_count > 1 {
                    effect = None;
                    reward_supported = false;
                    unresolved.push(field_problem(
                        parsed.source(),
                        at,
                        "duplicate mission effect fields",
                        root,
                    ));
                }
                if required_count > 1 {
                    unresolved.push(field_problem(
                        parsed.source(),
                        at,
                        "duplicate required_missions fields",
                        root,
                    ));
                }
                for (name, count) in &critical_fields {
                    if *count > 1
                        && matches!(
                            name.as_str(),
                            "completed_by" | "position" | "_title" | "_desc"
                        )
                    {
                        unresolved.push(field_problem(
                            parsed.source(),
                            at,
                            &format!("duplicate mission field {name}"),
                            root,
                        ));
                        has_semantic_field = true;
                    }
                }
                if required_count > 0 {
                    has_semantic_field = true;
                }
                for requirement in &required_missions {
                    if !ids.contains(requirement) {
                        unresolved.push(field_problem(
                            parsed.source(),
                            at,
                            &format!("unknown required mission {requirement}"),
                            root,
                        ));
                    }
                }
                let has_execution_rules = trigger_count > 0
                    || effect_count > 0
                    || !required_missions.is_empty()
                    || has_semantic_field;
                let event_effect = effect.as_ref().cloned().unwrap_or_default();
                let mut event_roots = BTreeSet::new();
                crate::script::event_references(&event_effect, &mut event_roots);
                let mut executed_event_roots = BTreeSet::new();
                let mut conditioned_event_roots = BTreeSet::new();
                let mut immediate_event_roots = BTreeSet::new();
                crate::script::mission_event_references(
                    &event_effect,
                    &mut executed_event_roots,
                    &mut conditioned_event_roots,
                    &mut immediate_event_roots,
                );
                let mut normalized_trigger = trigger.clone();
                if let Some(condition) = &mut normalized_trigger {
                    for source_root in root.source_roots() {
                        let prefix =
                            format!("{}{}", source_root.display(), std::path::MAIN_SEPARATOR);
                        relative_reasons(condition, &prefix);
                    }
                }
                let mut normalized_unresolved = unresolved;
                normalized_unresolved.sort();
                normalized_unresolved.dedup();
                programs.insert(
                    id.clone(),
                    MissionProgram {
                        id: id.clone(),
                        series: series_id.clone(),
                        title: format!("{id}_title"),
                        description: Some(format!("{id}_desc")),
                        required_missions,
                        trigger: normalized_trigger,
                        effect,
                        event_roots,
                        executed_event_roots,
                        conditioned_event_roots,
                        immediate_event_roots,
                        unresolved: normalized_unresolved,
                        reward_supported,
                        has_execution_rules,
                    },
                );
            }
        }
    }
    Ok(programs)
}

fn mission_compiler<'a>(
    world: &'a World,
    library: &'a ScriptLibrary,
    geographic_scopes: &BTreeMap<String, Vec<u32>>,
    truce_months: Option<u32>,
) -> ScriptCompiler<'a> {
    ScriptCompiler::for_world(world, library, true)
        .with_mission_ids(world.completions().identifiers.clone())
        .with_geographic_scopes(geographic_scopes.clone())
        .with_truce_months(truce_months)
}

fn mission_event_issue(
    program: &MissionProgram,
    catalog: &augustus_core::EventCatalog,
) -> Option<String> {
    let mut pending: VecDeque<_> = program
        .executed_event_roots
        .iter()
        .map(|id| {
            (
                id.clone(),
                program.immediate_event_roots.contains(id),
                program.conditioned_event_roots.contains(id),
            )
        })
        .collect();
    let mut visited = BTreeSet::new();
    while let Some((id, during_claim, conditional)) = pending.pop_front() {
        if !visited.insert((id.clone(), during_claim, conditional)) {
            continue;
        }
        let Some(definition) = catalog.definitions().get(&id) else {
            return Some(format!("reward references unavailable event {id}"));
        };
        if catalog
            .unavailable_options()
            .get(&id)
            .is_some_and(|options| !options.is_empty())
        {
            return Some(format!(
                "reward event {id} contains unsupported option effects"
            ));
        }
        if (conditional
            && catalog
                .conditions()
                .get(&id)
                .is_some_and(condition_is_unavailable))
            || catalog
                .option_conditions()
                .get(&id)
                .is_some_and(|conditions| conditions.values().any(condition_is_unavailable))
        {
            return Some(format!("reward event {id} has an unsupported condition"));
        }
        if catalog
            .option_weights()
            .get(&id)
            .is_some_and(json_contains_unavailable)
            || catalog
                .pictures()
                .get(&id)
                .is_some_and(json_contains_unavailable)
        {
            return Some(format!(
                "reward event {id} has unsupported option-weight or picture conditions"
            ));
        }
        if during_claim
            && (catalog
                .option_weights()
                .get(&id)
                .is_some_and(|value| json_value_reads_completion(value, &program.id))
                || catalog
                    .pictures()
                    .get(&id)
                    .is_some_and(|value| json_value_reads_completion(value, &program.id)))
        {
            return Some(format!(
                "reward event {id} reads this mission's mission_completed status; reward ordering is unresolved"
            ));
        }
        if during_claim && plan_reads_completion(&definition.immediate, &program.id) {
            return Some(format!(
                "reward event {id} reads this mission's mission_completed status; reward ordering is unresolved"
            ));
        }
        let mut references = BTreeSet::new();
        let mut conditional_references = BTreeSet::new();
        let mut immediate_references = BTreeSet::new();
        crate::script::mission_event_references(
            &definition.immediate,
            &mut references,
            &mut conditional_references,
            &mut immediate_references,
        );
        pending.extend(references.into_iter().map(|event| {
            (
                event.clone(),
                during_claim && immediate_references.contains(&event),
                conditional_references.contains(&event),
            )
        }));
        for plan in definition
            .options
            .iter()
            .map(|option| &option.effects)
            .chain(std::iter::once(&definition.after))
        {
            let mut references = BTreeSet::new();
            let mut conditional_references = BTreeSet::new();
            let mut immediate_references = BTreeSet::new();
            crate::script::mission_event_references(
                plan,
                &mut references,
                &mut conditional_references,
                &mut immediate_references,
            );
            pending.extend(references.into_iter().map(|event| {
                (
                    event.clone(),
                    false,
                    conditional_references.contains(&event),
                )
            }));
        }
        if catalog.conditions().get(&id).is_some_and(|condition| {
            during_claim && conditional && condition_reads_completion(condition, &program.id)
        }) || catalog
            .option_conditions()
            .get(&id)
            .is_some_and(|conditions| {
                during_claim
                    && conditions
                        .values()
                        .any(|condition| condition_reads_completion(condition, &program.id))
            })
        {
            return Some(format!(
                "reward event {id} reads this mission's mission_completed status; reward ordering is unresolved"
            ));
        }
    }
    None
}

fn plan_reads_completion(plan: &Plan, id: &str) -> bool {
    json_value_reads_completion(plan, id)
}

fn condition_reads_completion(condition: &Condition, id: &str) -> bool {
    json_value_reads_completion(condition, id)
}

fn json_value_reads_completion(value: &impl Serialize, id: &str) -> bool {
    serde_json::to_value(value)
        .ok()
        .is_some_and(|value| json_reads_completion(&value, id))
}

fn json_contains_unavailable(value: &impl Serialize) -> bool {
    serde_json::to_value(value)
        .ok()
        .is_some_and(|value| json_has_key(&value, "Unavailable"))
}

fn json_reads_completion(value: &serde_json::Value, id: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            if fields.contains_key("Preview") || fields.contains_key("Schedule") {
                return false;
            }
            fields.get("CompletionRecorded").is_some_and(|record| {
                record.get("id").and_then(serde_json::Value::as_str) == Some(id)
            }) || fields
                .values()
                .any(|value| json_reads_completion(value, id))
        }
        serde_json::Value::Array(values) => {
            values.iter().any(|value| json_reads_completion(value, id))
        }
        _ => false,
    }
}

fn condition_is_unavailable(condition: &Condition) -> bool {
    serde_json::to_value(condition)
        .ok()
        .is_some_and(|value| json_has_key(&value, "Unavailable"))
}

fn json_has_key(value: &serde_json::Value, name: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields.contains_key(name) || fields.values().any(|value| json_has_key(value, name))
        }
        serde_json::Value::Array(values) => values.iter().any(|value| json_has_key(value, name)),
        _ => false,
    }
}

fn field_problem(
    path: &std::path::Path,
    at: crate::SourceLocation,
    message: &str,
    root: &impl ContentView,
) -> String {
    let mut path = path.to_string_lossy().into_owned();
    for source_root in root.source_roots() {
        let prefix = format!("{}{}", source_root.display(), std::path::MAIN_SEPARATOR);
        path = path.replace(&prefix, "");
    }
    format!("{path}:{}:{}: {message}", at.line, at.column)
}

fn relative_error(error: &Error, root: &impl ContentView) -> String {
    let mut path = error.path().to_string_lossy().into_owned();
    for source_root in root.source_roots() {
        let prefix = format!("{}{}", source_root.display(), std::path::MAIN_SEPARATOR);
        path = path.replace(&prefix, "");
    }
    let location = error
        .location()
        .map(|at| format!(":{}:{}", at.line, at.column))
        .unwrap_or_default();
    format!("{path}{location}: {}", error.detail())
}

impl VanillaWorld {
    /// Evaluates current mission requirements without refreshing series selection,
    /// executing effects, consuming randomness, or changing the snapshot.
    pub fn country_missions(&self, country: &str) -> Result<VanillaCountryMissions, Error> {
        let entity = self
            .initial
            .country_entity(country)
            .ok_or_else(|| Error::message("missions", format!("unknown country {country}")))?;
        let world = self.initial.world();
        let selection_known = world.objectives().entities.contains_key(&entity);
        let completions_known = world.completions().entities.contains_key(&entity);
        let mut missions = Vec::with_capacity(self.mission_programs.len());
        for program in self.mission_programs.values() {
            let selected = selection_known
                .then(|| world.objective_available(entity, &program.id).ok())
                .flatten();
            let completed = completions_known
                .then(|| world.completion_recorded(entity, &program.id).ok())
                .flatten();
            let requirements_met = program.trigger.as_ref().and_then(|trigger| {
                let mut met = trigger.evaluate(world, entity).ok()?;
                for required in &program.required_missions {
                    met &= world.completion_recorded(entity, required).ok()?;
                }
                Some(met)
            });
            let mut unresolved = program.unresolved.clone();
            if !selection_known {
                unresolved.push("mission selection is unknown".into());
            }
            if !completions_known {
                unresolved.push("mission completions are unknown".into());
            }
            if requirements_met.is_none() {
                unresolved.push("mission requirements could not be evaluated".into());
            }
            let claimable = selected == Some(true)
                && completed == Some(false)
                && requirements_met == Some(true)
                && program.reward_supported
                && unresolved.is_empty();
            missions.push(VanillaMissionView {
                id: program.id.clone(),
                title: program.title.clone(),
                description: program.description.clone(),
                required_missions: program.required_missions.clone(),
                selected,
                completed,
                requirements_met,
                reward_supported: program.reward_supported,
                claimable,
                unresolved,
            });
        }
        Ok(VanillaCountryMissions {
            country: country.into(),
            selection_known,
            completions_known,
            missions,
        })
    }

    /// Revalidates the live claim gate, executes rewards, and then records completion.
    /// All world changes commit together; a failed reward leaves the live world usable.
    pub fn claim_mission(&mut self, country: &str, id: &str) -> Result<(), Error> {
        let snapshot = self.country_missions(country)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == id)
            .ok_or_else(|| Error::message("missions", format!("unknown mission {id}")))?;
        if !mission.claimable {
            return Err(Error::message(
                "missions",
                format!("mission {id} is not claimable"),
            ));
        }
        let entity = self
            .initial
            .country_entity(country)
            .expect("country was validated");
        let program = self
            .mission_programs
            .get(id)
            .expect("mission view came from compiled programs");
        let mut plan = program.effect.clone().ok_or_else(|| {
            Error::message("missions", format!("mission {id} has no executable reward"))
        })?;
        let mut staged = self.initial.clone();
        plan.push(Action::RecordCompletion { id: id.into() });
        staged
            .execute(entity, &plan)
            .map_err(|error| Error::caused_by("mission reward", error))?;
        self.initial = staged;
        Ok(())
    }
}

/// Index names only. Tree eligibility, rewards and series selection are separate.
pub(crate) fn identifiers(
    root: &impl ContentView,
    files: &[FileEntry],
) -> Result<BTreeSet<String>, Error> {
    let mut identifiers = BTreeSet::new();
    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "missions"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        for (key, operator, value) in parsed.reader().fields() {
            if key.read_string().starts_with('@') {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(operator, &fail)?;
            let series = value
                .read_object()
                .map_err(|_| fail("expected mission series block"))?;
            for (key, operator, value) in series.fields() {
                let id = key.read_string();
                if id.starts_with('@') || matches!(id.as_str(), "potential" | "potential_on_load") {
                    continue;
                }
                if value.read_object().is_ok() {
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(operator, &fail)?;
                    if id.is_empty() {
                        return Err(fail("mission declaration requires identifier"));
                    }
                    if !identifiers.insert(id.clone()) {
                        return Err(fail(&format!("duplicate mission identifier {id}")));
                    }
                }
            }
        }
    }
    Ok(identifiers)
}

fn relative_reasons(condition: &mut Condition, prefix: &str) {
    match condition {
        Condition::If {
            condition,
            then,
            otherwise,
        } => {
            for child in [condition.as_mut(), then.as_mut(), otherwise.as_mut()] {
                relative_reasons(child, prefix);
            }
        }
        Condition::Unavailable(reason) => *reason = reason.replace(prefix, ""),
        Condition::All(children) | Condition::Any(children) => {
            for child in children {
                relative_reasons(child, prefix);
            }
        }
        Condition::Not(child) => relative_reasons(child, prefix),
        _ => (),
    }
}

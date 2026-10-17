// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::BTreeMap,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use augustus_core::{EntityKind, EventDecisionSystem, PendingEvent, World};
use sha2::{Digest, Sha256};

use crate::Eu4Random;

#[derive(Debug)]
pub(crate) struct Eu4EventDecisionSystem {
    country_indices: BTreeMap<String, i32>,
    clock_origin: Instant,
}

impl Eu4EventDecisionSystem {
    pub(crate) fn new(
        country_order: &[String],
        clock_origin: Instant,
    ) -> Result<Self, augustus_core::Error> {
        let mut country_indices = BTreeMap::new();
        for (index, tag) in country_order.iter().enumerate() {
            let index = i32::try_from(index).map_err(|_| augustus_core::Error::Overflow)?;
            if country_indices.insert(tag.clone(), index).is_some() {
                return Err(augustus_core::Error::Invalid(
                    "duplicate country registration order".into(),
                ));
            }
        }
        Ok(Self {
            country_indices,
            clock_origin,
        })
    }
}

impl EventDecisionSystem for Eu4EventDecisionSystem {
    fn content_key(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"eu4-event-decisions:1\0");
        for (tag, index) in &self.country_indices {
            digest.update(tag.as_bytes());
            digest.update(index.to_le_bytes());
        }
        let fingerprint = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("eu4-event-decisions:1:{fingerprint}")
    }

    fn select(
        &self,
        world: &mut World,
        pending: &PendingEvent,
    ) -> Result<Option<String>, augustus_core::Error> {
        if world
            .event_catalog()
            .unavailable_options()
            .get(&pending.event)
            .is_some_and(|options| !options.is_empty())
        {
            return Ok(None);
        }
        let entity = world.entity(pending.current)?;
        if entity.kind != EntityKind::Country {
            return Ok(None);
        }
        let country_index = self
            .country_indices
            .get(&entity.name)
            .copied()
            .ok_or_else(|| {
                augustus_core::Error::Invalid(format!(
                    "country {} has no EU4 registration index",
                    entity.name
                ))
            })?;
        let definition = world
            .event_catalog()
            .definitions()
            .get(&pending.event)
            .ok_or_else(|| {
                augustus_core::Error::Invalid(format!("unknown event {}", pending.event))
            })?;
        let clock_value = self.clock_origin.elapsed().as_secs_f64();
        let time64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        let seed = resolve_ai_event_seed(
            world.eu4_event_seed(),
            country_index,
            0,
            clock_value,
            time64,
        );
        let selection = select_ai_event_option_detail(definition.options.len(), seed, |index| {
            world
                .event_option_weight(pending, index)?
                .map(|weight| i32::try_from(weight).map_err(|_| augustus_core::Error::Overflow))
                .transpose()
        })?;
        if let Some(index) = selection.option {
            return Ok(definition
                .options
                .get(index)
                .map(|option| option.id.clone()));
        }
        if !selection.has_positive_total {
            return Ok(definition.options.first().map(|option| option.id.clone()));
        }
        Ok(None)
    }
}

/// Evaluates an event option's integer weight and ordered conditional modifiers.
pub fn evaluate_ai_event_weight(initial: i32, modifiers: &[(i32, bool)], factor: i32) -> i32 {
    let mut value = initial;

    for &(modifier, applies) in modifiers {
        if applies {
            value = (i64::from(value) * i64::from(modifier) / 1_000) as i32;
        }
        if value == 0 {
            break;
        }
        if value.wrapping_add(300_000) as u32 > 0x3b9f_5de0 {
            value = 300_000_000;
            break;
        }
    }

    (i64::from(value) * i64::from(factor) / 1_000) as i32
}

/// Selects an option using EU4's local event-choice random stream.
///
/// `weight` returns `None` for an ineligible option and otherwise returns its evaluated
/// signed 32-bit weight. It is called once to total the weights and again to choose, matching
/// the game's two-pass evaluation. `seed` is the already-resolved local seed; callers are
/// responsible for supplying the correct event scope and seed source.
pub fn select_ai_event_option<E>(
    option_count: usize,
    seed: i32,
    weight: impl FnMut(usize) -> Result<Option<i32>, E>,
) -> Result<Option<usize>, E> {
    Ok(select_ai_event_option_detail(option_count, seed, weight)?.option)
}

struct AiSelection {
    option: Option<usize>,
    has_positive_total: bool,
}

fn select_ai_event_option_detail<E>(
    option_count: usize,
    seed: i32,
    mut weight: impl FnMut(usize) -> Result<Option<i32>, E>,
) -> Result<AiSelection, E> {
    let mut total = 0_i32;
    for index in 0..option_count {
        if let Some(value) = weight(index)? {
            total = total.wrapping_add(normalize_selection_weight(value));
        }
    }
    if total <= 0 {
        return Ok(AiSelection {
            option: None,
            has_positive_total: false,
        });
    }

    let denominator = (i64::from(total) * 1_000 / 100_000) as i32;
    let mut random = Eu4Random::seeded(seed as u32);
    let draw = (random.next_u32() as i32) % 100;
    let mut cumulative = 0_i32;

    for index in 0..option_count {
        let Some(value) = weight(index)? else {
            continue;
        };
        let value = normalize_selection_weight(value);
        let ratio = if denominator == 0 {
            -1
        } else if value.wrapping_add(0x0018_9373) as u32 <= 0x0039_580d {
            i64::from(value.wrapping_mul(1_000)) / i64::from(denominator)
        } else {
            i64::from(value) * 1_000 / i64::from(denominator)
        } as i32;

        cumulative = cumulative.wrapping_add(ratio / 1_000);
        if draw <= cumulative {
            return Ok(AiSelection {
                option: Some(index),
                has_positive_total: true,
            });
        }
    }

    Ok(AiSelection {
        option: None,
        has_positive_total: true,
    })
}

fn resolve_ai_event_seed(
    fixed_seed: Option<u32>,
    country_index: i32,
    province_id: i32,
    clock_value: f64,
    time64_seconds: u64,
) -> i32 {
    let scope_index = country_index.wrapping_add(province_id);
    if let Some(seed) = fixed_seed {
        return seed.wrapping_add(scope_index as u32) as i32;
    }
    if scope_index == 0 {
        return time64_seconds as u32 as i32;
    }
    cvttsd2si(clock_value + f64::from(scope_index))
}

fn cvttsd2si(value: f64) -> i32 {
    if !value.is_finite() || value < f64::from(i32::MIN) || value >= 2_147_483_648.0 {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}

fn normalize_selection_weight(value: i32) -> i32 {
    let divided = value as f32 / 1_000.0;
    let scaled = divided * 1_000.0;
    let adjusted = scaled + 0.5;
    let converted = if divided < 0.0 {
        adjusted.floor()
    } else {
        adjusted.trunc()
    };

    if !(-2_147_483_648.0..2_147_483_648.0).contains(&converted) {
        i32::MIN
    } else {
        converted as i32
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Instant};

    use augustus_core::{
        CountryController, EntityKind, EventCatalog, EventDefinition, EventOption, PendingEvent,
        PendingEventId, Schema, World,
    };

    use super::{Eu4EventDecisionSystem, cvttsd2si, resolve_ai_event_seed};

    #[test]
    fn local_seed_uses_fixed_index_or_clock_and_time64_fallback() {
        assert_eq!(
            resolve_ai_event_seed(Some(8_351_375), 303, 0, 0.0, 0),
            8_351_678
        );
        assert_eq!(resolve_ai_event_seed(None, 303, 0, 30.495_605, 0), 333);
        assert_eq!(resolve_ai_event_seed(None, 0, 0, 30.0, 4_294_967_300), 4);
        assert_eq!(resolve_ai_event_seed(Some(u32::MAX), 1, 0, 0.0, 0), 0);
    }

    #[test]
    fn clock_conversion_matches_truncating_sse_conversion() {
        assert_eq!(cvttsd2si(3.9), 3);
        assert_eq!(cvttsd2si(-3.9), -3);
        assert_eq!(cvttsd2si(f64::NAN), i32::MIN);
        assert_eq!(cvttsd2si(2_147_483_648.0), i32::MIN);
        assert_eq!(cvttsd2si(-2_147_483_648.0), i32::MIN);
    }

    #[test]
    fn registered_country_seed_selects_the_observed_ai_option() {
        let mut world = World::new(Schema::new(std::iter::empty::<&str>()).unwrap());
        let country = world.add_entity("MAM", EntityKind::Country);
        let options = (0..2)
            .map(|index| EventOption {
                id: index.to_string(),
                effects: Vec::new(),
            })
            .collect();
        let mut country_order: Vec<_> = (0..303).map(|index| format!("T{index:02}")).collect();
        country_order.push("MAM".into());
        let system = Eu4EventDecisionSystem::new(&country_order, Instant::now()).unwrap();
        let catalog = EventCatalog::new([EventDefinition {
            id: "event".into(),
            scope: EntityKind::Country,
            immediate: Vec::new(),
            options,
            after: Vec::new(),
        }])
        .unwrap();
        let catalog = catalog.with_decision_system(Arc::new(system)).unwrap();
        world.set_event_catalog(catalog).unwrap();
        world
            .set_country_controller(country, CountryController::Ai)
            .unwrap();
        world.set_eu4_event_seed(Some(8_351_375));
        world.trigger_event("event", country).unwrap();
        assert_eq!(world.queued_event_choices()[0].option, "1");
    }

    #[test]
    fn unsupported_province_scope_does_not_consume_an_ai_choice() {
        let mut world = World::new(Schema::new(std::iter::empty::<&str>()).unwrap());
        let province = world.add_entity("1", EntityKind::Province);
        let catalog = EventCatalog::new([EventDefinition {
            id: "event".into(),
            scope: EntityKind::Province,
            immediate: Vec::new(),
            options: vec![EventOption {
                id: "0".into(),
                effects: Vec::new(),
            }],
            after: Vec::new(),
        }])
        .unwrap();
        world.set_event_catalog(catalog).unwrap();
        let pending = PendingEvent {
            id: PendingEventId(1),
            event: "event".into(),
            current: province,
            day: 0,
        };
        let system = Eu4EventDecisionSystem::new(&[], Instant::now()).unwrap();

        assert_eq!(
            augustus_core::EventDecisionSystem::select(&system, &mut world, &pending).unwrap(),
            None
        );
    }
}

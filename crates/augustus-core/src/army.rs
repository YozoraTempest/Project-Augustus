// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

const INITIAL_REGIMENT_STRENGTH_MILLI: i64 = 1_000;
const INITIAL_REGIMENT_MORALE_MILLI: i64 = 1_000;

/// Inputs for the verified EU4 unit-production quantity stage.
///
/// The compatibility layer resolves the category, cap switch, category
/// maximum, existing regiments, and queued regiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaUnitProductionQuantityInput {
    pub effect_amount_milli: i32,
    pub quantity_modifier_milli: i32,
    pub category: i32,
    pub category_cap_enabled: bool,
    pub maximum_units: i32,
    pub existing_units: u16,
    pub pending_units: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaUnitProductionQuantityResult {
    pub scaled_effect_amount_milli: i32,
    pub uncapped_quantity: i32,
    pub remaining_cap: Option<i32>,
    pub quantity: Option<i32>,
}

/// Calculates the production quantity and the verified category-cap gate.
///
/// The signed 64-bit product is divided by 1000, narrowed to 32 bits, divided
/// by 1000 again, and raised to at least one. Categories 2 through 22 apply
/// the native cap only when its input flag is enabled.
pub fn calculate_vanilla_unit_production_quantity(
    input: VanillaUnitProductionQuantityInput,
) -> VanillaUnitProductionQuantityResult {
    let product = i64::from(input.effect_amount_milli) * i64::from(input.quantity_modifier_milli);
    let scaled_effect_amount_milli = (product / 1_000) as i32;
    let uncapped_quantity = (scaled_effect_amount_milli / 1_000).max(1);
    let remaining_cap = if input.category_cap_enabled && (2..=22).contains(&input.category) {
        Some(
            input
                .maximum_units
                .wrapping_sub(i32::from(input.existing_units))
                .wrapping_sub(input.pending_units as i32),
        )
    } else {
        None
    };
    let quantity = match remaining_cap {
        Some(cap) if cap < 1 => None,
        Some(cap) => Some(uncapped_quantity.min(cap)),
        None => Some(uncapped_quantity),
    };

    VanillaUnitProductionQuantityResult {
        scaled_effect_amount_milli,
        uncapped_quantity,
        remaining_cap,
        quantity,
    }
}

/// Inputs for the shared land and naval force limit calculation stage.
///
/// The EU4 adapter resolves the base value and the two modifier contributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaForceLimitInput {
    pub base_milli: i32,
    pub flat_modifier_milli: i32,
    pub percentage_modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaForceLimitResult {
    pub base_with_flat_modifier_milli: i32,
    pub scale_factor_milli: i32,
    pub scaled_limit_milli: i32,
    pub whole_unit_limit_milli: i32,
    pub force_limit_milli: i32,
}

/// Applies force limit modifiers, truncates to a whole unit, and enforces a
/// minimum limit of one unit.
pub fn calculate_vanilla_force_limit(input: VanillaForceLimitInput) -> VanillaForceLimitResult {
    let base_with_flat_modifier_milli = input.base_milli.wrapping_add(input.flat_modifier_milli);
    let scale_factor_milli = input.percentage_modifier_milli.wrapping_add(1_000);
    let scaled_limit_milli =
        (i64::from(base_with_flat_modifier_milli) * i64::from(scale_factor_milli) / 1_000) as i32;
    let whole_unit_limit_milli = (scaled_limit_milli / 1_000) * 1_000;
    let force_limit_milli = whole_unit_limit_milli.max(1_000);

    VanillaForceLimitResult {
        base_with_flat_modifier_milli,
        scale_factor_milli,
        scaled_limit_milli,
        whole_unit_limit_milli,
        force_limit_milli,
    }
}

/// Inputs for the verified morale stage used while completing a unit.
///
/// The compatibility layer resolves the country's two morale bases, the
/// unit-specific base selector, the global factor, and the special-owner
/// override condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaConstructionMoraleInput {
    pub regular_base_morale_milli: i32,
    pub alternate_base_morale_milli: i32,
    pub alternate_base_selected: bool,
    pub construction_factor_milli: i32,
    pub override_with_regular_base: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaConstructionMoraleResult {
    pub selected_base_morale_milli: i32,
    pub capped_factor_milli: i32,
    pub scaled_morale_milli: i32,
    pub morale_milli: i32,
}

/// Calculates the verified unit-construction morale stage.
///
/// The factor has an upper bound of 1000 and no lower bound. Multiplication
/// uses signed 64-bit intermediates, division truncates toward zero, and the
/// narrowed result is clamped at zero. The special-owner override selects the
/// regular base directly after the scaled value has been calculated.
pub fn calculate_vanilla_construction_morale(
    input: VanillaConstructionMoraleInput,
) -> VanillaConstructionMoraleResult {
    let selected_base_morale_milli = if input.alternate_base_selected {
        input.alternate_base_morale_milli
    } else {
        input.regular_base_morale_milli
    };
    let capped_factor_milli = input.construction_factor_milli.min(1_000);
    let scaled_morale_milli =
        ((i64::from(selected_base_morale_milli) * i64::from(capped_factor_milli) / 1_000) as i32)
            .max(0);
    let morale_milli = if input.override_with_regular_base {
        input.regular_base_morale_milli.max(0)
    } else {
        scaled_morale_milli
    };

    VanillaConstructionMoraleResult {
        selected_base_morale_milli,
        capped_factor_milli,
        scaled_morale_milli,
        morale_milli,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArmyId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegimentId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regiment {
    pub id: RegimentId,
    pub unit_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morale_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_province: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RegimentState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub strength_milli: Option<i64>,
    pub morale_milli: Option<i64>,
    pub home_province: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Army {
    pub id: ArmyId,
    pub owner: EntityId,
    pub location: EntityId,
    pub name: String,
    pub regiments: Vec<Regiment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct Armies {
    next_army_id: u64,
    next_regiment_id: u64,
    armies: Vec<Army>,
}

impl Armies {
    pub(crate) fn is_empty(&self) -> bool {
        self.armies.is_empty()
    }

    pub(crate) fn validate_regiment_types(regiments: &[String]) -> Result<(), Error> {
        if regiments.is_empty()
            || regiments.iter().any(|unit_type| {
                unit_type.trim().is_empty()
                    || unit_type.bytes().any(|byte| byte.is_ascii_whitespace())
            })
        {
            return Err(Error::Invalid("invalid army regiment list".into()));
        }
        Ok(())
    }

    pub(crate) fn validate_categories(categories: &[String]) -> Result<(), Error> {
        if categories.is_empty()
            || categories.iter().any(|category| {
                category.trim().is_empty()
                    || category.bytes().any(|byte| byte.is_ascii_whitespace())
            })
        {
            return Err(Error::Invalid("invalid army unit categories".into()));
        }
        Ok(())
    }

    fn create(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
        states: Option<&[RegimentState]>,
    ) -> Result<ArmyId, Error> {
        Self::validate_regiment_types(unit_types)?;
        if states.is_some_and(|states| !states.is_empty() && states.len() != unit_types.len()) {
            return Err(Error::Invalid(
                "army regiment state count does not match regiment count".into(),
            ));
        }
        if let Some(states) = states
            && states.iter().any(|state| {
                state.strength_milli.is_some_and(|strength| strength < 0)
                    || state.morale_milli.is_some_and(|morale| morale < 0)
            })
        {
            return Err(Error::Invalid("invalid regiment strength or morale".into()));
        }

        let id = ArmyId(self.next_army_id);
        let next_army_id = self.next_army_id.checked_add(1).ok_or(Error::Overflow)?;
        let next_regiment_id = self
            .next_regiment_id
            .checked_add(u64::try_from(unit_types.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let regiments = unit_types
            .iter()
            .enumerate()
            .map(|(offset, unit_type)| {
                // Only native construction supplies defaults; external state may stay unknown.
                let state = states.and_then(|states| states.get(offset));
                let has_external_state = states.is_some();
                Regiment {
                    id: RegimentId(self.next_regiment_id + offset as u64),
                    unit_type: unit_type.clone(),
                    name: state.and_then(|state| state.name.clone()),
                    strength_milli: if has_external_state {
                        state.and_then(|state| state.strength_milli)
                    } else {
                        Some(INITIAL_REGIMENT_STRENGTH_MILLI)
                    },
                    morale_milli: if has_external_state {
                        state.and_then(|state| state.morale_milli)
                    } else {
                        Some(INITIAL_REGIMENT_MORALE_MILLI)
                    },
                    home_province: state.and_then(|state| state.home_province),
                }
            })
            .collect();

        self.armies.push(Army {
            id,
            owner,
            location,
            name: name.into(),
            regiments,
        });
        self.next_army_id = next_army_id;
        self.next_regiment_id = next_regiment_id;
        Ok(id)
    }

    pub(crate) fn validate(&self, entities: &[crate::Entity]) -> Result<(), Error> {
        let mut army_ids = BTreeSet::new();
        let mut regiment_ids = BTreeSet::new();
        for army in &self.armies {
            if !army_ids.insert(army.id) || army.id.0 >= self.next_army_id {
                return Err(Error::Invalid("invalid army identifier".into()));
            }
            let owner = entities
                .get(army.owner.0)
                .ok_or_else(|| Error::Invalid("army references unknown owner".into()))?;
            let location = entities
                .get(army.location.0)
                .ok_or_else(|| Error::Invalid("army references unknown location".into()))?;
            if owner.kind != EntityKind::Country || location.kind != EntityKind::Province {
                return Err(Error::Invalid("invalid army owner or location".into()));
            }
            if army.regiments.is_empty() {
                return Err(Error::Invalid("army has no regiments".into()));
            }
            for regiment in &army.regiments {
                if !regiment_ids.insert(regiment.id) || regiment.id.0 >= self.next_regiment_id {
                    return Err(Error::Invalid("invalid regiment identifier".into()));
                }
                Self::validate_regiment_types(std::slice::from_ref(&regiment.unit_type))?;
                if regiment.strength_milli.is_some_and(|strength| strength < 0)
                    || regiment.morale_milli.is_some_and(|morale| morale < 0)
                {
                    return Err(Error::Invalid("invalid regiment strength or morale".into()));
                }
                if let Some(home) = regiment.home_province
                    && entities
                        .get(home.0)
                        .is_none_or(|entity| entity.kind != EntityKind::Province)
                {
                    return Err(Error::Invalid(
                        "regiment references an invalid home province".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn regiment_mut(
        &mut self,
        army_id: ArmyId,
        regiment_id: RegimentId,
    ) -> Result<&mut Regiment, Error> {
        let army = self
            .armies
            .iter_mut()
            .find(|army| army.id == army_id)
            .ok_or_else(|| Error::Invalid("unknown army identifier".into()))?;
        army.regiments
            .iter_mut()
            .find(|regiment| regiment.id == regiment_id)
            .ok_or_else(|| Error::Invalid("unknown regiment identifier".into()))
    }
}

impl World {
    pub fn armies(&self) -> &[Army] {
        &self.armies.armies
    }

    pub fn set_regiment_morale_milli(
        &mut self,
        army_id: ArmyId,
        regiment_id: RegimentId,
        morale_milli: i32,
    ) -> Result<(), Error> {
        self.require_running()?;
        let regiment = self.armies.regiment_mut(army_id, regiment_id)?;
        regiment.morale_milli = Some(i64::from(morale_milli.max(0)));
        Ok(())
    }

    pub fn apply_regiment_strength_loss_milli(
        &mut self,
        army_id: ArmyId,
        regiment_id: RegimentId,
        loss_milli: i32,
        maximum_strength_milli: i32,
    ) -> Result<(), Error> {
        self.require_running()?;
        if maximum_strength_milli < 0 {
            return Err(Error::Invalid(
                "regiment maximum strength must not be negative".into(),
            ));
        }
        let regiment = self.armies.regiment_mut(army_id, regiment_id)?;
        let strength = regiment
            .strength_milli
            .ok_or_else(|| Error::Invalid("regiment strength is unknown".into()))?;
        let strength = i32::try_from(strength)
            .map_err(|_| Error::Invalid("regiment strength exceeds native range".into()))?;
        let remaining = strength.wrapping_sub(loss_milli);
        let remaining = if remaining < 0 {
            0
        } else {
            remaining.min(maximum_strength_milli)
        };
        regiment.strength_milli = Some(i64::from(remaining));
        Ok(())
    }

    pub fn create_army(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
    ) -> Result<ArmyId, Error> {
        self.create_army_internal(owner, location, name, unit_types, None)
    }

    pub fn create_army_with_regiment_states(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
        states: &[RegimentState],
    ) -> Result<ArmyId, Error> {
        self.create_army_internal(owner, location, name, unit_types, Some(states))
    }

    fn create_army_internal(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        unit_types: &[String],
        states: Option<&[RegimentState]>,
    ) -> Result<ArmyId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("army owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("army location must be a province".into()));
        }
        if let Some(states) = states {
            for state in states {
                if let Some(home) = state.home_province
                    && self.entity(home)?.kind != EntityKind::Province
                {
                    return Err(Error::Invalid(
                        "regiment home province must be a province".into(),
                    ));
                }
            }
        }
        self.armies
            .create(owner, location, name, unit_types, states)
    }

    pub fn create_army_from_categories(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: &str,
        categories: &[String],
    ) -> Result<Option<ArmyId>, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("army owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("army location must be a province".into()));
        }
        Armies::validate_categories(categories)?;

        let mut unit_types = Vec::with_capacity(categories.len());
        for category in categories {
            let Some(unit_type) = self.selected_unit(owner, category) else {
                return Ok(None);
            };
            unit_types.push(unit_type.to_owned());
        }

        self.create_army(owner, location, name, &unit_types)
            .map(Some)
    }
}

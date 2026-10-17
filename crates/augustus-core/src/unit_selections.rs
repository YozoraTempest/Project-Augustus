// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitSelections {
    countries: BTreeMap<EntityId, BTreeMap<String, String>>,
}

impl UnitSelections {
    pub(crate) fn is_empty(&self) -> bool {
        self.countries.is_empty()
    }

    pub(crate) fn get(&self, country: EntityId, category: &str) -> Option<&str> {
        self.countries
            .get(&country)?
            .get(category)
            .map(String::as_str)
    }

    pub(crate) fn set(&mut self, country: EntityId, category: &str, unit_type: &str) {
        self.countries
            .entry(country)
            .or_default()
            .insert(category.into(), unit_type.into());
    }

    pub(crate) fn clear(&mut self, country: EntityId, category: &str) {
        if let Some(selections) = self.countries.get_mut(&country) {
            selections.remove(category);
            if selections.is_empty() {
                self.countries.remove(&country);
            }
        }
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (&country, selections) in &self.countries {
            if world.entity(country)?.kind != EntityKind::Country || selections.is_empty() {
                return Err(Error::Invalid("invalid country unit selection".into()));
            }
            for (category, unit_type) in selections {
                validate_identifier(category, "unit category")?;
                validate_identifier(unit_type, "unit template")?;
                let template = world
                    .unit_templates()
                    .definitions
                    .get(unit_type)
                    .ok_or_else(|| Error::Invalid("selected unit template is unknown".into()))?;
                if template.category != *category {
                    return Err(Error::Invalid(
                        "selected unit template has the wrong category".into(),
                    ));
                }
                let group = world
                    .affiliations()
                    .member("unit_type", country)
                    .ok_or_else(|| Error::Invalid("country unit type is unknown".into()))?;
                let mut template_group = None;
                for property in template
                    .properties
                    .iter()
                    .filter(|property| property.name == "unit_type")
                {
                    if template_group.is_some_and(|previous| previous != property.value) {
                        return Err(Error::Invalid(
                            "unit template has conflicting unit groups".into(),
                        ));
                    }
                    template_group = Some(property.value.as_str());
                }
                if template_group != Some(group) {
                    return Err(Error::Invalid(
                        "selected unit template has the wrong unit group".into(),
                    ));
                }
                if !is_enabled(world, country, unit_type)? {
                    return Err(Error::Invalid(
                        "selected unit template is not enabled by current technology".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn validate_identifier(value: &str, label: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(Error::Invalid(format!("invalid {label} identifier")));
    }
    Ok(())
}

pub(crate) fn is_enabled(world: &World, country: EntityId, unit_type: &str) -> Result<bool, Error> {
    let Some(level) = world
        .technology()
        .countries
        .get(&country)
        .and_then(|levels| levels.get("military"))
        .copied()
    else {
        return Ok(false);
    };
    let Some(levels) = world.technology().tables.get("military") else {
        return Ok(false);
    };
    let available = levels
        .get(..=usize::from(level))
        .ok_or_else(|| Error::Invalid("country military technology is out of range".into()))?;
    Ok(available.iter().any(|technology| {
        technology
            .enabled_units
            .iter()
            .any(|name| name == unit_type)
    }))
}

impl World {
    pub fn available_unit_templates(
        &self,
        country: EntityId,
    ) -> Result<BTreeMap<String, Vec<String>>, Error> {
        self.require_running()?;
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "unit template query requires a country".into(),
            ));
        }
        let Some(group) = self.affiliations().member("unit_type", country) else {
            return Ok(BTreeMap::new());
        };
        let Some(level) = self
            .technology()
            .countries
            .get(&country)
            .and_then(|levels| levels.get("military"))
            .copied()
        else {
            return Ok(BTreeMap::new());
        };
        let Some(levels) = self.technology().tables.get("military") else {
            return Ok(BTreeMap::new());
        };
        let available = levels
            .get(..=usize::from(level))
            .ok_or_else(|| Error::Invalid("country military technology is out of range".into()))?;
        let mut result = BTreeMap::<String, Vec<String>>::new();
        for technology in available {
            for unit_type in &technology.enabled_units {
                let Some(template) = self.unit_templates().definitions.get(unit_type) else {
                    continue;
                };
                let mut template_group = None;
                let mut conflicting_groups = false;
                for property in template
                    .properties
                    .iter()
                    .filter(|property| property.name == "unit_type")
                {
                    if template_group.is_some_and(|previous| previous != property.value) {
                        conflicting_groups = true;
                        break;
                    }
                    template_group = Some(property.value.as_str());
                }
                if conflicting_groups || template_group != Some(group) {
                    continue;
                }
                let candidates = result.entry(template.category.clone()).or_default();
                if !candidates.contains(unit_type) {
                    candidates.push(unit_type.clone());
                }
            }
        }
        Ok(result)
    }

    pub fn selected_unit(&self, country: EntityId, category: &str) -> Option<&str> {
        self.unit_selections.get(country, category)
    }

    pub fn clear_selected_unit(&mut self, country: EntityId, category: &str) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "unit selection owner must be a country".into(),
            ));
        }
        validate_identifier(category, "unit category")?;
        self.unit_selections.clear(country, category);
        Ok(())
    }

    pub fn set_selected_unit(
        &mut self,
        country: EntityId,
        category: &str,
        unit_type: &str,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "unit selection owner must be a country".into(),
            ));
        }
        validate_identifier(category, "unit category")?;
        validate_identifier(unit_type, "unit template")?;

        let template = self
            .unit_templates()
            .definitions
            .get(unit_type)
            .ok_or_else(|| Error::Invalid("selected unit template is unknown".into()))?;
        if template.category != category {
            return Err(Error::Invalid(
                "selected unit template has the wrong category".into(),
            ));
        }
        let group = self
            .affiliations()
            .member("unit_type", country)
            .ok_or_else(|| Error::Invalid("country unit type is unknown".into()))?;
        let mut template_group = None;
        for property in template
            .properties
            .iter()
            .filter(|property| property.name == "unit_type")
        {
            if template_group.is_some_and(|previous| previous != property.value) {
                return Err(Error::Invalid(
                    "unit template has conflicting unit groups".into(),
                ));
            }
            template_group = Some(property.value.as_str());
        }
        if template_group != Some(group) {
            return Err(Error::Invalid(
                "selected unit template has the wrong unit group".into(),
            ));
        }
        if !is_enabled(self, country, unit_type)? {
            return Err(Error::Invalid(
                "selected unit template is not enabled by current technology".into(),
            ));
        }

        self.unit_selections.set(country, category, unit_type);
        Ok(())
    }
}

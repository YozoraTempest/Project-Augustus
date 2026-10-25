// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, UnitTemplate, World};

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

fn template_unit_group(template: &UnitTemplate) -> Option<&str> {
    let mut group = None;
    for property in template
        .properties
        .iter()
        .filter(|property| property.name == "unit_type")
    {
        if group.is_some_and(|previous| previous != property.value) {
            return None;
        }
        group = Some(property.value.as_str());
    }
    group
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
                if template_unit_group(template) != Some(group) {
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
        self.unit_selections
            .get(country, category)
            .or_else(|| unique_available_unit_template(self, country, category))
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

fn unique_available_unit_template<'a>(
    world: &'a World,
    country: EntityId,
    category: &str,
) -> Option<&'a str> {
    let group = world.affiliations().member("unit_type", country)?;
    let level = world
        .technology()
        .countries
        .get(&country)?
        .get("military")
        .copied()?;
    let levels = world.technology().tables.get("military")?;
    let available = levels.get(..=usize::from(level))?;
    let mut selected = None;

    for technology in available {
        for unit_type in &technology.enabled_units {
            let Some(template) = world.unit_templates().definitions.get(unit_type) else {
                continue;
            };
            if template.category != category || template_unit_group(template) != Some(group) {
                continue;
            }
            if selected.is_some_and(|previous| previous != unit_type.as_str()) {
                return None;
            }
            selected = Some(unit_type.as_str());
        }
    }

    selected
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AffiliationDimension, Affiliations, EntityKind, Schema, Technology, TechnologyLevel,
        UnitTemplate, UnitTemplateProperty, UnitTemplates, World,
    };

    fn world_with_units(infantry: &[&str]) -> (World, crate::EntityId) {
        let mut world = World::new(Schema::new(std::iter::empty::<String>()).unwrap());
        let country = world.add_entity("AAA", EntityKind::Country);
        let mut affiliations = Affiliations::default();
        affiliations.0.insert(
            "unit_type".into(),
            AffiliationDimension {
                members: BTreeMap::from([(country, Some("western".into()))]),
                ..Default::default()
            },
        );
        world.set_affiliations(affiliations).unwrap();

        let mut enabled_units = infantry
            .iter()
            .map(|unit| (*unit).to_owned())
            .collect::<Vec<_>>();
        enabled_units.push("cavalry_a".into());
        world
            .set_technology(Technology {
                tables: BTreeMap::from([(
                    "military".into(),
                    vec![TechnologyLevel {
                        year: 1444,
                        values_micros: BTreeMap::new(),
                        flags: BTreeMap::new(),
                        enabled_units,
                        unresolved_properties: Default::default(),
                    }],
                )]),
                countries: BTreeMap::from([(country, BTreeMap::from([("military".into(), 0)]))]),
            })
            .unwrap();
        let template = |category: &str| UnitTemplate {
            category: category.into(),
            properties: vec![UnitTemplateProperty {
                name: "unit_type".into(),
                value: "western".into(),
            }],
        };
        let mut definitions = infantry
            .iter()
            .map(|unit| ((*unit).to_owned(), template("infantry")))
            .collect::<BTreeMap<_, _>>();
        definitions.insert("cavalry_a".into(), template("cavalry"));
        world
            .set_unit_templates(UnitTemplates { definitions })
            .unwrap();
        (world, country)
    }

    #[test]
    fn a_unique_available_template_is_the_effective_selection() {
        let (world, country) = world_with_units(&["infantry_a"]);
        assert_eq!(world.selected_unit(country, "infantry"), Some("infantry_a"));
        assert_eq!(world.selected_unit(country, "cavalry"), Some("cavalry_a"));
    }

    #[test]
    fn multiple_available_templates_require_an_explicit_selection() {
        let (mut world, country) = world_with_units(&["infantry_a", "infantry_b"]);
        assert_eq!(world.selected_unit(country, "infantry"), None);
        assert_eq!(world.selected_unit(country, "cavalry"), Some("cavalry_a"));

        world
            .set_selected_unit(country, "infantry", "infantry_b")
            .unwrap();
        assert_eq!(world.selected_unit(country, "infantry"), Some("infantry_b"));
        world.clear_selected_unit(country, "infantry").unwrap();
        assert_eq!(world.selected_unit(country, "infantry"), None);
    }
}

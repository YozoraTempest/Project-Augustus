// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitTemplateProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitTemplate {
    pub category: String,
    pub properties: Vec<UnitTemplateProperty>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitTemplates {
    pub definitions: BTreeMap<String, UnitTemplate>,
}

impl UnitTemplates {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        for (name, template) in &self.definitions {
            if name.trim().is_empty()
                || template.category.trim().is_empty()
                || template
                    .category
                    .bytes()
                    .any(|byte| byte.is_ascii_whitespace())
                || template
                    .properties
                    .iter()
                    .any(|property| property.name.is_empty())
            {
                return Err(Error::Invalid("invalid unit template definition".into()));
            }
        }
        Ok(())
    }
}

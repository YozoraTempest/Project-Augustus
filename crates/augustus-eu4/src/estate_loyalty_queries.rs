// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Read-only previews for the daily estate-loyalty update.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaEstateLoyaltyQuery {
    pub country: String,
    #[serde(default)]
    pub estate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEstateLoyaltyEvaluation {
    pub current_loyalty_milli: i64,
    pub target_loyalty_milli: i64,
    pub next_loyalty_milli: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEstateLoyaltyEntry {
    pub estate: String,
    pub active: Option<bool>,
    pub evaluation: Option<VanillaEstateLoyaltyEvaluation>,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaEstateLoyaltyQueryReport {
    pub country: String,
    pub entries: Vec<VanillaEstateLoyaltyEntry>,
}

impl crate::VanillaWorld {
    /// Previews one or all registered estates for the next daily loyalty step.
    ///
    /// The query follows `World::update_estate_loyalty`: only active estates
    /// with known current loyalty, target loyalty, and decay rules advance.
    /// It never writes the estate state or advances the world.
    pub fn query_vanilla_estate_loyalty(
        &self,
        query: &VanillaEstateLoyaltyQuery,
    ) -> Result<VanillaEstateLoyaltyQueryReport, crate::Error> {
        let country_entity = self.initial.country_entity(&query.country).ok_or_else(|| {
            crate::Error::message("estate loyalty query", "unknown estate loyalty country")
        })?;
        let states = self
            .initial
            .world()
            .estates()
            .countries
            .get(&country_entity);
        if let Some(estate) = query.estate.as_deref()
            && states.and_then(|states| states.get(estate)).is_none()
        {
            return Err(crate::Error::message(
                "estate loyalty query",
                format!("unknown estate loyalty estate {estate}"),
            ));
        }
        let names = if let Some(estate) = &query.estate {
            vec![estate.clone()]
        } else {
            states
                .map(|states| states.keys().cloned().collect())
                .unwrap_or_default()
        };
        let decay = self.initial.world().estates().rules.loyalty_decay;
        let entries = names
            .into_iter()
            .map(|estate| {
                let state = states
                    .and_then(|states| states.get(&estate))
                    .expect("validated estate query");
                let mut unresolved = Vec::new();
                let evaluation = if state.active != Some(true) {
                    unresolved.push(if state.active == Some(false) {
                        "estate_loyalty:inactive".into()
                    } else {
                        "estate_loyalty:unknown_active".into()
                    });
                    None
                } else {
                    let Some(current) = state.loyalty_milli else {
                        unresolved.push("estate_loyalty:current_unknown".into());
                        return VanillaEstateLoyaltyEntry {
                            estate,
                            active: state.active,
                            evaluation: None,
                            unresolved,
                        };
                    };
                    let Some(target) = state.loyalty_target_milli else {
                        unresolved.push("estate_loyalty:target_unknown".into());
                        return VanillaEstateLoyaltyEntry {
                            estate,
                            active: state.active,
                            evaluation: None,
                            unresolved,
                        };
                    };
                    let Some(decay) = decay else {
                        unresolved.push("estate_loyalty:missing_decay".into());
                        return VanillaEstateLoyaltyEntry {
                            estate,
                            active: state.active,
                            evaluation: None,
                            unresolved,
                        };
                    };
                    match decay.next_loyalty_milli(current, target) {
                        Ok(next) => Some(VanillaEstateLoyaltyEvaluation {
                            current_loyalty_milli: current,
                            target_loyalty_milli: target,
                            next_loyalty_milli: next,
                        }),
                        Err(error) => {
                            unresolved.push(format!("estate_loyalty:{error}"));
                            None
                        }
                    }
                };
                VanillaEstateLoyaltyEntry {
                    estate,
                    active: state.active,
                    evaluation,
                    unresolved,
                }
            })
            .collect();
        Ok(VanillaEstateLoyaltyQueryReport {
            country: query.country.clone(),
            entries,
        })
    }
}

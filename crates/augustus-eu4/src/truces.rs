// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{Calendar, InitialWorld, Truce, Truces, WorldDate};

use crate::{Error, FileEntry, SetupReport};
use crate::{LoadedSource, SourceLocation};

#[derive(Debug, Clone)]
pub(super) struct HistoricalTruceGrant {
    pub(super) country: String,
    pub(super) target: String,
    pub(super) date: WorldDate,
    pub(super) source: LoadedSource,
}

fn with_history_source<T>(
    source: &LoadedSource,
    result: Result<T, augustus_core::Error>,
) -> Result<T, Error> {
    result.map_err(|error| {
        Error::caused_by(&source.path, error).with_location(SourceLocation {
            line: source.line,
            column: source.column,
        })
    })
}

pub(super) fn bind(
    files: &[FileEntry],
    defines: &crate::Defines,
    report: &mut SetupReport,
) -> Result<(), Error> {
    if !files
        .iter()
        .any(|file| file.relative_path == std::path::Path::new("common/defines.lua"))
    {
        return Ok(());
    }
    if let Some((value, source)) = defines.optional("NDiplomacy", "TRUCE_YEARS")? {
        if value % 1_000_000 != 0 || value <= 0 {
            return Err(Error::message(
                &source.path,
                "TRUCE_YEARS requires a positive integer",
            ));
        }
        let months = u32::try_from(value / 1_000_000)
            .ok()
            .and_then(|years| years.checked_mul(12))
            .ok_or_else(|| {
                Error::message(
                    &source.path,
                    "TRUCE_YEARS exceeds the supported calendar range",
                )
            })?;
        report.truce_months = Some(months);
        report
            .definition_sources
            .insert("truce:default_months".into(), source);
    }
    Ok(())
}

pub(super) fn bind_history(
    initial: &mut InitialWorld,
    grants: &[HistoricalTruceGrant],
    truce_months: Option<u32>,
    report: &mut crate::BootstrapReport,
) -> Result<(), Error> {
    if grants.is_empty() {
        return Ok(());
    }
    let Some(months) = truce_months else {
        for grant in grants {
            *report
                .unapplied_country_fields
                .entry("add_truce_with".into())
                .or_default() += 1;
            report
                .unapplied_examples
                .entry("country:add_truce_with".into())
                .or_insert_with(|| grant.source.clone());
        }
        return Ok(());
    };

    let calendar =
        initial.world().calendar().cloned().ok_or_else(|| {
            Error::message("historical country truces", "calendar is unavailable")
        })?;
    let mut grants = grants.to_vec();
    grants.sort_by_key(|grant| {
        (
            grant.date,
            grant.source.path.clone(),
            grant.source.line,
            grant.source.column,
        )
    });

    let mut records = BTreeMap::new();
    for grant in grants {
        let country = initial.country_entity(&grant.country).ok_or_else(|| {
            Error::message(
                &grant.source.path,
                format!("unknown historical truce country {}", grant.country),
            )
            .with_location(SourceLocation {
                line: grant.source.line,
                column: grant.source.column,
            })
        })?;
        let target = initial.country_entity(&grant.target).ok_or_else(|| {
            Error::message(
                &grant.source.path,
                format!("unknown historical truce target {}", grant.target),
            )
            .with_location(SourceLocation {
                line: grant.source.line,
                column: grant.source.column,
            })
        })?;

        let event_calendar = Calendar {
            start: grant.date,
            month_lengths: calendar.month_lengths,
        };
        let expiration_day =
            with_history_source(&grant.source, event_calendar.after_months(0, months))?;
        let expiration_date =
            with_history_source(&grant.source, event_calendar.date(expiration_day))?;
        let retirement_day =
            with_history_source(&grant.source, event_calendar.next_month(expiration_day))?;
        let retirement_date =
            with_history_source(&grant.source, event_calendar.date(retirement_day))?;
        let truce = if retirement_date <= calendar.start {
            None
        } else {
            let expires_on = if expiration_date < calendar.start {
                0
            } else {
                with_history_source(&grant.source, calendar.day_offset(expiration_date))?
            };
            Some(Truce {
                expires_on: Some(expires_on),
                monthly: true,
            })
        };
        let (first, second) = if country <= target {
            (country, target)
        } else {
            (target, country)
        };
        records
            .entry(first)
            .or_insert_with(BTreeMap::new)
            .insert(second, truce);
    }

    initial
        .set_truces(Truces {
            records,
            ..Default::default()
        })
        .map_err(|error| Error::caused_by("historical country truces", error))
}

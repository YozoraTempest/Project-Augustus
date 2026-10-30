// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{DefineValue, Defines, Error, LoadedSource, SetupReport, SourceLocation};

pub(super) fn bind(defines: &Defines, report: &mut SetupReport) {
    let name = "GALLEY_COST";
    let Some(evaluated) = defines.values.get(&format!("NDefines.NMilitary.{name}")) else {
        return;
    };
    let source = evaluated.source.clone();
    report
        .definition_sources
        .insert(format!("naval_construction_cost:{name}"), source.clone());

    let result = match defines.optional_signed("NMilitary", name) {
        Ok(Some(_)) => match &evaluated.value {
            DefineValue::Number(value) => validate_cost_define(name, *value, &source),
            DefineValue::String(_) | DefineValue::Boolean(_) | DefineValue::Unsupported(_) => {
                unreachable!("numeric define validation rejects nonnumeric values before binding")
            }
        },
        Ok(None) => return,
        Err(error) => Err(error),
    };
    match result {
        Ok(milli) => {
            report
                .naval_construction_cost_defines_milli
                .insert(name.into(), milli);
        }
        Err(error) => {
            report
                .naval_construction_cost_define_errors
                .insert(name.into(), error.detail().to_owned());
        }
    }
}

fn validate_cost_define(name: &str, value: f64, source: &LoadedSource) -> Result<i32, Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(invalid_define(name, "must be nonnegative", source));
    }
    if value.fract() != 0.0 {
        return Err(invalid_define(
            name,
            "must be a whole currency amount",
            source,
        ));
    }
    if value > f64::from(i32::MAX / 1_000) {
        return Err(invalid_define(name, "exceeds the i32 milli range", source));
    }
    (value as i32)
        .checked_mul(1_000)
        .ok_or_else(|| invalid_define(name, "exceeds the i32 milli range", source))
}

fn invalid_define(name: &str, detail: &str, source: &LoadedSource) -> Error {
    Error::message(
        &source.path,
        format!("{name}: naval construction cost {detail}"),
    )
    .with_location(SourceLocation {
        line: source.line,
        column: source.column,
    })
}

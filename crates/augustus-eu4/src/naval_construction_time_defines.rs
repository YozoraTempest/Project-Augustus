// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{DefineValue, Defines, Error, LoadedSource, SetupReport, SourceLocation};

pub(super) fn bind(defines: &Defines, report: &mut SetupReport) {
    bind_define(defines, report, "GALLEY_TIME", validate_galley_time);
    bind_define(
        defines,
        report,
        "MIN_RECRUIT_TIME_MODIFIER",
        validate_minimum_modifier,
    );
}

fn bind_define(
    defines: &Defines,
    report: &mut SetupReport,
    name: &'static str,
    validate: fn(&str, f64, &LoadedSource) -> Result<i32, Error>,
) {
    let key = format!("NDefines.NMilitary.{name}");
    let Some(evaluated) = defines.values.get(&key) else {
        return;
    };
    let source = evaluated.source.clone();
    report
        .definition_sources
        .insert(format!("naval_construction_time:{name}"), source.clone());

    let result = match defines.optional("NMilitary", name) {
        Ok(Some(_)) => match &evaluated.value {
            DefineValue::Number(value) => validate(name, *value, &source),
            DefineValue::String(_) | DefineValue::Boolean(_) | DefineValue::Unsupported(_) => {
                unreachable!("numeric define validation rejects nonnumeric values before binding")
            }
        },
        Ok(None) => return,
        Err(error) => Err(error),
    };

    match result {
        Ok(value) => {
            report
                .naval_construction_time_defines
                .insert(name.into(), value);
        }
        Err(error) => {
            report
                .naval_construction_time_define_errors
                .insert(name.into(), error.detail().to_owned());
        }
    }
}

fn validate_galley_time(name: &str, value: f64, source: &LoadedSource) -> Result<i32, Error> {
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 {
        return Err(invalid_define(
            name,
            "must be a positive whole number of days",
            source,
        ));
    }
    if value > f64::from(i32::MAX) {
        return Err(invalid_define(name, "exceeds the i32 day range", source));
    }
    Ok(value as i32)
}

fn validate_minimum_modifier(name: &str, value: f64, source: &LoadedSource) -> Result<i32, Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(invalid_define(name, "must be nonnegative", source));
    }
    let milli = (value * 1_000.0).round();
    if !milli.is_finite() || value != milli / 1_000.0 {
        return Err(invalid_define(name, "must be an exact thousandth", source));
    }
    if milli > f64::from(i32::MAX) {
        return Err(invalid_define(name, "exceeds the i32 milli range", source));
    }
    Ok(milli as i32)
}

fn invalid_define(name: &str, detail: &str, source: &LoadedSource) -> Error {
    Error::message(
        &source.path,
        format!("{name}: naval construction time {detail}"),
    )
    .with_location(SourceLocation {
        line: source.line,
        column: source.column,
    })
}

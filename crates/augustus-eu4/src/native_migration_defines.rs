// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Defines, Error, LoadedSource, SetupReport, SourceLocation};

const NATIVE_MIGRATION_COST_DEFINES: [&str; 3] = [
    "TRIBE_MIGRATION_ADM_COST",
    "TRIBE_MIGRATION_DIP_COST",
    "TRIBE_MIGRATION_MIL_COST",
];

pub(super) fn bind(defines: &Defines, report: &mut SetupReport) {
    for name in NATIVE_MIGRATION_COST_DEFINES {
        let evaluated = defines.values.get(&format!("NDefines.NCountry.{name}"));
        let Some(evaluated) = evaluated else {
            continue;
        };
        let source = evaluated.source.clone();
        report
            .definition_sources
            .insert(format!("native_migration_cost:{name}"), source.clone());

        let result = match defines.optional("NCountry", name) {
            Ok(Some((micros, _))) => validate_cost_define(name, micros, &source),
            Ok(None) => continue,
            Err(error) => Err(error),
        };
        match result {
            Ok(milli) => {
                report
                    .native_migration_cost_defines_milli
                    .insert(name.into(), milli);
            }
            Err(error) => {
                report
                    .native_migration_cost_define_errors
                    .insert(name.into(), error.detail().to_owned());
            }
        }
    }
}

fn validate_cost_define(name: &str, micros: i64, source: &LoadedSource) -> Result<i32, Error> {
    if micros < 0 {
        return Err(invalid_define(name, "must be nonnegative", source));
    }
    if micros % 1_000 != 0 {
        return Err(invalid_define(name, "must be an exact thousandth", source));
    }
    i32::try_from(micros / 1_000)
        .map_err(|_| invalid_define(name, "exceeds the i32 milli range", source))
}

fn invalid_define(name: &str, detail: &str, source: &LoadedSource) -> Error {
    Error::message(
        &source.path,
        format!("{name}: native migration cost {detail}"),
    )
    .with_location(SourceLocation {
        line: source.line,
        column: source.column,
    })
}

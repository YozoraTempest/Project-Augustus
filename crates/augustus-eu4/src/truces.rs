// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{ContentRoot, Error, FileEntry, SetupReport};

pub(super) fn bind(
    root: &ContentRoot,
    files: &[FileEntry],
    report: &mut SetupReport,
) -> Result<(), Error> {
    if !files
        .iter()
        .any(|file| file.relative_path == std::path::Path::new("common/defines.lua"))
    {
        return Ok(());
    }
    if let Some((value, source)) =
        crate::defines::optional_literal(root, files, "NDiplomacy", "TRUCE_YEARS")?
    {
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

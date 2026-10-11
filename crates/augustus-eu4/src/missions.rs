// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use crate::bootstrap::{direct_txt, equal};
use crate::{ContentRoot, Error, FileEntry, TextEncoding};

/// Index names only. Tree eligibility, rewards and series selection are separate.
pub(crate) fn identifiers(
    root: &ContentRoot,
    files: &[FileEntry],
) -> Result<BTreeSet<String>, Error> {
    let mut identifiers = BTreeSet::new();
    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "missions"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        for (key, operator, value) in parsed.reader().fields() {
            if key.read_string().starts_with('@') {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(operator, &fail)?;
            let series = value
                .read_object()
                .map_err(|_| fail("expected mission series block"))?;
            for (key, operator, value) in series.fields() {
                let id = key.read_string();
                if id.starts_with('@') || matches!(id.as_str(), "potential" | "potential_on_load") {
                    continue;
                }
                if value.read_object().is_ok() {
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(operator, &fail)?;
                    if id.is_empty() {
                        return Err(fail("mission declaration requires identifier"));
                    }
                    identifiers.insert(id);
                }
            }
        }
    }
    Ok(identifiers)
}

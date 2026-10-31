// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{COUNTRY_LEDGER_EXPENSE_KEYS, COUNTRY_LEDGER_INCOME_KEYS};
use augustus_eu4::VanillaWorld;
use serde_json::Value;

/// Every numeric value inside CountryLedger is a monetary integer. Keep its
/// existing object and vector layout while preserving those integers through
/// Godot's floating-point JSON parser. Category identifiers remain map keys.
fn exact_money(value: &mut Value) {
    match value {
        Value::Number(number) => *value = Value::String(number.to_string()),
        Value::Array(values) => values.iter_mut().for_each(exact_money),
        Value::Object(values) => values.values_mut().for_each(exact_money),
        _ => {}
    }
}

/// Reads the current ledger without consolidating, rolling over or flushing.
/// The query date is not a timestamp for any of the ledger's stored vectors.
pub(crate) fn country_ledger_view(loaded: &VanillaWorld, country: &str) -> Result<Value, String> {
    let entity = loaded
        .initial
        .country_entity(country)
        .ok_or("unknown ledger country")?;
    let world = loaded.initial.world();
    let mut ledger = serde_json::to_value(world.country_ledgers().get(&entity))
        .map_err(|error| error.to_string())?;
    exact_money(&mut ledger);
    Ok(serde_json::json!({
        "country": country,
        "day": world.day().to_string(),
        "date": world.date().map_err(|error| error.to_string())?,
        "ledger": ledger,
        "income_keys": COUNTRY_LEDGER_INCOME_KEYS.as_slice(),
        "expense_keys": COUNTRY_LEDGER_EXPENSE_KEYS.as_slice(),
    }))
}

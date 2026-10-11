// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::Error;

/// Explicit apportionment policy supplied by the content translator or a native system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstateLandRules {
    pub crown_influence_milli: i64,
    pub minimum_crown_share_micros: i64,
    pub initial_share_quantum_micros: i64,
    pub normalized_share_quantum_micros: i64,
}

impl EstateLandRules {
    /// Inputs and outputs retain declaration order. Equal largest weights resolve
    /// to the first declaration; no identifier sorting changes that choice.
    pub fn distribute(&self, influence_milli: &[i64]) -> Result<Vec<i64>, Error> {
        const ONE: i128 = 1_000_000;
        let initial = self.initial_share_quantum_micros;
        let normalized = self.normalized_share_quantum_micros;
        if self.crown_influence_milli < 0
            || !(0..=1_000_000).contains(&self.minimum_crown_share_micros)
            || initial <= 0
            || normalized <= 0
            || 1_000_000 % initial != 0
            || initial % normalized != 0
            || self.minimum_crown_share_micros % normalized != 0
            || influence_milli.iter().any(|v| *v < 0)
        {
            return Err(Error::Invalid(
                "invalid estate land distribution inputs".into(),
            ));
        }
        let total = influence_milli
            .iter()
            .fold(i128::from(self.crown_influence_milli), |sum, v| {
                sum + i128::from(*v)
            });
        if total == 0 {
            return Ok(vec![0; influence_milli.len()]);
        }
        let mut shares: Vec<i64> = influence_milli
            .iter()
            .map(|v| (i128::from(*v) * ONE / total / i128::from(initial)) as i64 * initial)
            .collect();
        let sum: i64 = shares.iter().sum();
        let available = 1_000_000 - self.minimum_crown_share_micros;
        if sum <= available {
            return Ok(shares);
        }
        let mut largest = 0;
        for i in 1..influence_milli.len() {
            if influence_milli[i] > influence_milli[largest] {
                largest = i;
            }
        }
        let divisor = i128::from(sum) * i128::from(normalized);
        for share in &mut shares {
            let numerator = i128::from(*share) * i128::from(available);
            *share = ((numerator + divisor - 1) / divisor) as i64 * normalized;
        }
        let excess: i64 = shares.iter().sum::<i64>() - available;
        shares[largest] = shares[largest]
            .checked_sub(excess)
            .filter(|v| *v >= 0)
            .ok_or_else(|| Error::Invalid("estate rounding exceeds largest allocation".into()))?;
        Ok(shares)
    }
}

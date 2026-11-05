// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ManpowerRules {
    pub base_capacity: u64,
    pub per_development: u64,
    pub recovery_months: u32,
    pub minimum_monthly: u64,
    pub infantry_cost: u64,
}

/// 只保存可变储备；上限和恢复速度由领土及规则实时计算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManpowerPool {
    pub(crate) available: u64,
    // 以恢复周期为分母保存不足一人的余量，跨月、存读档均不会丢失。
    pub(crate) recovery_remainder: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManpowerSummary {
    pub available: u64,
    pub capacity: u64,
    pub monthly_recovery: u64,
}

impl ManpowerPool {
    pub(crate) fn full(capacity: u64) -> Self {
        Self {
            available: capacity,
            recovery_remainder: 0,
        }
    }

    pub(crate) fn monthly_recovery(&self, capacity: u64, rules: &ManpowerRules) -> u64 {
        let months = u64::from(rules.recovery_months);
        let accrued =
            capacity.max(rules.minimum_monthly * months) + u64::from(self.recovery_remainder);
        accrued / months
    }

    pub(crate) fn recover(&mut self, capacity: u64, rules: &ManpowerRules) {
        if self.available >= capacity {
            self.available = capacity;
            self.recovery_remainder = 0;
            return;
        }
        let months = u64::from(rules.recovery_months);
        let accrued =
            capacity.max(rules.minimum_monthly * months) + u64::from(self.recovery_remainder);
        self.available = (self.available + accrued / months).min(capacity);
        self.recovery_remainder = if self.available == capacity {
            0
        } else {
            (accrued % months) as u32
        };
    }
}

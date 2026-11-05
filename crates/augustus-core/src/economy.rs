// SPDX-License-Identifier: GPL-3.0-only

use rust_decimal::RoundingStrategy;
use serde::{Deserialize, Serialize};

use crate::{GameDate, Money};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthlyTaxSettlement {
    /// 税额所属月份，以该月第一天表示；实际入库发生在下月第一天。
    pub month: GameDate,
    pub settled_on: GameDate,
    pub received: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountryFinances {
    treasury: Money,
    last_settlement: Option<MonthlyTaxSettlement>,
    // 以年税额为单位保存除以 12 后的余量，后续月份补入，避免逐月舍入丢钱。
    tax_remainder: Money,
}

impl CountryFinances {
    pub fn treasury(&self) -> Money {
        self.treasury
    }

    pub fn last_settlement(&self) -> Option<&MonthlyTaxSettlement> {
        self.last_settlement.as_ref()
    }

    // World 在接受建设项目之前统一检查余额，扣款与项目创建在同一次操作中完成。
    pub(crate) fn spend(&mut self, amount: Money) {
        self.treasury -= amount;
    }

    pub(crate) fn settle_tax(&mut self, annual_tax: Money, settled_on: GameDate) {
        let months = Money::from(12);
        let accrued = annual_tax + self.tax_remainder;
        // 国库精确到千分之一金币；用十进制计算并保留余量，连续十二个月还原全年税额。
        let received = (accrued / months).round_dp_with_strategy(3, RoundingStrategy::ToZero);
        self.tax_remainder = accrued - received * months;
        self.treasury += received;
        let previous_day = settled_on.add_days(-1);
        self.last_settlement = Some(MonthlyTaxSettlement {
            month: GameDate::new(previous_day.year(), previous_day.month(), 1)
                .expect("结算月份必须有效"),
            settled_on,
            received,
        });
    }
}

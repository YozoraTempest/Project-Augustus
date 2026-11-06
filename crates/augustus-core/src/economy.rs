// SPDX-License-Identifier: GPL-3.0-only

use rust_decimal::RoundingStrategy;
use serde::{Deserialize, Serialize};

use crate::{GameDate, Money};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthlySettlement {
    /// 收支所属月份，以该月第一天表示；实际扣账发生在下月第一天。
    pub month: GameDate,
    pub settled_on: GameDate,
    #[serde(alias = "received")]
    pub tax: Money,
    // 旧存档只结算过税收，其历史军费为零，不追溯扣款。
    #[serde(default)]
    pub army_maintenance: Money,
    /// 本次结算计入总额的超额维护。旧档没有此项，视为没有超编费用。
    #[serde(default)]
    pub excess_maintenance: Money,
}

impl MonthlySettlement {
    pub fn balance(&self) -> Money {
        self.tax - self.army_maintenance
    }

    pub fn base_maintenance(&self) -> Money {
        self.army_maintenance - self.excess_maintenance
    }
}

/// 当前世界状态对应的月度预算，与已发生的结算记录分开。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyBudget {
    pub tax: Money,
    /// 基础军费与超额维护的月度合计，与结算使用的年费一致。
    pub army_maintenance: Money,
    pub base_army_maintenance: Money,
    pub excess_army_maintenance: Money,
}

impl MonthlyBudget {
    pub fn balance(&self) -> Money {
        self.tax - self.army_maintenance
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountryFinances {
    treasury: Money,
    last_settlement: Option<MonthlySettlement>,
    // 以年税额为单位保存除以 12 后的余量，后续月份补入，避免逐月舍入丢钱。
    tax_remainder: Money,
    #[serde(default)]
    maintenance_remainder: Money,
    /// 超额维护单独保留余量，十二个月合计等于当年超额年费。
    #[serde(default)]
    excess_remainder: Money,
}

impl CountryFinances {
    pub fn treasury(&self) -> Money {
        self.treasury
    }

    pub fn last_settlement(&self) -> Option<&MonthlySettlement> {
        self.last_settlement.as_ref()
    }

    // World 在接受建设项目之前统一检查余额，扣款与项目创建在同一次操作中完成。
    pub(crate) fn spend(&mut self, amount: Money) {
        self.treasury -= amount;
    }

    pub(crate) fn refund(&mut self, amount: Money) {
        self.treasury += amount;
    }

    pub(crate) fn settle(
        &mut self,
        annual_tax: Money,
        annual_base_maintenance: Money,
        annual_excess_maintenance: Money,
        settled_on: GameDate,
    ) {
        let tax = monthly_amount(annual_tax, &mut self.tax_remainder);
        let base_maintenance =
            monthly_amount(annual_base_maintenance, &mut self.maintenance_remainder);
        let excess_maintenance =
            monthly_amount(annual_excess_maintenance, &mut self.excess_remainder);
        let army_maintenance = base_maintenance + excess_maintenance;
        // 固定支出完整记账，余额不足时允许国库为负，不能让缺钱的军队免交维护费。
        self.treasury += tax - army_maintenance;
        let previous_day = settled_on.add_days(-1);
        self.last_settlement = Some(MonthlySettlement {
            month: GameDate::new(previous_day.year(), previous_day.month(), 1)
                .expect("结算月份必须有效"),
            settled_on,
            tax,
            army_maintenance,
            excess_maintenance,
        });
    }

    /// 下一次月初结算后的国库。日期只写入结算记录，不改变金额。
    pub(crate) fn treasury_after_next_settlement(
        &self,
        annual_tax: Money,
        annual_base_maintenance: Money,
        annual_excess_maintenance: Money,
    ) -> Money {
        let mut finances = self.clone();
        finances.settle(
            annual_tax,
            annual_base_maintenance,
            annual_excess_maintenance,
            GameDate::new(1444, 1, 1).expect("日期有效"),
        );
        finances.treasury()
    }
}

fn monthly_amount(annual: Money, remainder: &mut Money) -> Money {
    let months = Money::from(12);
    let accrued = annual + *remainder;
    // 国库精确到千分之一金币，保留余量使十二个月的累计金额等于全年金额。
    let amount = (accrued / months).round_dp_with_strategy(3, RoundingStrategy::ToZero);
    *remainder = accrued - amount * months;
    amount
}

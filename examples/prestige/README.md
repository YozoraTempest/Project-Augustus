# 声望与实时数值反馈

这是自制场景，包含两个虚构国家和一个省份。AAA 的初始声望为 12.001，BBB 为 5.000。场景显式提供每年 1 点增长与 5% 基础衰减，并仅让拥有省份的国家参与变化；这些输入不是原版开局生成算法。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/prestige --trigger prestige.begin --save prestige_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/prestige --input native --load prestige_pending.json --choose 0:0 --save prestige_waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/prestige --load prestige_waiting.json --days 20 --save prestige_month.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/prestige --input native --load prestige_month.json --choose 1:0 --days 31 --save prestige_negative.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/prestige --load prestige_negative.json --choose 2:0 --days 1 --choose 3:0
```

`add_prestige` 与 `prestige` 使用共享的千分位指标，逐字面截断，允许负值，增减限制在 −100～100。条件也支持国家、国家类型的 ROOT、皇帝与已声明类型的全局目标之间的实时比较；未知值会报告错误，不以零替代。

场景先核对精度、上下界和国家比较，再将 AAA 设为 50。月初事件先看到 50，随后月结得到 49.875。下一阶段将声望设为 −50，并添加每年 1.2 点增长、−2% 衰减的自制修正；月结读取当前声望和修正，得到 −49.525。BBB 没有省份，保持原值。余额、修正、延期与待选项可以在两种输入之间交叉保存恢复。

原生属性使用 `Numeric` 读取权威数值，`Scale` 缩放整组贡献，`Conditional` 按当前条件选择来源。缩放按组汇总后截断，并保留各来源及舍入差；未选择的分支不读取状态，但全部分支仍须通过结构校验。数值与条件不能引用其他派生属性，避免循环依赖。属性结果交给已有月度指标结算，核心不保存一份声望副本。

本示例的年度增长与基础衰减由场景提供。完整原版初始声望、所有年度来源及其更新顺序仍未完成；未知原版初值保持未知。

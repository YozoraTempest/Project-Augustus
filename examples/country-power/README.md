# 国家点数库存

这个自制场景演示点数增减、负值下限、可增长上限，以及超额库存的保留。初始行政点数为 1200，上限为 999；再加一点仍是 1200，扣掉 250 后变为 950，次日补点最多到 999。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-power --trigger power.begin --choose 0:0 --save power-waiting.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/country-power --input native --load power-waiting.json --days 1 --choose 1:0
```

第一次运行后的行政、外交、军事点数分别为 950、999、50；继续一天后为 999、998、50。EU4 写法与原生 JSON 使用相同的核心库存和存档。脚本中的小数向零截断，当前仅接受最多三位小数的字面数值。

场景用 `metrics.entities` 声明库存，用 `metrics.ceilings` 声明各实体的增长上限，定义中的 `growth_ceiling = true` 启用此规则。`minimum` 和 `maximum` 是库存可保存的数值范围。改变增长上限不修改已有库存；未知库存不能进行运算，未知上限不能执行需要增长的操作。系统实现可调用 `World::set_metric_ceiling` 更新上限。

这些初值由场景明确提供。原版开局的点数初始化、月收入、动态上限来源、科技购买和其他点数消费尚未接入。

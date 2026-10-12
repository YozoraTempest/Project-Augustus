# 停用政体指标的读取

自制场景中的 `BBB` 没有活动正统性，仍明确保存了数值 100。EU4 脚本与原生计划共用这份状态；查询可以读取保留值，`add_legitimacy` 不会修改停用指标。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/government-stored-values --input eu4 --entity 1 \
  --trigger stored.start --choose 0:0 --save stored.json

cargo run --locked -p augustus-run --bin augustus-events -- \
  --content examples/government-stored-values --input native \
  --load stored.json --days 1 --choose 1:0
```

日志依次为 `retained`、`owner`、`after`、`later`。即时查询、首都所有者和延期事件读取同一个保存值。

`GovernmentState.inactive_metrics` 保存停用指标，空集合省略。`GovernmentMetricStored` 检查活动或停用存储项是否存在；只读数值槽 `StoredGovernment` 读取它们。`GovernmentMetricActive` 和 `Government` 数值槽继续只处理活动指标。停用值不贡献属性；同一指标不能同时出现在活动与停用集合中。明确未知的值或未知选择会报错。

本场景不执行政体转换。政体切换、改革回调及其他政府指标仍待接入。

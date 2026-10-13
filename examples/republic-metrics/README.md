# 共和传统与奉献值

这个自制场景用同一个世界状态处理政体切换、指标增减和事件。AAA 由玩家控制，BBB 由 AI 控制；两种输入分别使用 EU4 写法和原生计划。

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/republic-metrics --trigger institutions.start --save institutions_started.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/republic-metrics --input native --load institutions_started.json --choose 0:0 --days 2 --save institutions_later.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/republic-metrics --load institutions_later.json --choose 1:0 --save institutions_chosen.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/republic-metrics --load institutions_chosen.json --trigger institutions.return --choose 2:0
```

开始事件切换到共和国，将传统明确设到 10.125；接受选项后为 13。两天后的事件切换到神权，将奉献值设到 7.125，接受后为 10。此时传统保留在停用存储中，其增减效果不改变保留值。返回共和国后，传统至少为内容声明的进入下限；本例为 50，奉献值 10 继续保留。

将第一次命令改为 `--entity 2` 可以运行 BBB 的 AI 分支；AI 会处理选项，不用传入 `--choose`。使用两天推进再触发 `institutions.return` 即可完成同一流程。已接受的 AI 命令也可以通过核心接口在执行前保存和恢复，见 [AI 事件选项](../event-decisions/README.md)。

兼容输入支持 `add_republican_tradition`、`add_devotion` 的字面增减，精度到小数后三位，使用现有的 0 至 100 上下限。效果只修改活动指标；查询可以读取明确保留的停用值。活动值或政体选择未知时继续报告未知，不用零代替。

原生内容直接声明指标范围，并使用通用 `UpdateNumeric`。`GovernmentForm.retain_higher_metrics` 指定跨类型进入时保留较大值的指标：`metrics` 中的进入值作为下限，明确未知的保留值仍未知。字段省略时按已有固定进入值处理，同类型转换保留返回后的当前状态。EU4 的普通共和国使用传统下限 50，神权的奉献值仍采用固定进入值 50。

原版历史开局初值、自然增长、完整改革生命周期及属性缩放尚未接通；本例明确提供初始状态。新翻译规则会改变原版世界的内容指纹，已有存档仍须匹配原来的有效内容，不跨内容加载。
